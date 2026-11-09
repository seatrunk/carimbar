use crate::dsig::{ALG_RSA_SHA256, ALG_SHA256};

/// Perfil criptográfico escolhido pelo chamador, sem detecção ou fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PerfilAssinatura {
    /// RSA-SHA256 e digest SHA-256, usados por [`carimbar::dsig::assinar`] e [`carimbar::dsig::verificar`].
    Sha256,
    /// RSA-SHA1 e digest SHA-1 fixados no XSD NF-e PL_010b v1.30.
    /// Requer a feature `nfe-sha1` e escolha explícita; não é o padrão.
    #[cfg(feature = "nfe-sha1")]
    NfeSha1,
}

impl PerfilAssinatura {
    pub(crate) fn assinatura_uri(self) -> &'static str {
        match self {
            Self::Sha256 => ALG_RSA_SHA256,
            #[cfg(feature = "nfe-sha1")]
            Self::NfeSha1 => crate::dsig::ALG_RSA_SHA1,
        }
    }

    pub(crate) fn digest_uri(self) -> &'static str {
        match self {
            Self::Sha256 => ALG_SHA256,
            #[cfg(feature = "nfe-sha1")]
            Self::NfeSha1 => crate::dsig::ALG_SHA1,
        }
    }

    pub(crate) fn digest(self, dados: &[u8]) -> Vec<u8> {
        use sha2::{Digest, Sha256};
        match self {
            Self::Sha256 => Sha256::digest(dados).to_vec(),
            #[cfg(feature = "nfe-sha1")]
            Self::NfeSha1 => ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, dados)
                .as_ref()
                .to_vec(),
        }
    }

    pub(crate) fn verificador(self) -> &'static ring::signature::RsaParameters {
        match self {
            Self::Sha256 => &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            #[cfg(feature = "nfe-sha1")]
            Self::NfeSha1 => &ring::signature::RSA_PKCS1_2048_8192_SHA1_FOR_LEGACY_USE_ONLY,
        }
    }
}
