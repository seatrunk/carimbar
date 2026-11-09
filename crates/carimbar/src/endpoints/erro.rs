use thiserror::Error;

/// Falha ao resolver um endpoint ou preencher sua URL.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ErroEndpoints {
    /// Não há URL publicada para a combinação pedida.
    #[error("serviço {servico} indisponível para {contexto} em {ambiente}")]
    NaoDisponivel {
        /// Serviço pedido.
        servico: &'static str,
        /// UF, autorizador ou modelo que delimitou a busca.
        contexto: String,
        /// Ambiente pedido.
        ambiente: &'static str,
    },
    /// Sigla ou código de UF desconhecido.
    #[error("UF inválida: {0:?}")]
    Uf(String),
    /// Placeholder da URL não informado.
    #[error("parâmetro {0} ausente")]
    ParametroAusente(String),
    /// A URL resolvida contém um placeholder que não foi preenchido.
    #[error("placeholder não preenchido na URL: {0}")]
    Placeholder(String),
    /// A URL resolvida contém um placeholder desconhecido para o serviço.
    #[error("placeholder desconhecido para o serviço: {0}")]
    PlaceholderDesconhecido(String),
}
