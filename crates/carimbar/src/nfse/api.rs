//! Cliente das APIs JSON da Sefin Nacional (emissão, consulta e eventos).
//!
//! O cliente não abre conexão sozinho: ele recebe um [`Executor`], que em
//! produção é um `carimbar::transport::ClienteMtls`. Essa separação permite
//! testar todas as respostas sem rede e sem certificado.
//!
//! O envio de emissão nunca repete uma requisição. Uma falha depois do início
//! da escrita vira [`ResultadoEmissao::Desconhecido`] e o chamador deve
//! reconciliar com [`ClienteSefin::consultar_dps`] antes de tentar de novo.

use crate::dsig::Signer;
use crate::endpoints::{ConsultaNfse, ServicoNfse, resolver_nfse};
use crate::nfse::dps::Dps;
use crate::nfse::erro::ErroNfse;
use crate::nfse::evento::EventoCancelamento;
use crate::nfse::gzip::{comprimir_b64, descomprimir_b64};
use crate::nfse::identificadores::{ChaveNfse, IdDps};
use crate::transport::{ClienteMtls, ErroEnvio, Requisicao, Resposta};
use crate::types::TpAmb;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

/// Executor de uma requisição HTTP já montada.
pub trait Executor {
    /// Executa uma única tentativa, preservando a classificação de falhas.
    fn executar(&self, requisicao: &Requisicao) -> Result<Resposta, ErroEnvio>;
}

impl Executor for ClienteMtls {
    fn executar(&self, requisicao: &Requisicao) -> Result<Resposta, ErroEnvio> {
        ClienteMtls::executar(self, requisicao)
    }
}

/// Erro fiscal devolvido pela Sefin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErroFiscal {
    /// Código do erro.
    pub codigo: String,
    /// Descrição.
    pub descricao: String,
    /// Complemento, quando houver.
    pub complemento: Option<String>,
}

/// Rejeição de um documento pela Sefin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejeicao {
    /// Id da DPS, quando a Sefin conseguiu extraí-lo.
    pub id_dps: Option<String>,
    /// Erros informados.
    pub erros: Vec<ErroFiscal>,
}

/// NFS-e autorizada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmissaoAutorizada {
    /// Id da DPS enviada.
    pub id_dps: String,
    /// Chave de acesso da NFS-e.
    pub chave: ChaveNfse,
    /// XML da NFS-e autorizada, já descompactado.
    pub xml_nfse: String,
    /// Alertas informados pela Sefin.
    pub alertas: Vec<String>,
    /// Data e hora de processamento, como informada.
    pub data_hora_processamento: Option<String>,
    /// Versão do aplicativo da Sefin.
    pub versao_aplicativo: Option<String>,
}

/// Resultado de uma tentativa de emissão.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultadoEmissao {
    /// NFS-e gerada e retornada.
    Autorizada(EmissaoAutorizada),
    /// A Sefin rejeitou a DPS por regra de negócio.
    Rejeitada(Rejeicao),
    /// A DPS já havia sido processada.
    Duplicidade(Rejeicao),
    /// A requisição pode ter sido processada, mas o resultado não foi lido.
    /// Consulte pelo Id da DPS antes de reenviar.
    Desconhecido {
        /// Diagnóstico da falha.
        detalhe: String,
    },
}

/// NFS-e retornada por consulta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfseConsultada {
    /// Chave consultada.
    pub chave: ChaveNfse,
    /// XML da NFS-e.
    pub xml_nfse: String,
}

/// Resultado da consulta de uma DPS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpsLocalizada {
    /// Id da DPS consultada.
    pub id_dps: String,
    /// Chave da NFS-e gerada a partir dela.
    pub chave: ChaveNfse,
}

/// Evento registrado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventoRegistrado {
    /// XML do evento registrado.
    pub xml_evento: String,
}

/// Resultado de uma tentativa de registro de evento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultadoEvento {
    /// Evento registrado.
    Registrado(EventoRegistrado),
    /// A Sefin rejeitou o pedido.
    Rejeitado(Rejeicao),
    /// A requisição pode ter sido processada, mas o resultado não foi lido.
    Desconhecido {
        /// Diagnóstico da falha.
        detalhe: String,
    },
}

/// Cliente das APIs da Sefin Nacional para um ambiente.
pub struct ClienteSefin<'a> {
    ambiente: TpAmb,
    executor: &'a dyn Executor,
}

impl<'a> ClienteSefin<'a> {
    /// Cria o cliente para o ambiente e o executor informados.
    pub fn new(ambiente: TpAmb, executor: &'a dyn Executor) -> Self {
        Self { ambiente, executor }
    }

    /// Ambiente usado nas URLs e nas validações.
    pub fn ambiente(&self) -> TpAmb {
        self.ambiente
    }

    /// Assina a DPS, envia para `/nfse` e interpreta a resposta.
    ///
    /// A assinatura usa o perfil SHA-256 do XMLDSig. Uma falha de rede antes do
    /// envio retorna erro; uma falha depois do início da escrita retorna
    /// [`ResultadoEmissao::Desconhecido`].
    pub fn emitir(&self, dps: &Dps, assinante: &dyn Signer) -> Result<ResultadoEmissao, ErroNfse> {
        if dps.ambiente != self.ambiente {
            return Err(ErroNfse::campo(
                "dps.tpAmb",
                "a DPS não pertence ao ambiente do cliente",
            ));
        }
        let xml = dps.para_xml_assinado(assinante)?;
        self.emitir_xml(&xml)
    }

    /// Assina, valida a DPS assinada contra o XSD indicado e só então envia.
    ///
    /// O chamador informa o pacote correspondente ao ambiente: produção ou
    /// produção restrita. O XSD não é embutido no crate.
    #[cfg(feature = "xsd")]
    pub fn emitir_validando(
        &self,
        dps: &Dps,
        assinante: &dyn Signer,
        esquema: impl AsRef<std::path::Path>,
    ) -> Result<ResultadoEmissao, ErroNfse> {
        if dps.ambiente != self.ambiente {
            return Err(ErroNfse::campo(
                "dps.tpAmb",
                "a DPS não pertence ao ambiente do cliente",
            ));
        }
        let xml = dps.para_xml_assinado(assinante)?;
        crate::xml::xsd::validar(xml.as_bytes(), esquema)
            .map_err(|erro| ErroNfse::Xsd(erro.to_string()))?;
        self.emitir_xml(&xml)
    }

    /// Envia uma DPS já assinada. O XML não é modificado.
    pub fn emitir_xml(&self, dps_assinada: &str) -> Result<ResultadoEmissao, ErroNfse> {
        let url = resolver_nfse(&ConsultaNfse::new(ServicoNfse::EmitirDps, self.ambiente))?.url;
        let corpo = json!({ "dpsXmlGZipB64": comprimir_b64(dps_assinada)? });
        let requisicao = Requisicao::post_json(url, serde_json::to_vec(&corpo).map_err(json_erro)?);
        let resposta = match self.executor.executar(&requisicao) {
            Ok(resposta) => resposta,
            Err(ErroEnvio::AntesDoEnvio(erro)) => return Err(ErroNfse::AntesDoEnvio(erro)),
            Err(ErroEnvio::ResultadoDesconhecido(erro)) => {
                return Ok(ResultadoEmissao::Desconhecido {
                    detalhe: erro.to_string(),
                });
            }
        };
        self.interpretar_emissao(&resposta)
    }

    /// Consulta uma NFS-e pela chave.
    pub fn consultar(&self, chave: &ChaveNfse) -> Result<NfseConsultada, ErroNfse> {
        let endpoint = resolver_nfse(&ConsultaNfse::new(
            ServicoNfse::ConsultarNfse,
            self.ambiente,
        ))?;
        let url = endpoint.preencher(&[("chave", chave.as_str())])?;
        let resposta = self.executar_consulta(&url)?;
        if resposta.status == 404 {
            return Err(ErroNfse::NaoEncontrada);
        }
        if !resposta.sucesso() {
            return Err(ErroNfse::Http {
                status: resposta.status,
                corpo: corpo_texto(&resposta),
            });
        }
        let objeto = objeto(&resposta)?;
        let xml = texto(&objeto, "nfseXmlGZipB64")
            .ok_or_else(|| ErroNfse::Json("resposta de consulta sem nfseXmlGZipB64".to_owned()))?;
        Ok(NfseConsultada {
            chave: chave.clone(),
            xml_nfse: descomprimir_b64(&xml)?,
        })
    }

    /// Consulta se uma DPS gerou NFS-e. É a operação de reconciliação para
    /// [`ResultadoEmissao::Desconhecido`].
    pub fn consultar_dps(&self, id: &IdDps) -> Result<DpsLocalizada, ErroNfse> {
        let endpoint = resolver_nfse(&ConsultaNfse::new(ServicoNfse::ConsultarDps, self.ambiente))?;
        let url = endpoint.preencher(&[("idDps", id.as_str())])?;
        let resposta = self.executar_consulta(&url)?;
        if resposta.status == 404 {
            return Err(ErroNfse::NaoEncontrada);
        }
        if !resposta.sucesso() {
            return Err(ErroNfse::Http {
                status: resposta.status,
                corpo: corpo_texto(&resposta),
            });
        }
        let objeto = objeto(&resposta)?;
        let id_dps = texto(&objeto, "idDps")
            .ok_or_else(|| ErroNfse::Json("resposta sem idDps".to_owned()))?;
        let chave = texto(&objeto, "chaveAcesso")
            .ok_or_else(|| ErroNfse::Json("resposta sem chaveAcesso".to_owned()))?;
        Ok(DpsLocalizada {
            id_dps,
            chave: ChaveNfse::parse(&chave, self.ambiente)?,
        })
    }

    /// Assina e registra o evento de cancelamento.
    pub fn registrar_cancelamento(
        &self,
        evento: &EventoCancelamento,
        assinante: &dyn Signer,
    ) -> Result<ResultadoEvento, ErroNfse> {
        if evento.ambiente != self.ambiente {
            return Err(ErroNfse::campo(
                "evento.tpAmb",
                "o evento não pertence ao ambiente do cliente",
            ));
        }
        let xml = evento.para_xml_assinado(assinante)?;
        self.registrar_evento_xml(&xml)
    }

    /// Assina, valida o pedido contra o XSD indicado e só então envia.
    #[cfg(feature = "xsd")]
    pub fn registrar_cancelamento_validando(
        &self,
        evento: &EventoCancelamento,
        assinante: &dyn Signer,
        esquema: impl AsRef<std::path::Path>,
    ) -> Result<ResultadoEvento, ErroNfse> {
        if evento.ambiente != self.ambiente {
            return Err(ErroNfse::campo(
                "evento.tpAmb",
                "o evento não pertence ao ambiente do cliente",
            ));
        }
        let xml = evento.para_xml_assinado(assinante)?;
        crate::xml::xsd::validar(xml.as_bytes(), esquema)
            .map_err(|erro| ErroNfse::Xsd(erro.to_string()))?;
        self.registrar_evento_xml(&xml)
    }

    /// Registra um pedido de evento já assinado.
    pub fn registrar_evento_xml(&self, pedido_assinado: &str) -> Result<ResultadoEvento, ErroNfse> {
        let chave = chave_do_pedido(pedido_assinado)?;
        let endpoint = resolver_nfse(&ConsultaNfse::new(
            ServicoNfse::RegistrarEvento,
            self.ambiente,
        ))?;
        let url = endpoint.preencher(&[("chave", &chave)])?;
        let corpo = json!({
            "pedidoRegistroEventoXmlGZipB64": comprimir_b64(pedido_assinado)?
        });
        let requisicao = Requisicao::post_json(url, serde_json::to_vec(&corpo).map_err(json_erro)?);
        let resposta = match self.executor.executar(&requisicao) {
            Ok(resposta) => resposta,
            Err(ErroEnvio::AntesDoEnvio(erro)) => return Err(ErroNfse::AntesDoEnvio(erro)),
            Err(ErroEnvio::ResultadoDesconhecido(erro)) => {
                return Ok(ResultadoEvento::Desconhecido {
                    detalhe: erro.to_string(),
                });
            }
        };
        self.interpretar_evento(&resposta)
    }

    fn executar_consulta(&self, url: &str) -> Result<Resposta, ErroNfse> {
        let requisicao = Requisicao::get_json(url);
        self.executor
            .executar(&requisicao)
            .map_err(|erro| match erro {
                ErroEnvio::AntesDoEnvio(erro) | ErroEnvio::ResultadoDesconhecido(erro) => {
                    ErroNfse::ResultadoDesconhecido(erro)
                }
            })
    }

    fn interpretar_emissao(&self, resposta: &Resposta) -> Result<ResultadoEmissao, ErroNfse> {
        if resposta.status == 409 {
            return Ok(ResultadoEmissao::Duplicidade(rejeicao(resposta)?));
        }
        if (500..600).contains(&resposta.status) {
            return Ok(ResultadoEmissao::Desconhecido {
                detalhe: format!("HTTP {}: {}", resposta.status, corpo_texto(resposta)),
            });
        }
        if !resposta.sucesso() {
            if (400..500).contains(&resposta.status) {
                return Ok(ResultadoEmissao::Rejeitada(rejeicao(resposta)?));
            }
            return Err(ErroNfse::Http {
                status: resposta.status,
                corpo: corpo_texto(resposta),
            });
        }
        let objeto = objeto(resposta)?;
        if let Some(xml) = texto(&objeto, "nfseXmlGZipB64") {
            let id_dps = texto(&objeto, "idDps")
                .ok_or_else(|| ErroNfse::Json("resposta de sucesso sem idDps".to_owned()))?;
            let chave = texto(&objeto, "chaveAcesso")
                .ok_or_else(|| ErroNfse::Json("resposta de sucesso sem chaveAcesso".to_owned()))?;
            return Ok(ResultadoEmissao::Autorizada(EmissaoAutorizada {
                id_dps,
                chave: ChaveNfse::parse(&chave, self.ambiente)?,
                xml_nfse: descomprimir_b64(&xml)?,
                alertas: alertas(&objeto),
                data_hora_processamento: texto(&objeto, "dataHoraProcessamento"),
                versao_aplicativo: texto(&objeto, "versaoAplicativo"),
            }));
        }
        if tem_erros(&objeto) {
            return Ok(ResultadoEmissao::Rejeitada(rejeicao_do_objeto(&objeto)));
        }
        Err(ErroNfse::Json(
            "resposta de emissão sem nfseXmlGZipB64 nem erros".to_owned(),
        ))
    }

    fn interpretar_evento(&self, resposta: &Resposta) -> Result<ResultadoEvento, ErroNfse> {
        if (500..600).contains(&resposta.status) {
            return Ok(ResultadoEvento::Desconhecido {
                detalhe: format!("HTTP {}: {}", resposta.status, corpo_texto(resposta)),
            });
        }
        if resposta.status == 409 || (400..500).contains(&resposta.status) {
            return Ok(ResultadoEvento::Rejeitado(rejeicao(resposta)?));
        }
        if !resposta.sucesso() {
            return Err(ErroNfse::Http {
                status: resposta.status,
                corpo: corpo_texto(resposta),
            });
        }
        let objeto = objeto(resposta)?;
        if let Some(xml) = texto(&objeto, "eventoXmlGZipB64") {
            return Ok(ResultadoEvento::Registrado(EventoRegistrado {
                xml_evento: descomprimir_b64(&xml)?,
            }));
        }
        if tem_erros(&objeto) {
            return Ok(ResultadoEvento::Rejeitado(rejeicao_do_objeto(&objeto)));
        }
        Err(ErroNfse::Json(
            "resposta de evento sem eventoXmlGZipB64 nem erros".to_owned(),
        ))
    }
}

fn chave_do_pedido(xml: &str) -> Result<String, ErroNfse> {
    let documento = crate::xml::Documento::parse(xml.as_bytes())?;
    let inf = documento
        .raiz()
        .primeiro_filho("infPedReg")
        .ok_or_else(|| ErroNfse::campo("pedRegEvento", "infPedReg ausente"))?;
    let chave = inf
        .primeiro_filho("chNFSe")
        .map(|elemento| elemento.texto())
        .filter(|texto| !texto.is_empty())
        .ok_or_else(|| ErroNfse::campo("pedRegEvento", "chNFSe ausente"))?;
    if chave.len() != 50 {
        return Err(ErroNfse::campo("chNFSe", "esperado 50 posições"));
    }
    Ok(chave)
}

fn objeto(resposta: &Resposta) -> Result<Map<String, Value>, ErroNfse> {
    let valor: Value = serde_json::from_slice(&resposta.corpo)
        .map_err(|erro| ErroNfse::Json(format!("JSON: {erro}")))?;
    valor
        .as_object()
        .cloned()
        .ok_or_else(|| ErroNfse::Json("corpo JSON não é um objeto".to_owned()))
}

fn rejeicao(resposta: &Resposta) -> Result<Rejeicao, ErroNfse> {
    let objeto = objeto(resposta)?;
    if !tem_erros(&objeto) {
        return Err(ErroNfse::Http {
            status: resposta.status,
            corpo: corpo_texto(resposta),
        });
    }
    Ok(rejeicao_do_objeto(&objeto))
}

fn rejeicao_do_objeto(objeto: &Map<String, Value>) -> Rejeicao {
    Rejeicao {
        id_dps: texto(objeto, "idDps"),
        erros: erros_do_objeto(objeto),
    }
}

fn erros_do_objeto(objeto: &Map<String, Value>) -> Vec<ErroFiscal> {
    let mut vistos = BTreeSet::new();
    let mut erros = Vec::new();
    for campo in ["erros", "erro"] {
        let Some(valor) = objeto.get(campo) else {
            continue;
        };
        let mut adicionar = |erro: ErroFiscal| {
            let chave = (erro.codigo.clone(), erro.descricao.clone());
            if vistos.insert(chave) {
                erros.push(erro);
            }
        };
        match valor {
            Value::Array(lista) => {
                for item in lista {
                    if let Some(erro) = erro_fiscal(item) {
                        adicionar(erro);
                    }
                }
            }
            Value::Object(_) => {
                if let Some(erro) = erro_fiscal(valor) {
                    adicionar(erro);
                }
            }
            Value::String(mensagem) => adicionar(ErroFiscal {
                codigo: String::new(),
                descricao: mensagem.clone(),
                complemento: None,
            }),
            _ => {}
        }
    }
    erros
}

fn erro_fiscal(valor: &Value) -> Option<ErroFiscal> {
    let objeto = valor.as_object()?;
    let codigo = texto(objeto, "codigo")
        .or_else(|| texto(objeto, "code"))
        .unwrap_or_default();
    let descricao = texto(objeto, "descricao")
        .or_else(|| texto(objeto, "mensagem"))
        .or_else(|| texto(objeto, "message"))
        .unwrap_or_default();
    if codigo.is_empty() && descricao.is_empty() {
        return None;
    }
    Some(ErroFiscal {
        codigo,
        descricao,
        complemento: texto(objeto, "complemento"),
    })
}

fn tem_erros(objeto: &Map<String, Value>) -> bool {
    !erros_do_objeto(objeto).is_empty()
}

fn alertas(objeto: &Map<String, Value>) -> Vec<String> {
    objeto
        .get("alertas")
        .and_then(Value::as_array)
        .map(|lista| {
            lista
                .iter()
                .filter_map(|item| match item {
                    Value::String(texto) => Some(texto.clone()),
                    Value::Object(objeto) => {
                        texto(objeto, "descricao").or_else(|| texto(objeto, "mensagem"))
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn texto(objeto: &Map<String, Value>, campo: &str) -> Option<String> {
    match objeto.get(campo)? {
        Value::String(texto) => Some(texto.clone()),
        Value::Number(numero) => Some(numero.to_string()),
        _ => None,
    }
}

fn corpo_texto(resposta: &Resposta) -> String {
    let texto = String::from_utf8_lossy(&resposta.corpo);
    texto.chars().take(512).collect()
}

fn json_erro(erro: serde_json::Error) -> ErroNfse {
    ErroNfse::Json(format!("serialização: {erro}"))
}
