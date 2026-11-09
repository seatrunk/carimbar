//! NF-e modelo 55 v4.00, autorização, consulta, eventos, inutilização e SVC.
//!
//! Status: implementação da Etapa 5 (`docs/plano-mvp.md`, issue #18).

#![deny(missing_docs)]

mod cliente;
mod erro;
mod evento;
mod modelo;

pub use cliente::{
    ClienteNfe, Contingencia, Executor, Protocolo, ResultadoAutorizacao, ResultadoConsulta,
    ResultadoInutilizacao,
};
pub use erro::ErroNfe;
pub use evento::{EventoCancelamento, EventoCce, Inutilizacao, ResultadoEvento};
pub use modelo::{
    Destinatario, Emitente, Endereco, IbsCbsItem, Item, NotaFiscal, Pagamento, TotalIbsCbs,
};

/// Assina um XML de NF-e já serializado com o perfil SHA-1.
#[cfg(feature = "nfe-sha1")]
pub fn assinar_xml(xml: &str, assinante: &dyn carimbar_dsig::Signer) -> Result<String, ErroNfe> {
    let documento = carimbar_xml::Documento::parse(xml.as_bytes())?;
    let inf = documento
        .raiz()
        .primeiro_filho("infNFe")
        .ok_or_else(|| ErroNfe::campo("infNFe", "ausente"))?;
    let id = inf
        .atributo("Id")
        .ok_or_else(|| ErroNfe::campo("infNFe/@Id", "ausente"))?;
    let assinado = carimbar_dsig::assinar_com_perfil(
        &documento,
        id,
        assinante,
        carimbar_dsig::PerfilAssinatura::NfeSha1,
    )?;
    Ok(assinado.to_xml()?)
}

/// Verifica um XML de NF-e assinado no perfil SHA-1.
#[cfg(feature = "nfe-sha1")]
pub fn verificar_xml(xml: &str) -> Result<carimbar_dsig::Verificacao, ErroNfe> {
    let documento = carimbar_xml::Documento::parse(xml.as_bytes())?;
    Ok(carimbar_dsig::verificar_com_perfil(
        &documento,
        carimbar_dsig::PerfilAssinatura::NfeSha1,
    )?)
}
