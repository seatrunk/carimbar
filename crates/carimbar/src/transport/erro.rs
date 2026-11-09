use thiserror::Error;

/// Falha de transporte antes de existir uma resposta HTTP utilizável.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ErroTransporte {
    /// URL ausente, sem `https` ou com caractere inválido.
    #[error("URL inválida: {0}")]
    Url(String),
    /// Resolução DNS falhou.
    #[error("falha ao resolver o host {host}")]
    Dns {
        /// Host consultado.
        host: String,
    },
    /// Conexão TCP recusada ou interrompida.
    #[error("falha de conexão com {host}:{porta}")]
    Conexao {
        /// Host conectado.
        host: String,
        /// Porta conectada.
        porta: u16,
    },
    /// Handshake TLS recusado (cadeia, identidade, versão ou algoritmo).
    #[error("falha de TLS: {0}")]
    Tls(String),
    /// Tempo limite de conexão, escrita ou leitura.
    #[error("tempo limite esgotado em {0}")]
    Timeout(&'static str),
    /// Resposta HTTP fora do formato esperado.
    #[error("protocolo HTTP inválido: {0}")]
    Protocolo(String),
    /// Falha de I/O em uma conexão já estabelecida.
    #[error("falha de I/O durante {fase}: {detalhe}")]
    Io {
        /// Operação que falhou (`leitura` ou `escrita`).
        fase: &'static str,
        /// Descrição do erro do sistema.
        detalhe: String,
    },
    /// Resposta maior que o limite configurado.
    #[error("resposta excedeu o limite de {limite} bytes")]
    RespostaGrande {
        /// Limite configurado.
        limite: usize,
    },
    /// Status HTTP recebido; o corpo continua disponível para o chamador.
    #[error("status HTTP {status}")]
    Status {
        /// Código de status.
        status: u16,
    },
    /// Configuração local inválida (raiz, identidade ou opções).
    #[error("configuração inválida: {0}")]
    Configuracao(String),
}

impl ErroTransporte {
    /// `true` se valer a pena repetir uma operação segura.
    pub fn transitorio(&self) -> bool {
        matches!(
            self,
            Self::Dns { .. } | Self::Conexao { .. } | Self::Timeout(_) | Self::Io { .. }
        )
    }
}

/// Resultado de um envio que pode já ter sido processado pelo servidor.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ErroEnvio {
    /// A requisição comprovadamente não chegou a ser escrita; repetir é seguro.
    #[error("falha antes do envio: {0}")]
    AntesDoEnvio(ErroTransporte),
    /// A requisição pode ter sido processada; não reenvie sem consultar o estado.
    #[error("resultado desconhecido: {0}")]
    ResultadoDesconhecido(ErroTransporte),
}

impl ErroEnvio {
    /// `true` quando repetir o envio é comprovadamente seguro.
    pub fn seguro_para_reenviar(&self) -> bool {
        matches!(self, Self::AntesDoEnvio(_))
    }
}
