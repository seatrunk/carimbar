//! DANFSe em PDF a partir da NFS-e autorizada.
//!
//! A geração é local: não há serviço externo. O PDF é montado com `printpdf`
//! e o QR com `qrcode`; nenhum valor fiscal é recalculado, apenas transcrito
//! do XML autorizado.
//!
//! # Decisão de implementação
//!
//! O esqueleto previa Typst. O MVP usa `printpdf` com fontes padrão do PDF
//! porque o runtime do Typst e a fonte empacotada aumentariam muito o binário e
//! o tempo de build; o resultado continua offline e determinístico. Os campos
//! são transcritos do XML, sem recomputar tributos.
//!
//! # Limites
//!
//! - O texto usa as fontes padrão Helvetica, com cobertura WinAnsi.
//! - O QR aponta para a consulta pública da NFS-e; a URL deve ser reconferida
//!   antes de produção.
//! - A identificação de cancelamento é feita pelo `cStat` da NFS-e ou pelo
//!   evento de cancelamento fornecido pelo chamador.
//!
//! Status: implementação da Etapa 4 (`docs/plano-mvp.md`, issue #15).

#![deny(missing_docs)]

mod danfse;
mod erro;

pub use danfse::{DadosDanfse, OpcoesDanfse, PessoaDanfse, extrair_dados};
pub use erro::ErroDanfe;

/// Gera o DANFSe de uma NFS-e autorizada.
pub fn gerar_danfse(xml_autorizado: &str) -> Result<Vec<u8>, ErroDanfe> {
    danfse::gerar(xml_autorizado, &OpcoesDanfse::default())
}

/// Gera o DANFSe com opções explícitas (por exemplo, cancelamento).
pub fn gerar_danfse_com_opcoes(
    xml_autorizado: &str,
    opcoes: &OpcoesDanfse,
) -> Result<Vec<u8>, ErroDanfe> {
    danfse::gerar(xml_autorizado, opcoes)
}

/// Gera o DANFSe e, se o evento informado for de cancelamento, marca o PDF.
pub fn gerar_danfse_com_evento(
    xml_autorizado: &str,
    xml_evento: Option<&str>,
) -> Result<Vec<u8>, ErroDanfe> {
    let cancelada = xml_evento.is_some_and(danfse::evento_cancela);
    danfse::gerar(xml_autorizado, &OpcoesDanfse { cancelada })
}
