use thiserror::Error;

/// Erro ao assinar ou verificar XMLDSig.
///
/// As mensagens não incluem chave privada nem conteúdo do certificado.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ErroDsig {
    /// O documento não tem assinatura.
    #[error("documento sem assinatura XMLDSig")]
    SemAssinatura,
    /// O documento tem mais de uma assinatura, ou assinaturas fora do perfil.
    #[error("documento com {0} assinaturas XMLDSig; o perfil aceita exatamente uma")]
    MultiplasAssinaturas(usize),
    /// A assinatura não é filha do elemento raiz, que é o único lugar do perfil.
    #[error("assinatura XMLDSig fora do elemento raiz")]
    AssinaturaForaDaRaiz,
    /// Estrutura XMLDSig ausente ou malformada.
    #[error("estrutura XMLDSig inválida: {0}")]
    Estrutura(String),
    /// Algoritmo fora do perfil suportado.
    #[error("algoritmo não suportado em {campo}: {uri}")]
    AlgoritmoNaoSuportado {
        /// Elemento onde o algoritmo aparece.
        campo: &'static str,
        /// URI informada.
        uri: String,
    },
    /// A referência não aponta para um elemento local por `#Id`.
    #[error("referência inválida: {0}")]
    ReferenciaInvalida(String),
    /// Nenhum elemento tem o `Id` referenciado.
    #[error("nenhum elemento com Id \"{0}\"")]
    ReferenciaNaoEncontrada(String),
    /// Mais de um elemento tem o mesmo `Id`; a referência seria ambígua.
    #[error("Id \"{id}\" aparece em {ocorrencias} elementos")]
    ReferenciaAmbigua {
        /// Id duplicado.
        id: String,
        /// Quantidade de ocorrências.
        ocorrencias: usize,
    },
    /// O digest do elemento referenciado não confere: o conteúdo foi alterado.
    #[error("digest divergente: o conteúdo assinado foi alterado")]
    DigestDivergente,
    /// A assinatura RSA não confere com a chave do certificado.
    #[error("assinatura RSA inválida")]
    AssinaturaInvalida,
    /// O certificado do KeyInfo não é um X.509 válido.
    #[error("certificado do KeyInfo inválido")]
    CertificadoInvalido,
    /// Valor base64 inválido.
    #[error("valor base64 inválido em {0}")]
    Base64(&'static str),
    /// Falha na canonicalização do XML.
    #[error("falha na canonicalização: {0}")]
    Canonicalizacao(String),
    /// O assinante (por exemplo, um certificado A1) falhou ao assinar.
    #[error("falha no assinante: {0}")]
    Assinante(String),
}

impl From<carimbar_xml::ErroXml> for ErroDsig {
    fn from(erro: carimbar_xml::ErroXml) -> Self {
        Self::Canonicalizacao(erro.to_string())
    }
}
