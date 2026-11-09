use thiserror::Error;

/// Erro ao abrir ou usar um certificado A1.
///
/// As mensagens não incluem senha, chave privada nem conteúdo do arquivo.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ErroCertificado {
    /// Senha incorreta, ou arquivo alterado após a exportação.
    #[error("senha incorreta ou arquivo PKCS#12 alterado")]
    SenhaIncorreta,
    /// O conteúdo não é um PKCS#12 válido.
    #[error("arquivo PKCS#12 inválido")]
    ArquivoInvalido,
    /// O arquivo excede o tamanho máximo aceito.
    #[error("arquivo PKCS#12 maior que {limite} bytes")]
    ArquivoGrande {
        /// Limite em bytes.
        limite: usize,
    },
    /// O PKCS#12 não contém chave privada.
    #[error("PKCS#12 sem chave privada")]
    SemChavePrivada,
    /// Algoritmo de chave que este crate não usa. O A1 aceito é RSA.
    #[error("algoritmo de chave não suportado: OID {0}; use RSA")]
    ChaveNaoSuportada(String),
    /// Chave RSA rejeitada: formato inválido ou tamanho abaixo de 2048 bits.
    #[error("chave RSA inválida ou menor que 2048 bits")]
    ChaveInvalida,
    /// Nenhum certificado da cadeia corresponde à chave privada.
    #[error("nenhum certificado do PKCS#12 corresponde à chave privada")]
    ChaveNaoCorresponde,
    /// Certificado X.509 malformado.
    #[error("certificado X.509 inválido")]
    CertificadoInvalido,
    /// O certificado não tem CNPJ no OID ICP-Brasil 2.16.76.1.3.3 (SAN).
    #[error("certificado sem CNPJ do titular (OID ICP-Brasil 2.16.76.1.3.3)")]
    SemCnpjTitular,
    /// O CNPJ do titular não é o CNPJ pedido.
    #[error("titular do certificado ({encontrado}) não corresponde ao CNPJ {esperado}")]
    TitularIncompativel {
        /// CNPJ pedido.
        esperado: String,
        /// CNPJ encontrado no certificado.
        encontrado: String,
    },
    /// O certificado ainda não está válido na data pedida.
    #[error("certificado ainda não é válido; início em {0}")]
    AindaNaoValido(String),
    /// O certificado expirou na data pedida.
    #[error("certificado expirado em {0}")]
    Expirado(String),
    /// Falha na operação de assinatura.
    #[error("falha ao assinar: {0}")]
    Assinatura(String),
}
