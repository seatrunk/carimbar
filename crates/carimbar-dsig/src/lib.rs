//! XMLDSig enveloped com RSA-SHA256: assinatura via trait [`Signer`] e verificação
//! com validação de digest, algoritmos, `Id` e assinatura.
//!
//! Perfil suportado: uma `Signature` filha do elemento raiz, referência `#Id` para um
//! elemento sem assinatura própria, C14N 1.0 inclusiva, RSA-SHA256 e digest SHA-256.
//! Algoritmos fora desse perfil são recusados. Em particular, SHA-1 e C14N com
//! comentários não são aceitos.
//! O XSD de NF-e vendorizado exige RSA-SHA1: esse perfil ainda não é suportado.
//!
//! A chave privada nunca entra neste crate: a assinatura é pedida ao [`Signer`].
//! `CertificadoA1` implementa o trait. A verificação não valida confiança de cadeia.
//!
//! Status: entrega parcial da Etapa 1 (`docs/plano-mvp.md`, issue #9).

mod assinatura;
mod erro;
mod verificacao;

pub use assinatura::{Signer, assinar};
pub use erro::ErroDsig;
pub use verificacao::{Verificacao, verificar};

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
