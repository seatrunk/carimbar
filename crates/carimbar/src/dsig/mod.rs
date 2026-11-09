//! XMLDSig enveloped: assinatura via trait [`Signer`] e verificação
//! com validação de digest, algoritmos, `Id` e assinatura.
//!
//! Perfil suportado: uma `Signature` filha do elemento raiz, referência `#Id` para um
//! elemento sem assinatura própria, C14N 1.0 inclusiva, RSA-SHA256 e digest SHA-256.
//! Com a feature `nfe-sha1`, o perfil legado RSA-SHA1/SHA-1 da NF-e pode ser
//! selecionado explicitamente em [`assinar_com_perfil`] e [`verificar_com_perfil`].
//! As funções [`assinar`] e [`verificar`] continuam exigindo SHA-256, mesmo quando
//! a feature está habilitada. Não há detecção de perfil nem fallback automático.
//! Outros algoritmos e C14N com comentários são recusados.
//!
//! A chave privada nunca entra neste crate: a assinatura é pedida ao [`Signer`].
//! `CertificadoA1` implementa o trait. A verificação não valida confiança de cadeia.
//!
//! Status: implementação da Etapa 1 (`docs/plano-mvp.md`, issue #9).

mod assinatura;
mod erro;
mod perfil;
mod verificacao;

pub use assinatura::{Signer, assinar, assinar_com_perfil};
pub use erro::ErroDsig;
pub use perfil::PerfilAssinatura;
pub use verificacao::{Verificacao, verificar, verificar_com_perfil};

/// Namespace XMLDSig.
pub const NS_DSIG: &str = "http://www.w3.org/2000/09/xmldsig#";
/// C14N 1.0 inclusiva, sem comentários.
pub const ALG_C14N: &str = "http://www.w3.org/TR/2001/REC-xml-c14n-20010315";
/// Transformada enveloped-signature.
pub const ALG_ENVELOPED: &str = "http://www.w3.org/2000/09/xmldsig#enveloped-signature";
/// Digest SHA-256.
pub const ALG_SHA256: &str = "http://www.w3.org/2001/04/xmlenc#sha256";
/// RSASSA-PKCS1-v1_5 com SHA-256.
pub const ALG_RSA_SHA256: &str = "http://www.w3.org/2001/04/xmldsig-more#rsa-sha256";
/// Digest SHA-1 do contrato legado da NF-e; uso explícito via `nfe-sha1`.
#[cfg(feature = "nfe-sha1")]
pub const ALG_SHA1: &str = "http://www.w3.org/2000/09/xmldsig#sha1";
/// RSA-SHA1 do contrato legado da NF-e; uso explícito via `nfe-sha1`.
#[cfg(feature = "nfe-sha1")]
pub const ALG_RSA_SHA1: &str = "http://www.w3.org/2000/09/xmldsig#rsa-sha1";
