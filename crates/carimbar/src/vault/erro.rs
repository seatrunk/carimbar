use crate::cert::ErroCertificado;
use thiserror::Error;

/// Falha ao operar o cofre.
///
/// As mensagens nunca incluem senha, PFX, DEK ou KEK. Elas são adequadas para
/// log, mas o chamador não deve registrar o conteúdo dos segredos por conta
/// própria.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ErroCofre {
    /// Falha do SQLite local.
    #[error("falha no banco local: {0}")]
    Banco(String),
    /// O arquivo não é um cofre `carimbar-vault` reconhecido.
    #[error("formato de cofre inválido: {0}")]
    Formato(String),
    /// O cofre já existe no caminho informado.
    #[error("já existe um cofre nesse caminho")]
    CofreJaExiste,
    /// A chave mestra não autentica o verificador do cofre.
    #[error("chave mestra incorreta")]
    ChaveIncorreta,
    /// Um registro cifrado não autentica com a chave e o contexto esperados.
    #[error("registro cifrado adulterado ou incompatível")]
    RegistroAdulterado,
    /// A empresa ainda não existe no cofre.
    #[error("empresa não cadastrada no cofre")]
    EmpresaNaoEncontrada,
    /// Não há certificado com esse identificador para a empresa.
    #[error("certificado não encontrado")]
    CertificadoNaoEncontrado,
    /// Já existe um certificado com esse identificador para a empresa.
    #[error("certificado já importado com esse identificador")]
    JaExiste,
    /// O certificado A1 foi recusado.
    #[error("certificado recusado: {0}")]
    Certificado(#[from] ErroCertificado),
    /// Falha criptográfica inesperada.
    #[error("falha criptográfica")]
    Cripto,
    /// O gerador de números aleatórios do sistema falhou.
    #[error("gerador aleatório indisponível: {0}")]
    Aleatorio(String),
    /// Parâmetro de configuração inválido.
    #[error("parâmetro inválido: {0}")]
    Parametro(String),
    /// A cadeia de auditoria não confere na posição indicada.
    #[error("auditoria inconsistente na posição {0}")]
    Auditoria(i64),
}

impl From<rusqlite::Error> for ErroCofre {
    fn from(erro: rusqlite::Error) -> Self {
        Self::Banco(erro.to_string())
    }
}
