//! Fachada `Emissor`: journal, idempotência, controles de produção e
//! integração entre cofre, DPS e transporte.
//!
//! O fluxo é sempre em duas fases:
//!
//! 1. [`Emissor::preparar_nfse`] valida a DPS, calcula o hash do conteúdo e
//!    persiste a operação no estado `preparada`;
//! 2. [`Emissor::confirmar_nfse`] assina, reivindica o envio de forma atômica e
//!    chama a Sefin uma única vez.
//!
//! Repetir a preparação com o mesmo Id e o mesmo conteúdo devolve a operação
//! existente. Reutilizar o Id com conteúdo diferente retorna
//! [`ErroEmissor::ConflitoIdempotencia`]. Um resultado desconhecido nunca é
//! reenviado às cegas: [`Emissor::reconciliar_nfse`] consulta o estado na Sefin
//! antes de qualquer nova tentativa.

use crate::erro::ErroEmissor;
use crate::journal::{EstadoOperacao, Journal, Operacao, TipoOperacao, hash_conteudo};
use carimbar_cert::CertificadoA1;
use carimbar_dsig::assinar;
use carimbar_nfse::{
    CacheParametros, ChaveNfse, ClienteAdn, ClienteParametros, ClienteSefin, Dps, ErroFiscal,
    EventoCancelamento, Executor, IdDps, LoteDistribuicao, ParametrosConvenio, Rejeicao,
    ResultadoEmissao, ResultadoEvento, StatusDistribuicao,
};
use carimbar_types::{Ambiente, Cnpj, ConfirmacaoProducao, TpAmb};
use carimbar_vault::{Cofre, MetadadosCertificado};
use carimbar_xml::Documento;

/// Política do kill switch de produção.
pub trait KillSwitch: Send + Sync {
    /// `true` quando operações de produção são permitidas.
    fn producao_permitida(&self) -> bool;
}

/// Kill switch que lê `CARIMBAR_PRODUCAO` a cada operação.
///
/// A ausência da variável libera produção; os valores `0`, `false`, `nao` e
/// `não` bloqueiam. O CI usa `0`.
#[derive(Debug, Clone, Copy, Default)]
pub struct KillSwitchAmbiente;

impl KillSwitch for KillSwitchAmbiente {
    fn producao_permitida(&self) -> bool {
        match std::env::var("CARIMBAR_PRODUCAO") {
            Ok(valor) => {
                let valor = valor.trim().to_ascii_lowercase();
                !matches!(valor.as_str(), "0" | "false" | "nao" | "não")
            }
            Err(_) => true,
        }
    }
}

/// Kill switch fixo, para testes e processos com política própria.
#[derive(Debug, Clone, Copy)]
pub struct KillSwitchFixo(pub bool);

impl KillSwitch for KillSwitchFixo {
    fn producao_permitida(&self) -> bool {
        self.0
    }
}

/// Operação preparada e persistida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperacaoPreparada {
    /// Operação persistida.
    pub operacao: Operacao,
    /// `true` quando esta chamada criou a operação.
    pub nova: bool,
}

impl OperacaoPreparada {
    /// Identificador local.
    pub fn id(&self) -> i64 {
        self.operacao.id
    }

    /// Estado atual.
    pub fn estado(&self) -> EstadoOperacao {
        self.operacao.estado
    }
}

/// Resultado de uma operação de NFS-e.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultadoNfse {
    /// NFS-e autorizada, com XML preservado para auditoria e DANFSe.
    Autorizada {
        /// Operação local.
        operacao: i64,
        /// Chave de acesso.
        chave: ChaveNfse,
        /// XML da NFS-e autorizada.
        xml_nfse: String,
    },
    /// Rejeição fiscal.
    Rejeitada {
        /// Operação local.
        operacao: i64,
        /// Erros fiscais, quando disponíveis.
        erros: Vec<ErroFiscal>,
        /// Diagnóstico textual.
        detalhe: String,
    },
    /// Resultado não lido; consulte antes de reenviar.
    Desconhecido {
        /// Operação local.
        operacao: i64,
        /// Diagnóstico.
        detalhe: String,
    },
    /// A reconciliação confirmou que a operação não foi processada.
    NaoEncontrada {
        /// Operação local.
        operacao: i64,
        /// Diagnóstico.
        detalhe: String,
    },
    /// Cancelamento registrado.
    Cancelada {
        /// Operação local.
        operacao: i64,
        /// XML do evento.
        xml_evento: String,
    },
}

/// Resultado de uma rodada de sincronização do ADN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoSincronizacao {
    /// Cursor no início da rodada.
    pub inicio_nsu: u64,
    /// Maior NSU persistido ao final.
    pub ultimo_nsu: u64,
    /// Documentos novos inseridos.
    pub documentos_novos: usize,
    /// Páginas consultadas.
    pub paginas: usize,
}

/// Fachada de emissão de NFS-e para uma empresa.
///
/// O `Emissor` possui o cofre, o journal e uma referência ao executor de
/// transporte. O executor é a única porta para a rede e pode ser substituído
/// por uma implementação simulada nos testes.
pub struct Emissor<'a> {
    empresa: Cnpj,
    cofre: Cofre,
    journal: Journal,
    executor: &'a dyn Executor,
    kill_switch: Box<dyn KillSwitch>,
    certificado_id: Option<String>,
    cache_parametros: CacheParametros,
}

impl std::fmt::Debug for Emissor<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Emissor")
            .field("empresa", &self.empresa)
            .field("ambiente", &self.ambiente())
            .field("certificado_id", &self.certificado_id)
            .finish_non_exhaustive()
    }
}

impl<'a> Emissor<'a> {
    /// Cria a fachada para a empresa, usando o kill switch do ambiente.
    pub fn novo(
        cofre: Cofre,
        journal: Journal,
        empresa: Cnpj,
        executor: &'a dyn Executor,
    ) -> Result<Self, ErroEmissor> {
        Self::com_kill_switch(
            cofre,
            journal,
            empresa,
            executor,
            Box::new(KillSwitchAmbiente),
        )
    }

    /// Cria a fachada com uma política de kill switch explícita.
    pub fn com_kill_switch(
        cofre: Cofre,
        journal: Journal,
        empresa: Cnpj,
        executor: &'a dyn Executor,
        kill_switch: Box<dyn KillSwitch>,
    ) -> Result<Self, ErroEmissor> {
        journal.garantir_empresa(&empresa)?;
        let certificado_id = journal.certificado_producao(&empresa)?;
        Ok(Self {
            empresa,
            cofre,
            journal,
            executor,
            kill_switch,
            certificado_id,
            cache_parametros: CacheParametros::padrao(),
        })
    }

    /// Empresa emitente.
    pub fn empresa(&self) -> &Cnpj {
        &self.empresa
    }

    /// Ambiente atual, derivado do journal e da promoção explícita.
    pub fn ambiente(&self) -> Ambiente {
        match self.journal.ambiente_empresa(&self.empresa) {
            Ok(TpAmb::Producao) => Ambiente::producao(ConfirmacaoProducao::new(self.empresa)),
            _ => Ambiente::Homologacao,
        }
    }

    /// Journal subjacente, para consulta de operações e auditoria.
    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Importa um certificado A1 para a empresa.
    pub fn importar_certificado(
        &mut self,
        id: &str,
        pfx: &[u8],
        senha: &str,
    ) -> Result<MetadadosCertificado, ErroEmissor> {
        self.journal.garantir_empresa(&self.empresa)?;
        Ok(self.cofre.importar(&self.empresa, id, pfx, senha)?)
    }

    /// Lista os certificados públicos da empresa.
    pub fn listar_certificados(&self) -> Result<Vec<MetadadosCertificado>, ErroEmissor> {
        Ok(self.cofre.listar(&self.empresa)?)
    }

    /// Seleciona o certificado usado nas próximas assinaturas.
    pub fn selecionar_certificado(&mut self, id: impl Into<String>) {
        self.certificado_id = Some(id.into());
    }

    /// Promove a empresa para produção, vinculando a confirmação ao CNPJ.
    ///
    /// Exige o kill switch liberado e um certificado pertencente à empresa.
    /// A promoção é uma ação administrativa explícita; não é feita pelo MCP.
    pub fn promover_producao(
        &mut self,
        confirmacao: ConfirmacaoProducao,
        certificado_id: &str,
    ) -> Result<(), ErroEmissor> {
        if confirmacao.cnpj() != &self.empresa {
            return Err(ErroEmissor::ConfirmacaoDivergente(*confirmacao.cnpj()));
        }
        if !self.kill_switch.producao_permitida() {
            return Err(ErroEmissor::ProducaoBloqueada);
        }
        let certificado = self.cofre.certificado(&self.empresa, certificado_id)?;
        if certificado.cnpj() != &self.empresa {
            return Err(ErroEmissor::CertificadoDeOutraEmpresa);
        }
        self.journal
            .promover_empresa(&self.empresa, certificado_id)?;
        self.certificado_id = Some(certificado_id.to_owned());
        Ok(())
    }

    /// Devolve a empresa para homologação.
    pub fn rebaixar_homologacao(&mut self) -> Result<(), ErroEmissor> {
        self.journal.rebaixar_empresa(&self.empresa)?;
        self.certificado_id = None;
        Ok(())
    }

    /// Prepara uma DPS para emissão.
    ///
    /// Não assina nem envia. A operação fica persistida no estado
    /// `preparada`; repetir com o mesmo conteúdo devolve a operação existente.
    pub fn preparar_nfse(&mut self, dps: &Dps) -> Result<OperacaoPreparada, ErroEmissor> {
        let ambiente = self.ambiente().tp_amb();
        if dps.ambiente != ambiente {
            return Err(ErroEmissor::Divergencia { campo: "dps.tpAmb" });
        }
        if dps.prestador.inscricao.cnpj() != Some(&self.empresa) {
            return Err(ErroEmissor::Divergencia {
                campo: "dps.prestador.inscricao",
            });
        }
        let xml = dps.para_xml()?;
        let chave = dps.id()?;
        let hash = hash_conteudo(&xml);
        let (operacao, nova) = self.journal.criar(
            &self.empresa,
            ambiente,
            TipoOperacao::EmitirNfse,
            chave.as_str(),
            &hash,
            &xml,
        )?;
        Ok(OperacaoPreparada { operacao, nova })
    }

    /// Confirma a operação, assinando e enviando no máximo uma vez.
    ///
    /// Estados já autorizados ou rejeitados são devolvidos sem rede. Estados de
    /// envio pendente são reconciliados, nunca reenviados às cegas.
    pub fn confirmar_nfse(&mut self, id: i64) -> Result<ResultadoNfse, ErroEmissor> {
        let operacao = self.journal.operacao(id)?;
        self.conferir_operacao(&operacao, TipoOperacao::EmitirNfse)?;
        match operacao.estado {
            EstadoOperacao::Autorizada => return self.resultado_autorizada(&operacao),
            EstadoOperacao::Rejeitada => return Ok(self.resultado_rejeitada(&operacao)),
            EstadoOperacao::Enviada | EstadoOperacao::Desconhecida => {
                return self.reconciliar_nfse(id);
            }
            EstadoOperacao::Preparada
            | EstadoOperacao::FalhaAntesDoEnvio
            | EstadoOperacao::NaoEncontrada => {}
            EstadoOperacao::Cancelada => {
                return Err(ErroEmissor::EstadoInvalido {
                    id,
                    estado: operacao.estado.as_str(),
                    acao: "confirmar emissão",
                });
            }
        }
        self.exigir_producao_permitida()?;
        let certificado = self.certificado_atual()?;
        let documento = Documento::parse(operacao.xml_preparado.as_bytes())?;
        let assinado = assinar(&documento, &operacao.chave_operacao, &certificado)?;
        let xml = assinado.to_xml()?;
        if !self.journal.reivindicar_envio(id, &xml)? {
            let atual = self.journal.operacao(id)?;
            return match atual.estado {
                EstadoOperacao::Autorizada => self.resultado_autorizada(&atual),
                EstadoOperacao::Rejeitada => Ok(self.resultado_rejeitada(&atual)),
                EstadoOperacao::Enviada | EstadoOperacao::Desconhecida => {
                    Err(ErroEmissor::EmAndamento(id))
                }
                _ => Err(ErroEmissor::EmAndamento(id)),
            };
        }

        let cliente = ClienteSefin::new(operacao.ambiente, self.executor);
        match cliente.emitir_xml(&xml) {
            Ok(ResultadoEmissao::Autorizada(autorizada)) => {
                self.journal.marcar_autorizada(
                    id,
                    autorizada.chave.as_str(),
                    &autorizada.xml_nfse,
                    autorizada.data_hora_processamento.as_deref(),
                )?;
                Ok(ResultadoNfse::Autorizada {
                    operacao: id,
                    chave: autorizada.chave,
                    xml_nfse: autorizada.xml_nfse,
                })
            }
            Ok(ResultadoEmissao::Rejeitada(rejeicao)) => {
                let detalhe = formatar_rejeicao(&rejeicao);
                self.journal.marcar_rejeitada(id, &detalhe)?;
                Ok(ResultadoNfse::Rejeitada {
                    operacao: id,
                    erros: rejeicao.erros,
                    detalhe,
                })
            }
            Ok(ResultadoEmissao::Duplicidade(rejeicao)) => {
                self.journal.marcar_desconhecida(
                    id,
                    &format!("duplicidade: {}", formatar_rejeicao(&rejeicao)),
                )?;
                self.reconciliar_nfse(id)
            }
            Ok(ResultadoEmissao::Desconhecido { detalhe }) => {
                self.journal.marcar_desconhecida(id, &detalhe)?;
                Ok(ResultadoNfse::Desconhecido {
                    operacao: id,
                    detalhe,
                })
            }
            Err(carimbar_nfse::ErroNfse::AntesDoEnvio(erro)) => {
                let detalhe = erro.to_string();
                self.journal.marcar_falha_antes_envio(id, &detalhe)?;
                Err(ErroEmissor::Nfse(carimbar_nfse::ErroNfse::AntesDoEnvio(
                    erro,
                )))
            }
            Err(erro) => {
                self.journal.marcar_desconhecida(id, &erro.to_string())?;
                Err(ErroEmissor::Nfse(erro))
            }
        }
    }

    /// Consolida a emissão: prepara e confirma.
    pub fn executar_nfse(&mut self, dps: &Dps) -> Result<ResultadoNfse, ErroEmissor> {
        let preparada = self.preparar_nfse(dps)?;
        self.confirmar_nfse(preparada.id())
    }

    /// Reconcilia uma operação enviada ou com resultado desconhecido.
    ///
    /// Consulta a DPS na Sefin e, se houver NFS-e, busca e persiste o XML
    /// autorizado. Se a DPS não existir, marca a operação como não encontrada e
    /// só então um novo `confirmar_nfse` pode reenviar.
    pub fn reconciliar_nfse(&mut self, id: i64) -> Result<ResultadoNfse, ErroEmissor> {
        let operacao = self.journal.operacao(id)?;
        self.conferir_operacao(&operacao, TipoOperacao::EmitirNfse)?;
        match operacao.estado {
            EstadoOperacao::Autorizada => return self.resultado_autorizada(&operacao),
            EstadoOperacao::Rejeitada => return Ok(self.resultado_rejeitada(&operacao)),
            EstadoOperacao::Enviada | EstadoOperacao::Desconhecida => {}
            EstadoOperacao::Preparada
            | EstadoOperacao::FalhaAntesDoEnvio
            | EstadoOperacao::NaoEncontrada
            | EstadoOperacao::Cancelada => {
                return Err(ErroEmissor::EstadoInvalido {
                    id,
                    estado: operacao.estado.as_str(),
                    acao: "reconciliar",
                });
            }
        }
        let id_dps = IdDps::parse(&operacao.chave_operacao, operacao.ambiente)?;
        let cliente = ClienteSefin::new(operacao.ambiente, self.executor);
        match cliente.consultar_dps(&id_dps) {
            Ok(localizada) => {
                let nfse = cliente.consultar(&localizada.chave)?;
                self.journal.marcar_autorizada(
                    id,
                    localizada.chave.as_str(),
                    &nfse.xml_nfse,
                    None,
                )?;
                Ok(ResultadoNfse::Autorizada {
                    operacao: id,
                    chave: localizada.chave,
                    xml_nfse: nfse.xml_nfse,
                })
            }
            Err(carimbar_nfse::ErroNfse::NaoEncontrada) => {
                let detalhe = "DPS não encontrada na consulta".to_owned();
                self.journal.marcar_nao_encontrada(id, &detalhe)?;
                Ok(ResultadoNfse::NaoEncontrada {
                    operacao: id,
                    detalhe,
                })
            }
            Err(erro) => Err(ErroEmissor::Nfse(erro)),
        }
    }

    /// Consulta uma NFS-e pela chave.
    pub fn consultar_nfse(&self, chave: &ChaveNfse) -> Result<String, ErroEmissor> {
        let cliente = ClienteSefin::new(self.ambiente().tp_amb(), self.executor);
        Ok(cliente.consultar(chave)?.xml_nfse)
    }

    /// Prepara o cancelamento de uma NFS-e.
    pub fn preparar_cancelamento(
        &mut self,
        evento: &EventoCancelamento,
    ) -> Result<OperacaoPreparada, ErroEmissor> {
        let ambiente = self.ambiente().tp_amb();
        if evento.ambiente != ambiente {
            return Err(ErroEmissor::Divergencia {
                campo: "evento.tpAmb",
            });
        }
        if evento.autor.cnpj() != Some(&self.empresa) {
            return Err(ErroEmissor::Divergencia {
                campo: "evento.autor",
            });
        }
        let xml = evento.para_xml()?;
        let chave = evento.id();
        let hash = hash_conteudo(&xml);
        let (operacao, nova) = self.journal.criar(
            &self.empresa,
            ambiente,
            TipoOperacao::CancelarNfse,
            &chave,
            &hash,
            &xml,
        )?;
        Ok(OperacaoPreparada { operacao, nova })
    }

    /// Confirma o cancelamento, assinando e enviando no máximo uma vez.
    pub fn confirmar_cancelamento(&mut self, id: i64) -> Result<ResultadoNfse, ErroEmissor> {
        let operacao = self.journal.operacao(id)?;
        self.conferir_operacao(&operacao, TipoOperacao::CancelarNfse)?;
        match operacao.estado {
            EstadoOperacao::Cancelada => {
                return Ok(ResultadoNfse::Cancelada {
                    operacao: id,
                    xml_evento: operacao.xml_retorno.unwrap_or_default(),
                });
            }
            EstadoOperacao::Rejeitada => return Ok(self.resultado_rejeitada(&operacao)),
            EstadoOperacao::Enviada | EstadoOperacao::Desconhecida => {
                return Err(ErroEmissor::ReconciliacaoNecessaria(id));
            }
            EstadoOperacao::Preparada
            | EstadoOperacao::FalhaAntesDoEnvio
            | EstadoOperacao::NaoEncontrada => {}
            EstadoOperacao::Autorizada => {
                return Err(ErroEmissor::EstadoInvalido {
                    id,
                    estado: operacao.estado.as_str(),
                    acao: "confirmar cancelamento",
                });
            }
        }
        self.exigir_producao_permitida()?;
        let certificado = self.certificado_atual()?;
        let documento = Documento::parse(operacao.xml_preparado.as_bytes())?;
        let assinado = assinar(&documento, &operacao.chave_operacao, &certificado)?;
        let xml = assinado.to_xml()?;
        if !self.journal.reivindicar_envio(id, &xml)? {
            return Err(ErroEmissor::EmAndamento(id));
        }
        let cliente = ClienteSefin::new(operacao.ambiente, self.executor);
        match cliente.registrar_evento_xml(&xml) {
            Ok(ResultadoEvento::Registrado(registrado)) => {
                self.journal.marcar_cancelada(id, &registrado.xml_evento)?;
                Ok(ResultadoNfse::Cancelada {
                    operacao: id,
                    xml_evento: registrado.xml_evento,
                })
            }
            Ok(ResultadoEvento::Rejeitado(rejeicao)) => {
                let detalhe = formatar_rejeicao(&rejeicao);
                self.journal.marcar_rejeitada(id, &detalhe)?;
                Ok(ResultadoNfse::Rejeitada {
                    operacao: id,
                    erros: rejeicao.erros,
                    detalhe,
                })
            }
            Ok(ResultadoEvento::Desconhecido { detalhe }) => {
                self.journal.marcar_desconhecida(id, &detalhe)?;
                Ok(ResultadoNfse::Desconhecido {
                    operacao: id,
                    detalhe,
                })
            }
            Err(carimbar_nfse::ErroNfse::AntesDoEnvio(erro)) => {
                let detalhe = erro.to_string();
                self.journal.marcar_falha_antes_envio(id, &detalhe)?;
                Err(ErroEmissor::Nfse(carimbar_nfse::ErroNfse::AntesDoEnvio(
                    erro,
                )))
            }
            Err(erro) => {
                self.journal.marcar_desconhecida(id, &erro.to_string())?;
                Err(ErroEmissor::Nfse(erro))
            }
        }
    }

    /// Executa o cancelamento: prepara e confirma.
    pub fn executar_cancelamento(
        &mut self,
        evento: &EventoCancelamento,
    ) -> Result<ResultadoNfse, ErroEmissor> {
        let preparada = self.preparar_cancelamento(evento)?;
        self.confirmar_cancelamento(preparada.id())
    }

    /// Consulta o convênio do município, com cache e atualização explícita.
    pub fn consultar_convenio(
        &self,
        codigo_municipio: &str,
        forcar_atualizacao: bool,
    ) -> Result<ParametrosConvenio, ErroEmissor> {
        let cliente = ClienteParametros::new(
            self.ambiente().tp_amb(),
            self.executor,
            Some(&self.cache_parametros),
        );
        Ok(cliente.consultar_convenio(codigo_municipio, forcar_atualizacao)?)
    }

    /// Consulta a alíquota de um serviço, com cache e atualização explícita.
    pub fn consultar_aliquota(
        &self,
        codigo_municipio: &str,
        codigo_servico: &str,
        competencia: &str,
        forcar_atualizacao: bool,
    ) -> Result<carimbar_nfse::ConsultaAliquotas, ErroEmissor> {
        let cliente = ClienteParametros::new(
            self.ambiente().tp_amb(),
            self.executor,
            Some(&self.cache_parametros),
        );
        Ok(cliente.consultar_aliquota(
            codigo_municipio,
            codigo_servico,
            competencia,
            forcar_atualizacao,
        )?)
    }

    /// Limpa o cache de parâmetros municipais.
    pub fn limpar_cache_parametros(&self) {
        self.cache_parametros.limpar();
    }

    /// Sincroniza documentos do ADN por NSU.
    ///
    /// Cada página é persistida junto com o novo cursor. Uma falha de
    /// transporte ou de escrita deixa o cursor anterior, e a próxima execução
    /// retoma do último NSU persistido. `limite_paginas` evita laço infinito.
    pub fn sincronizar_adn(
        &mut self,
        limite_paginas: u32,
    ) -> Result<ResultadoSincronizacao, ErroEmissor> {
        let empresa = self.empresa;
        let ambiente = self.ambiente().tp_amb();
        let inicio = self.journal.adn_cursor(&empresa, ambiente)?;
        let inicio_nsu = u64::try_from(inicio)
            .map_err(|_| ErroEmissor::Journal("cursor negativo".to_owned()))?;
        let cliente = ClienteAdn::new(ambiente, self.executor);
        let mut nsu = inicio_nsu;
        let mut ultimo_nsu = inicio_nsu;
        let mut documentos_novos = 0_usize;
        let mut paginas = 0_usize;
        for _ in 0..limite_paginas {
            let lote = cliente.consultar(nsu, None, None)?;
            validar_lote(&lote)?;
            if lote.documentos.is_empty() {
                break;
            }
            let proximo = lote.ultimo_nsu.max(nsu);
            documentos_novos +=
                self.journal
                    .adn_persistir_lote(&empresa, ambiente, &lote.documentos, proximo)?;
            paginas += 1;
            ultimo_nsu = proximo;
            if proximo <= nsu {
                break;
            }
            nsu = proximo;
        }
        Ok(ResultadoSincronizacao {
            inicio_nsu,
            ultimo_nsu,
            documentos_novos,
            paginas,
        })
    }

    /// Documentos do ADN persistidos para a empresa e o ambiente.
    pub fn documentos_adn(
        &self,
    ) -> Result<Vec<crate::journal::DocumentoAdnArmazenado>, ErroEmissor> {
        self.journal
            .adn_documentos(&self.empresa, self.ambiente().tp_amb())
    }

    fn exigir_producao_permitida(&self) -> Result<(), ErroEmissor> {
        if self.ambiente().eh_producao() && !self.kill_switch.producao_permitida() {
            return Err(ErroEmissor::ProducaoBloqueada);
        }
        Ok(())
    }

    fn certificado_atual(&self) -> Result<CertificadoA1, ErroEmissor> {
        let id = self
            .certificado_id
            .clone()
            .or_else(|| {
                self.journal
                    .certificado_producao(&self.empresa)
                    .ok()
                    .flatten()
            })
            .or_else(|| {
                self.cofre
                    .listar(&self.empresa)
                    .ok()
                    .and_then(|lista| lista.first().map(|meta| meta.id.clone()))
            })
            .ok_or(ErroEmissor::CertificadoAusente(self.empresa))?;
        let certificado = self.cofre.certificado(&self.empresa, &id)?;
        if certificado.cnpj() != &self.empresa {
            return Err(ErroEmissor::CertificadoDeOutraEmpresa);
        }
        Ok(certificado)
    }

    fn conferir_operacao(
        &self,
        operacao: &Operacao,
        tipo: TipoOperacao,
    ) -> Result<(), ErroEmissor> {
        if operacao.empresa != self.empresa
            || operacao.ambiente != self.ambiente().tp_amb()
            || operacao.tipo != tipo
        {
            return Err(ErroEmissor::OperacaoDeOutraEmpresa(operacao.id));
        }
        Ok(())
    }

    fn resultado_autorizada(&self, operacao: &Operacao) -> Result<ResultadoNfse, ErroEmissor> {
        let chave = operacao
            .chave_nfse
            .as_deref()
            .ok_or_else(|| ErroEmissor::Journal("operação autorizada sem chave".to_owned()))?;
        let chave = ChaveNfse::parse(chave, operacao.ambiente)?;
        let xml_nfse = operacao
            .xml_retorno
            .clone()
            .ok_or_else(|| ErroEmissor::Journal("operação autorizada sem XML".to_owned()))?;
        Ok(ResultadoNfse::Autorizada {
            operacao: operacao.id,
            chave,
            xml_nfse,
        })
    }

    fn resultado_rejeitada(&self, operacao: &Operacao) -> ResultadoNfse {
        ResultadoNfse::Rejeitada {
            operacao: operacao.id,
            erros: Vec::new(),
            detalhe: operacao
                .erro
                .clone()
                .unwrap_or_else(|| "rejeição sem detalhe persistido".to_owned()),
        }
    }
}

fn validar_lote(lote: &LoteDistribuicao) -> Result<(), ErroEmissor> {
    if lote.status == StatusDistribuicao::Rejeicao {
        let detalhe = lote
            .erros
            .iter()
            .map(|erro| format!("{}: {}", erro.codigo, erro.descricao))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ErroEmissor::Nfse(carimbar_nfse::ErroNfse::Distribuicao(
            if detalhe.is_empty() {
                "consulta do ADN rejeitada".to_owned()
            } else {
                detalhe
            },
        )));
    }
    Ok(())
}

fn formatar_rejeicao(rejeicao: &Rejeicao) -> String {
    if rejeicao.erros.is_empty() {
        return "rejeição sem erros informados".to_owned();
    }
    rejeicao
        .erros
        .iter()
        .map(|erro| {
            if erro.codigo.is_empty() {
                erro.descricao.clone()
            } else {
                format!("{}: {}", erro.codigo, erro.descricao)
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}
