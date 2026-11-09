//! Erros da fachada e do journal.

use crate::danfe::ErroDanfe;
use crate::dsig::ErroDsig;
use crate::nfe::ErroNfe;
use crate::nfse::ErroNfse;
use crate::types::Cnpj;
use crate::vault::ErroCofre;
use crate::xml::ErroXml;
use thiserror::Error;

/// Falha ao preparar, confirmar ou reconciliar uma operação fiscal.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ErroEmissor {
    /// Falha no cofre.
    #[error("cofre: {0}")]
    Cofre(#[from] ErroCofre),
    /// Falha na camada NFS-e.
    #[error("NFS-e: {0}")]
    Nfse(#[from] ErroNfse),
    /// Falha na camada NF-e.
    #[error("NF-e: {0}")]
    Nfe(#[from] ErroNfe),
    /// Falha ao ler ou montar XML.
    #[error("XML: {0}")]
    Xml(#[from] ErroXml),
    /// Falha de assinatura.
    #[error("assinatura: {0}")]
    Assinatura(#[from] ErroDsig),
    /// Falha na geração de PDF.
    #[error("PDF: {0}")]
    Danfe(#[from] ErroDanfe),
    /// Falha no journal local.
    #[error("journal: {0}")]
    Journal(String),
    /// A empresa ainda não foi configurada no journal.
    #[error("empresa {0} não cadastrada")]
    EmpresaNaoEncontrada(Cnpj),
    /// A operação não existe.
    #[error("operação {0} não encontrada")]
    OperacaoNaoEncontrada(i64),
    /// A operação pertence a outra empresa, ambiente ou tipo.
    #[error("operação {0} pertence a outra empresa, ambiente ou tipo")]
    OperacaoDeOutraEmpresa(i64),
    /// A mesma chave de idempotência foi usada com conteúdo diferente.
    #[error("conflito de idempotência em {chave}: conteúdo difere da operação existente")]
    ConflitoIdempotencia {
        /// Chave da operação.
        chave: String,
    },
    /// O kill switch de produção está ativo.
    #[error("produção bloqueada: produção exige CARIMBAR_PRODUCAO=1")]
    ProducaoBloqueada,
    /// A confirmação de produção não corresponde ao CNPJ da empresa.
    #[error("confirmação de produção não corresponde ao CNPJ {0}")]
    ConfirmacaoDivergente(Cnpj),
    /// Não há certificado utilizável para a empresa.
    #[error("certificado ausente para a empresa {0}")]
    CertificadoAusente(Cnpj),
    /// O certificado não pertence à empresa.
    #[error("certificado pertence a outro CNPJ")]
    CertificadoDeOutraEmpresa,
    /// A operação não está em um estado que aceite a ação.
    #[error("operação {id} no estado {estado} não aceita {acao}")]
    EstadoInvalido {
        /// Operação.
        id: i64,
        /// Estado atual.
        estado: &'static str,
        /// Ação tentada.
        acao: &'static str,
    },
    /// O conteúdo não corresponde à empresa ou ao ambiente do emissor.
    #[error("{campo} não corresponde à empresa ou ao ambiente do emissor")]
    Divergencia {
        /// Campo divergente.
        campo: &'static str,
    },
    /// A operação foi enviada e precisa de reconciliação antes de novo envio.
    #[error("operação {0} tem resultado desconhecido; reconcilie antes de reenviar")]
    ReconciliacaoNecessaria(i64),
    /// A operação já foi processada por outra instância.
    #[error("operação {0} já está em processamento por outra instância")]
    EmAndamento(i64),
}

impl From<rusqlite::Error> for ErroEmissor {
    fn from(erro: rusqlite::Error) -> Self {
        Self::Journal(erro.to_string())
    }
}
