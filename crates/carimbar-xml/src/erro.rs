//! Tipos de erro da camada XML.

use thiserror::Error;

/// Erro de leitura, escrita, canonicalização ou validação de XML.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ErroXml {
    /// A entrada excedeu um limite de recursos.
    #[error("limite de {limite} excedido: {valor} > {maximo}")]
    Limite {
        /// Nome do limite.
        limite: &'static str,
        /// Valor observado.
        valor: usize,
        /// Valor máximo permitido.
        maximo: usize,
    },
    /// XML malformado.
    #[error("XML malformado na posição {posicao}: {detalhe}")]
    Sintaxe {
        /// Posição em bytes no documento.
        posicao: u64,
        /// Descrição do problema.
        detalhe: String,
    },
    /// DTD encontrada. XML fiscal não pode carregar DTD.
    #[error("DTD não é permitida em XML fiscal (posição {posicao})")]
    DtdProibida {
        /// Posição em bytes no documento.
        posicao: u64,
    },
    /// Entidade não suportada.
    #[error("entidade &{nome}; não suportada (posição {posicao})")]
    Entidade {
        /// Nome da entidade.
        nome: String,
        /// Posição em bytes no documento.
        posicao: u64,
    },
    /// Namespace inválido ou prefixo não declarado.
    #[error("namespace inválido: {0}")]
    Namespace(String),
    /// Estrutura do documento inválida.
    #[error("estrutura XML inválida: {0}")]
    Estrutura(String),
    /// Codificação diferente de UTF-8.
    #[error("codificação não suportada: {0}; use UTF-8")]
    Codificacao(String),
    /// Falha ao canonicalizar.
    #[error("falha ao canonicalizar: {0}")]
    Canonicalizacao(String),
    /// O documento não é válido para o XSD informado.
    #[cfg(feature = "xsd")]
    #[error("XML inválido para o XSD: {0}")]
    Xsd(String),
}

impl ErroXml {
    pub(crate) fn limite(limite: &'static str, valor: usize, maximo: usize) -> Self {
        Self::Limite {
            limite,
            valor,
            maximo,
        }
    }

    pub(crate) fn sintaxe(posicao: u64, detalhe: impl Into<String>) -> Self {
        Self::Sintaxe {
            posicao,
            detalhe: detalhe.into(),
        }
    }
}
