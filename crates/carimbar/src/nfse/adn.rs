//! Distribuição de DF-e do ADN por NSU.
//!
//! A resposta usa PascalCase e pode vir com HTTP 400/404 contendo um lote
//! válido; o sinal de erro é o campo `StatusProcessamento`. O XML de cada
//! documento vem em GZip/base64 (`ArquivoXml`).

use crate::endpoints::{ConsultaNfse, ServicoNfse, resolver_nfse};
use crate::nfse::erro::ErroNfse;
use crate::nfse::gzip::descomprimir_b64;
use crate::transport::{ErroEnvio, Requisicao, Resposta};
use crate::types::TpAmb;
use serde_json::Value;

use crate::nfse::api::Executor;

/// Status de uma consulta de distribuição.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusDistribuicao {
    /// Há documentos no lote.
    DocumentosLocalizados,
    /// Nenhum documento pendente.
    NenhumDocumento,
    /// O ADN rejeitou a consulta.
    Rejeicao,
}

impl StatusDistribuicao {
    fn parse(texto: &str) -> Result<Self, ErroNfse> {
        match texto {
            "DOCUMENTOS_LOCALIZADOS" => Ok(Self::DocumentosLocalizados),
            "NENHUM_DOCUMENTO_LOCALIZADO" => Ok(Self::NenhumDocumento),
            "REJEICAO" => Ok(Self::Rejeicao),
            outro => Err(ErroNfse::Json(format!(
                "StatusProcessamento desconhecido: {outro}"
            ))),
        }
    }
}

/// Tipo do documento distribuído.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TipoDocumento {
    /// Nenhum tipo informado.
    Nenhum,
    /// DPS.
    Dps,
    /// Pedido de registro de evento.
    PedidoRegistroEvento,
    /// NFS-e.
    Nfse,
    /// Evento.
    Evento,
    /// Cadastro Nacional de Contribuintes.
    Cnc,
    /// Tipo novo, preservado para não perder documentos.
    Outro(String),
}

impl TipoDocumento {
    /// Texto estável do tipo.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Nenhum => "NENHUM",
            Self::Dps => "DPS",
            Self::PedidoRegistroEvento => "PEDIDO_REGISTRO_EVENTO",
            Self::Nfse => "NFSE",
            Self::Evento => "EVENTO",
            Self::Cnc => "CNC",
            Self::Outro(valor) => valor,
        }
    }

    fn parse(texto: &str) -> Self {
        match texto {
            "NENHUM" => Self::Nenhum,
            "DPS" => Self::Dps,
            "PEDIDO_REGISTRO_EVENTO" => Self::PedidoRegistroEvento,
            "NFSE" => Self::Nfse,
            "EVENTO" => Self::Evento,
            "CNC" => Self::Cnc,
            outro => Self::Outro(outro.to_owned()),
        }
    }
}

/// Documento fiscal distribuído.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentoDistribuido {
    /// NSU do documento.
    pub nsu: u64,
    /// Chave de acesso, quando houver.
    pub chave_acesso: Option<String>,
    /// Tipo do documento.
    pub tipo_documento: TipoDocumento,
    /// Tipo do evento, quando aplicável.
    pub tipo_evento: Option<String>,
    /// XML já descompactado.
    pub xml: String,
    /// Data e hora de geração, como informada.
    pub data_hora_geracao: Option<String>,
}

/// Mensagem de alerta ou erro do ADN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MensagemAdn {
    /// Código.
    pub codigo: String,
    /// Descrição.
    pub descricao: String,
    /// Complemento.
    pub complemento: Option<String>,
}

/// Lote devolvido pela distribuição.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoteDistribuicao {
    /// Status.
    pub status: StatusDistribuicao,
    /// Documentos do lote.
    pub documentos: Vec<DocumentoDistribuido>,
    /// Alertas.
    pub alertas: Vec<MensagemAdn>,
    /// Erros.
    pub erros: Vec<MensagemAdn>,
    /// Maior NSU do lote, ou o NSU consultado quando vazio.
    pub ultimo_nsu: u64,
    /// Ambiente.
    pub ambiente: TpAmb,
    /// Versão do aplicativo do ADN.
    pub versao_aplicativo: Option<String>,
    /// Data e hora de processamento.
    pub data_hora_processamento: Option<String>,
}

/// Cliente da API de distribuição do ADN.
pub struct ClienteAdn<'a> {
    ambiente: TpAmb,
    executor: &'a dyn Executor,
}

impl<'a> ClienteAdn<'a> {
    /// Cria o cliente para o ambiente.
    pub fn new(ambiente: TpAmb, executor: &'a dyn Executor) -> Self {
        Self { ambiente, executor }
    }

    /// Consulta um NSU, opcionalmente filtrando por CNPJ e pedindo lote.
    pub fn consultar(
        &self,
        nsu: u64,
        cnpj_consulta: Option<&str>,
        lote: Option<bool>,
    ) -> Result<LoteDistribuicao, ErroNfse> {
        let endpoint = resolver_nfse(&ConsultaNfse::new(
            ServicoNfse::DistribuicaoDfe,
            self.ambiente,
        ))?;
        let nsu_texto = nsu.to_string();
        let url = endpoint.preencher(&[("nsu", &nsu_texto)])?;
        let mut url = url;
        let mut consulta = Vec::new();
        if let Some(cnpj) = cnpj_consulta {
            consulta.push(format!("cnpjConsulta={cnpj}"));
        }
        if let Some(lote) = lote {
            consulta.push(format!("lote={lote}"));
        }
        if !consulta.is_empty() {
            url.push('?');
            url.push_str(&consulta.join("&"));
        }

        let resposta = match self.executor.executar(&Requisicao::get_json(url)) {
            Ok(resposta) => resposta,
            Err(ErroEnvio::AntesDoEnvio(erro) | ErroEnvio::ResultadoDesconhecido(erro)) => {
                return Err(ErroNfse::ResultadoDesconhecido(erro));
            }
        };
        self.interpretar(&resposta, nsu)
    }

    fn interpretar(&self, resposta: &Resposta, nsu: u64) -> Result<LoteDistribuicao, ErroNfse> {
        let corpo: Value = serde_json::from_slice(&resposta.corpo)
            .map_err(|erro| ErroNfse::Json(format!("JSON do ADN: {erro}")))?;
        let objeto = corpo
            .as_object()
            .ok_or_else(|| ErroNfse::Json("corpo do ADN não é objeto".to_owned()))?;
        let status = objeto
            .get("StatusProcessamento")
            .and_then(Value::as_str)
            .ok_or_else(|| ErroNfse::Json("resposta do ADN sem StatusProcessamento".to_owned()))?;
        if resposta.status != 200 && !matches!(resposta.status, 400 | 404) {
            return Err(ErroNfse::Http {
                status: resposta.status,
                corpo: String::from_utf8_lossy(&resposta.corpo)
                    .chars()
                    .take(512)
                    .collect(),
            });
        }
        let status = StatusDistribuicao::parse(status)?;
        let mut documentos = Vec::new();
        if let Some(lista) = objeto.get("LoteDFe").and_then(Value::as_array) {
            for item in lista {
                documentos.push(interpretar_documento(item)?);
            }
        }
        let ultimo_nsu = documentos
            .iter()
            .map(|documento| documento.nsu)
            .max()
            .unwrap_or(nsu);
        let ambiente = match objeto.get("TipoAmbiente").and_then(Value::as_u64) {
            Some(1) => TpAmb::Producao,
            Some(2) => TpAmb::Homologacao,
            _ => self.ambiente,
        };
        Ok(LoteDistribuicao {
            status,
            documentos,
            alertas: mensagens(objeto.get("Alertas")),
            erros: mensagens(objeto.get("Erros")),
            ultimo_nsu,
            ambiente,
            versao_aplicativo: texto(objeto, "VersaoAplicativo"),
            data_hora_processamento: texto(objeto, "DataHoraProcessamento"),
        })
    }
}

fn interpretar_documento(valor: &Value) -> Result<DocumentoDistribuido, ErroNfse> {
    let objeto = valor
        .as_object()
        .ok_or_else(|| ErroNfse::Json("documento do ADN não é objeto".to_owned()))?;
    let nsu = objeto
        .get("NSU")
        .and_then(Value::as_u64)
        .ok_or_else(|| ErroNfse::Json("documento sem NSU".to_owned()))?;
    let xml_b64 = objeto
        .get("ArquivoXml")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let xml = if xml_b64.is_empty() {
        String::new()
    } else {
        descomprimir_b64(xml_b64)?
    };
    Ok(DocumentoDistribuido {
        nsu,
        chave_acesso: texto(objeto, "ChaveAcesso"),
        tipo_documento: objeto
            .get("TipoDocumento")
            .and_then(Value::as_str)
            .map(TipoDocumento::parse)
            .unwrap_or(TipoDocumento::Nenhum),
        tipo_evento: texto(objeto, "TipoEvento"),
        xml,
        data_hora_geracao: texto(objeto, "DataHoraGeracao"),
    })
}

fn mensagens(valor: Option<&Value>) -> Vec<MensagemAdn> {
    valor
        .and_then(Value::as_array)
        .map(|lista| {
            lista
                .iter()
                .filter_map(|item| {
                    let objeto = item.as_object()?;
                    Some(MensagemAdn {
                        codigo: texto(objeto, "Codigo").unwrap_or_default(),
                        descricao: texto(objeto, "Descricao").unwrap_or_default(),
                        complemento: texto(objeto, "Complemento"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn texto(objeto: &serde_json::Map<String, Value>, campo: &str) -> Option<String> {
    match objeto.get(campo)? {
        Value::String(texto) => Some(texto.clone()),
        Value::Number(numero) => Some(numero.to_string()),
        _ => None,
    }
}
