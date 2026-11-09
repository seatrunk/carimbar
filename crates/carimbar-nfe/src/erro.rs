//! Erros da camada NF-e.

use carimbar_dsig::ErroDsig;
use carimbar_endpoints::ErroEndpoints;
use carimbar_transport::ErroTransporte;
use carimbar_types::ErroTipo;
use carimbar_xml::ErroXml;
use thiserror::Error;

/// Falha ao montar, assinar, enviar ou interpretar uma NF-e.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ErroNfe {
    /// Campo de domínio inválido.
    #[error("{campo}: {detalhe}")]
    Campo {
        /// Campo.
        campo: &'static str,
        /// Detalhe.
        detalhe: String,
    },
    /// Tipo de domínio inválido.
    #[error("domínio: {0}")]
    Tipo(#[from] ErroTipo),
    /// Endpoint inválido.
    #[error("endpoint: {0}")]
    Endpoint(#[from] ErroEndpoints),
    /// XML inválido.
    #[error("XML: {0}")]
    Xml(#[from] ErroXml),
    /// Falha de assinatura.
    #[error("assinatura: {0}")]
    Assinatura(#[from] ErroDsig),
    /// Falha antes do envio.
    #[error("falha antes do envio: {0}")]
    AntesDoEnvio(#[from] ErroTransporte),
    /// Resultado desconhecido após envio.
    #[error("resultado desconhecido: {0}")]
    ResultadoDesconhecido(ErroTransporte),
    /// Falha de transporte não classificada.
    #[error("transporte: {0}")]
    Transporte(String),
    /// HTTP inesperado.
    #[error("HTTP {status}: {corpo}")]
    Http {
        /// Status.
        status: u16,
        /// Corpo.
        corpo: String,
    },
    /// Resposta fora do contrato.
    #[error("resposta inválida: {0}")]
    Resposta(String),
    /// Rejeição fiscal.
    #[error("NF-e rejeitada: {codigo} {motivo}")]
    Rejeitada {
        /// Código SEFAZ.
        codigo: String,
        /// Motivo.
        motivo: String,
    },
    /// Operação ainda em processamento.
    #[error("processamento pendente: {0}")]
    Pendente(String),
    /// XML não passou no XSD.
    #[cfg(feature = "xsd")]
    #[error("XML inválido para o XSD: {0}")]
    Xsd(String),
}

impl ErroNfe {
    pub(crate) fn campo(campo: &'static str, detalhe: impl Into<String>) -> Self {
        Self::Campo {
            campo,
            detalhe: detalhe.into(),
        }
    }
}
