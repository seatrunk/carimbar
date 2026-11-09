use thiserror::Error;

/// Erro de validação de um tipo de domínio.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ErroTipo {
    /// Tamanho diferente do esperado (após remover a máscara).
    #[error("{campo}: esperado {esperado} posições, recebido {recebido}")]
    Tamanho {
        /// Campo validado.
        campo: &'static str,
        /// Tamanho esperado.
        esperado: usize,
        /// Tamanho recebido.
        recebido: usize,
    },
    /// Caractere fora do alfabeto permitido.
    #[error("{campo}: caractere inválido {caractere:?} na posição {posicao}")]
    Caractere {
        /// Campo validado.
        campo: &'static str,
        /// Caractere encontrado.
        caractere: char,
        /// Posição (0-based) após remover a máscara.
        posicao: usize,
    },
    /// Dígito verificador não confere.
    #[error("{campo}: dígito verificador inválido")]
    DigitoVerificador {
        /// Campo validado.
        campo: &'static str,
    },
    /// Sequência repetida (ex.: 00000000000000), rejeitada pela RFB.
    #[error("{campo}: sequência repetida não é válida")]
    Repetido {
        /// Campo validado.
        campo: &'static str,
    },
    /// Campo interno com valor fora do domínio (UF, mês, modelo...).
    #[error("{campo}: valor fora do domínio: {valor}")]
    Dominio {
        /// Campo validado.
        campo: &'static str,
        /// Valor recebido.
        valor: String,
    },
    /// Valor monetário inválido.
    #[error("valor monetário inválido: {0}")]
    Dinheiro(String),
}
