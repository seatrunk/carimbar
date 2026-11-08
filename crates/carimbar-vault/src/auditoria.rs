//! Auditoria encadeada por hash.
//!
//! Cada registro guarda o hash do anterior, o instante, o evento, a empresa
//! (quando aplicável) e um detalhe textual. A cadeia detecta alteração,
//! remoção de registro intermediário ou reordenação feita sem recomputar todos
//! os hashes. Como não há âncora externa, quem tiver acesso de escrita ao
//! arquivo pode reescrever a cadeia inteira; para garantia mais forte, ancore
//! periodicamente o último hash fora do cofre.

use std::time::SystemTime;

use carimbar_types::Cnpj;
use sha2::{Digest, Sha256};

use crate::erro::ErroCofre;

/// Eventos registrados pelo cofre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum EventoAuditoria {
    /// Cofre criado.
    CofreCriado,
    /// Primeira DEK de uma empresa criada.
    EmpresaCriada,
    /// Certificado importado.
    CertificadoImportado,
    /// Certificado removido.
    CertificadoRemovido,
    /// DEK de uma empresa rotacionada.
    DekRotacionada,
    /// KEK trocada e todas as DEKs reembrulhadas.
    KekRotacionada,
}

impl EventoAuditoria {
    /// Nome estável gravado no arquivo e usado no hash.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CofreCriado => "cofre_criado",
            Self::EmpresaCriada => "empresa_criada",
            Self::CertificadoImportado => "certificado_importado",
            Self::CertificadoRemovido => "certificado_removido",
            Self::DekRotacionada => "dek_rotacionada",
            Self::KekRotacionada => "kek_rotacionada",
        }
    }

    pub(crate) fn de_str(texto: &str) -> Result<Self, ErroCofre> {
        match texto {
            "cofre_criado" => Ok(Self::CofreCriado),
            "empresa_criada" => Ok(Self::EmpresaCriada),
            "certificado_importado" => Ok(Self::CertificadoImportado),
            "certificado_removido" => Ok(Self::CertificadoRemovido),
            "dek_rotacionada" => Ok(Self::DekRotacionada),
            "kek_rotacionada" => Ok(Self::KekRotacionada),
            outro => Err(ErroCofre::Formato(format!(
                "evento de auditoria desconhecido: {outro}"
            ))),
        }
    }
}

/// Registro de auditoria já lido do cofre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistroAuditoria {
    /// Posição no arquivo.
    pub seq: i64,
    /// Instante do evento.
    pub em: SystemTime,
    /// Tipo do evento.
    pub evento: EventoAuditoria,
    /// Empresa envolvida, quando o evento é por empresa.
    pub empresa: Option<Cnpj>,
    /// Detalhe textual não secreto.
    pub detalhe: String,
    /// Hash do registro, em hexadecimal.
    pub hash: String,
}

/// Hash inicial da cadeia (32 bytes zero).
pub(crate) fn hash_inicial() -> [u8; 32] {
    [0_u8; 32]
}

/// Calcula o hash do registro a partir do anterior e dos campos canonicalizados.
///
/// Cada campo textual é prefixado pelo tamanho em big-endian, o que evita
/// ambiguidade na concatenação.
pub(crate) fn calcular_hash(
    anterior: &[u8; 32],
    em: i64,
    evento: &str,
    empresa: Option<&str>,
    detalhe: &str,
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(anterior);
    digest.update(em.to_be_bytes());
    for campo in [
        evento.as_bytes(),
        empresa.unwrap_or_default().as_bytes(),
        detalhe.as_bytes(),
    ] {
        digest.update((campo.len() as u64).to_be_bytes());
        digest.update(campo);
    }
    digest.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_muda_com_qualquer_campo() {
        let base = hash_inicial();
        let referencia = calcular_hash(&base, 1, "evento", Some("empresa"), "detalhe");
        assert_ne!(
            referencia,
            calcular_hash(&base, 2, "evento", Some("empresa"), "detalhe")
        );
        assert_ne!(
            referencia,
            calcular_hash(&base, 1, "outro", Some("empresa"), "detalhe")
        );
        assert_ne!(
            referencia,
            calcular_hash(&base, 1, "evento", None, "detalhe")
        );
        assert_ne!(
            referencia,
            calcular_hash(&base, 1, "evento", Some("empresa"), "outro")
        );
    }

    #[test]
    fn campos_delimitados_nao_colidem_na_concatenacao() {
        let base = hash_inicial();
        assert_ne!(
            calcular_hash(&base, 1, "ab", Some("c"), ""),
            calcular_hash(&base, 1, "a", Some("bc"), "")
        );
    }

    #[test]
    fn eventos_serializam_e_voltam() {
        for evento in [
            EventoAuditoria::CofreCriado,
            EventoAuditoria::EmpresaCriada,
            EventoAuditoria::CertificadoImportado,
            EventoAuditoria::CertificadoRemovido,
            EventoAuditoria::DekRotacionada,
            EventoAuditoria::KekRotacionada,
        ] {
            assert_eq!(
                EventoAuditoria::de_str(evento.as_str()).expect("evento"),
                evento
            );
        }
        assert!(EventoAuditoria::de_str("desconhecido").is_err());
    }
}
