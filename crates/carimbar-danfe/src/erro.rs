//! Erros da geração de DANFSe.

use carimbar_xml::ErroXml;
use thiserror::Error;

/// Falha ao extrair dados ou gerar o PDF.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ErroDanfe {
    /// XML ilegível ou inválido.
    #[error("XML: {0}")]
    Xml(#[from] ErroXml),
    /// O XML não é uma NFS-e autorizada.
    #[error("XML não é uma NFS-e autorizada: {0}")]
    NaoAutorizada(String),
    /// Campo obrigatório ausente.
    #[error("campo ausente na NFS-e: {0}")]
    CampoAusente(&'static str),
    /// Falha ao gerar o QR.
    #[error("QR: {0}")]
    Qr(String),
    /// Falha ao serializar o PDF.
    #[error("PDF: {0}")]
    Pdf(String),
}
