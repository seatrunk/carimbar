//! Erros da camada NFS-e Nacional.
//!
//! As mensagens não incluem chave privada, senha nem conteúdo de certificado.
//! Falhas de transporte preservam a distinção entre "não enviado" e "resultado
//! desconhecido" definida por `carimbar-transport`.

use carimbar_dsig::ErroDsig;
use carimbar_endpoints::ErroEndpoints;
use carimbar_transport::ErroTransporte;
use carimbar_xml::ErroXml;
use thiserror::Error;

/// Falha ao montar, assinar, enviar ou interpretar uma operação da NFS-e.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ErroNfse {
    /// Campo de domínio inválido.
    #[error("{campo}: {detalhe}")]
    Campo {
        /// Campo validado.
        campo: &'static str,
        /// Descrição do problema.
        detalhe: String,
    },
    /// URL do serviço não pôde ser resolvida ou preenchida.
    #[error("endpoint: {0}")]
    Endpoint(#[from] ErroEndpoints),
    /// XML inválido ao montar, ler ou validar.
    #[error("XML: {0}")]
    Xml(#[from] ErroXml),
    /// Falha de assinatura XMLDSig.
    #[error("assinatura: {0}")]
    Assinatura(#[from] ErroDsig),
    /// A requisição comprovadamente não foi escrita; repetir é seguro.
    #[error("falha antes do envio: {0}")]
    AntesDoEnvio(#[from] ErroTransporte),
    /// Falha de transporte não classificada pelo contrato atual.
    #[error("transporte: {0}")]
    Transporte(String),
    /// A requisição foi escrita, mas a resposta não pôde ser lida. Consulte o
    /// estado antes de repetir uma emissão.
    #[error("resultado desconhecido: {0}")]
    ResultadoDesconhecido(ErroTransporte),
    /// Resposta HTTP com status inesperado.
    #[error("HTTP {status}: {corpo}")]
    Http {
        /// Código de status.
        status: u16,
        /// Corpo recebido, truncado para diagnóstico.
        corpo: String,
    },
    /// JSON ausente, malformado ou fora da estrutura esperada.
    #[error("resposta JSON inválida: {0}")]
    Json(String),
    /// Falha ao comprimir ou descomprimir o XML transportado.
    #[error("gzip/base64: {0}")]
    Compressao(String),
    /// O recurso consultado não existe no ambiente.
    #[error("documento não encontrado")]
    NaoEncontrada,
    /// A DPS enviada já foi processada; consulte pelo identificador.
    #[error("DPS já processada: {0}")]
    Duplicidade(String),
    /// XML não passou na validação XSD.
    #[cfg(feature = "xsd")]
    #[error("XML inválido para o XSD: {0}")]
    Xsd(String),
}

impl ErroNfse {
    pub(crate) fn campo(campo: &'static str, detalhe: impl Into<String>) -> Self {
        Self::Campo {
            campo,
            detalhe: detalhe.into(),
        }
    }

    /// `true` quando repetir a operação é comprovadamente seguro.
    pub fn seguro_para_reenviar(&self) -> bool {
        matches!(self, Self::AntesDoEnvio(_))
    }
}
