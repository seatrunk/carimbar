//! Cliente SOAP 1.2 da NF-e: autorização, recibo e consulta.

use crate::erro::ErroNfe;
use carimbar_endpoints::{ConsultaNfe, ServicoNfe, Uf, resolver_nfe};
use carimbar_transport::{ErroEnvio, Requisicao, Resposta};
use carimbar_types::TpAmb;
use carimbar_xml::{Documento, Elemento, No, Nome};

/// Executor de uma requisição HTTP.
pub use carimbar_nfse::Executor;

/// Contingência da NF-e.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contingencia {
    /// Emissão normal.
    Normal,
    /// SVC-AN.
    SvcAn,
    /// SVC-RS.
    SvcRs,
}

/// Protocolo de autorização.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Protocolo {
    /// Chave.
    pub chave: String,
    /// Número do protocolo.
    pub numero: String,
    /// XML autorizado (`nfeProc`).
    pub xml: String,
}

/// Resultado da autorização.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultadoAutorizacao {
    /// Autorizada.
    Autorizada(Protocolo),
    /// Lote recebido; consultar recibo.
    Recibo(String),
    /// Rejeitada.
    Rejeitada {
        /// Código SEFAZ.
        codigo: String,
        /// Motivo.
        motivo: String,
    },
}

/// Resultado da consulta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoConsulta {
    /// Código.
    pub codigo: String,
    /// Motivo.
    pub motivo: String,
    /// Protocolo, quando houver.
    pub protocolo: Option<Protocolo>,
}

/// Resultado da inutilização.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoInutilizacao {
    /// Código.
    pub codigo: String,
    /// Motivo.
    pub motivo: String,
    /// Protocolo.
    pub protocolo: Option<String>,
}

/// Cliente SOAP da NF-e.
pub struct ClienteNfe<'a> {
    ambiente: TpAmb,
    uf: Uf,
    contingencia: Contingencia,
    executor: &'a dyn Executor,
}

impl<'a> ClienteNfe<'a> {
    /// Cria o cliente.
    pub fn novo(
        ambiente: TpAmb,
        uf: Uf,
        contingencia: Contingencia,
        executor: &'a dyn Executor,
    ) -> Self {
        Self {
            ambiente,
            uf,
            contingencia,
            executor,
        }
    }

    /// Envia uma NF-e assinada com `indSinc=1`.
    pub fn autorizar_xml(&self, xml_assinado: &str) -> Result<ResultadoAutorizacao, ErroNfe> {
        let envelope = format!(
            "<enviNFe xmlns=\"{}\" versao=\"4.00\"><idLote>1</idLote><indSinc>1</indSinc>{}</enviNFe>",
            crate::modelo::NS_NFE,
            xml_assinado.trim()
        );
        let resposta = self.soap(ServicoNfe::Autorizacao, &envelope)?;
        let documento = Documento::parse(&resposta.corpo)?;
        let ret = procurar(&documento, "retEnviNFe")
            .ok_or_else(|| ErroNfe::Resposta("retEnviNFe ausente".to_owned()))?;
        let codigo = texto(ret, "cStat").unwrap_or_default();
        let motivo = texto(ret, "xMotivo").unwrap_or_default();
        match codigo.as_str() {
            "103" => {
                let recibo = ret
                    .primeiro_filho("infRec")
                    .and_then(|inf| texto(inf, "nRec"))
                    .unwrap_or_default();
                Ok(ResultadoAutorizacao::Recibo(recibo))
            }
            "104" => {
                if let Some(protocolo) = montar_protocolo(xml_assinado, ret)? {
                    Ok(ResultadoAutorizacao::Autorizada(protocolo))
                } else {
                    Ok(ResultadoAutorizacao::Rejeitada { codigo, motivo })
                }
            }
            _ => Ok(ResultadoAutorizacao::Rejeitada { codigo, motivo }),
        }
    }

    /// Consulta o resultado de um lote pelo recibo.
    pub fn consultar_recibo(&self, recibo: &str) -> Result<ResultadoAutorizacao, ErroNfe> {
        let consulta = format!(
            "<consReciNFe xmlns=\"{}\" versao=\"4.00\"><tpAmb>{}</tpAmb><nRec>{recibo}</nRec></consReciNFe>",
            crate::modelo::NS_NFE,
            self.ambiente.codigo()
        );
        let resposta = self.soap(ServicoNfe::RetAutorizacao, &consulta)?;
        let documento = Documento::parse(&resposta.corpo)?;
        let ret = procurar(&documento, "retConsReciNFe")
            .ok_or_else(|| ErroNfe::Resposta("retConsReciNFe ausente".to_owned()))?;
        let codigo = texto(ret, "cStat").unwrap_or_default();
        let motivo = texto(ret, "xMotivo").unwrap_or_default();
        if codigo == "104"
            && let Some(protocolo) = montar_protocolo("", ret)?
        {
            return Ok(ResultadoAutorizacao::Autorizada(protocolo));
        }
        if matches!(codigo.as_str(), "105" | "106") {
            return Ok(ResultadoAutorizacao::Recibo(recibo.to_owned()));
        }
        Ok(ResultadoAutorizacao::Rejeitada { codigo, motivo })
    }

    /// Consulta a situação da NF-e pela chave.
    pub fn consultar(&self, chave: &str) -> Result<ResultadoConsulta, ErroNfe> {
        let consulta = format!(
            "<consSitNFe xmlns=\"{}\" versao=\"4.00\"><tpAmb>{}</tpAmb><xServ>CONSULTAR</xServ><chNFe>{chave}</chNFe></consSitNFe>",
            crate::modelo::NS_NFE,
            self.ambiente.codigo()
        );
        let resposta = self.soap(ServicoNfe::ConsultaProtocolo, &consulta)?;
        let documento = Documento::parse(&resposta.corpo)?;
        let ret = procurar(&documento, "retConsSitNFe")
            .ok_or_else(|| ErroNfe::Resposta("retConsSitNFe ausente".to_owned()))?;
        let codigo = texto(ret, "cStat").unwrap_or_default();
        let motivo = texto(ret, "xMotivo").unwrap_or_default();
        let protocolo = montar_protocolo("", ret)?;
        Ok(ResultadoConsulta {
            codigo,
            motivo,
            protocolo,
        })
    }

    /// Envia um `envEvento` já montado e assinado.
    pub fn enviar_evento(
        &self,
        env_evento: &str,
    ) -> Result<crate::evento::ResultadoEvento, ErroNfe> {
        let resposta = self.soap(ServicoNfe::RecepcaoEvento, env_evento)?;
        crate::evento::interpretar_ret_evento(&resposta.corpo)
    }

    /// Envia uma inutilização já assinada.
    pub fn enviar_inutilizacao(&self, inut: &str) -> Result<ResultadoInutilizacao, ErroNfe> {
        let resposta = self.soap(ServicoNfe::Inutilizacao, inut)?;
        let documento = Documento::parse(&resposta.corpo)?;
        let ret = procurar(&documento, "retInutNFe")
            .ok_or_else(|| ErroNfe::Resposta("retInutNFe ausente".to_owned()))?;
        let inf = ret.primeiro_filho("infInut").unwrap_or(ret);
        let codigo = texto(inf, "cStat").unwrap_or_default();
        let motivo = texto(inf, "xMotivo").unwrap_or_default();
        let protocolo = texto(inf, "nProt");
        Ok(ResultadoInutilizacao {
            codigo,
            motivo,
            protocolo,
        })
    }

    fn soap(&self, servico: ServicoNfe, conteudo: &str) -> Result<Resposta, ErroNfe> {
        let (acao, namespace) = acao(servico);
        let mut consulta = ConsultaNfe::new(self.uf, servico, self.ambiente);
        if let Some(contingencia) = self.contingencia_endpoint() {
            consulta = consulta.em_contingencia(contingencia);
        }
        let url = resolver_nfe(&consulta)?.url;
        let corpo = format!("<nfeDadosMsg xmlns=\"{namespace}\">{conteudo}</nfeDadosMsg>");
        let requisicao = Requisicao::post_soap12(url, acao, &corpo)?;
        let resposta = match self.executor.executar(&requisicao) {
            Ok(resposta) => resposta,
            Err(ErroEnvio::AntesDoEnvio(erro)) => return Err(ErroNfe::AntesDoEnvio(erro)),
            Err(ErroEnvio::ResultadoDesconhecido(erro)) => {
                return Err(ErroNfe::ResultadoDesconhecido(erro));
            }
            Err(outro) => return Err(ErroNfe::Transporte(outro.to_string())),
        };
        if resposta.status != 200 {
            return Err(ErroNfe::Http {
                status: resposta.status,
                corpo: String::from_utf8_lossy(&resposta.corpo)
                    .chars()
                    .take(512)
                    .collect(),
            });
        }
        Ok(resposta)
    }

    fn contingencia_endpoint(&self) -> Option<carimbar_endpoints::Contingencia> {
        match self.contingencia {
            Contingencia::Normal => None,
            Contingencia::SvcAn => Some(carimbar_endpoints::Contingencia::SvcAn),
            Contingencia::SvcRs => Some(carimbar_endpoints::Contingencia::SvcRs),
        }
    }
}

fn acao(servico: ServicoNfe) -> (&'static str, &'static str) {
    match servico {
        ServicoNfe::Autorizacao => (
            "http://www.portalfiscal.inf.br/nfe/wsdl/NFeAutorizacao4/nfeAutorizacaoLote",
            "http://www.portalfiscal.inf.br/nfe/wsdl/NFeAutorizacao4",
        ),
        ServicoNfe::RetAutorizacao => (
            "http://www.portalfiscal.inf.br/nfe/wsdl/NFeRetAutorizacao4/nfeRetAutorizacaoLote",
            "http://www.portalfiscal.inf.br/nfe/wsdl/NFeRetAutorizacao4",
        ),
        ServicoNfe::ConsultaProtocolo => (
            "http://www.portalfiscal.inf.br/nfe/wsdl/NFeConsultaProtocolo4/nfeConsultaNF",
            "http://www.portalfiscal.inf.br/nfe/wsdl/NFeConsultaProtocolo4",
        ),
        ServicoNfe::RecepcaoEvento => (
            "http://www.portalfiscal.inf.br/nfe/wsdl/NFeRecepcaoEvento4/nfeRecepcaoEvento",
            "http://www.portalfiscal.inf.br/nfe/wsdl/NFeRecepcaoEvento4",
        ),
        ServicoNfe::Inutilizacao => (
            "http://www.portalfiscal.inf.br/nfe/wsdl/NFeInutilizacao4/nfeInutilizacaoNF",
            "http://www.portalfiscal.inf.br/nfe/wsdl/NFeInutilizacao4",
        ),
        _ => ("", ""),
    }
}

fn procurar<'a>(documento: &'a Documento, local: &str) -> Option<&'a Elemento> {
    documento
        .caminhos_por_local(local)
        .first()
        .and_then(|caminho| documento.elemento_em(caminho))
}

fn texto(elemento: &Elemento, local: &str) -> Option<String> {
    elemento
        .primeiro_filho(local)
        .map(Elemento::texto)
        .filter(|valor| !valor.is_empty())
}

fn montar_protocolo(xml_assinado: &str, ret: &Elemento) -> Result<Option<Protocolo>, ErroNfe> {
    let Some(prot) = ret.primeiro_filho("protNFe") else {
        return Ok(None);
    };
    let inf = prot
        .primeiro_filho("infProt")
        .ok_or_else(|| ErroNfe::Resposta("protNFe sem infProt".to_owned()))?;
    let chave = texto(inf, "chNFe").unwrap_or_default();
    let numero = texto(inf, "nProt").unwrap_or_default();
    let xml = if xml_assinado.is_empty() {
        String::new()
    } else {
        let original = Documento::parse(xml_assinado.as_bytes())?;
        let raiz = Elemento::new(Nome::new("nfeProc"))
            .com_namespace(None, crate::modelo::NS_NFE)
            .com_atributo_texto("versao", "4.00")
            .com_filho(No::Elemento(original.raiz().clone()))
            .com_filho(No::Elemento(prot.clone()));
        Documento::new(raiz).to_xml()?
    };
    Ok(Some(Protocolo { chave, numero, xml }))
}
