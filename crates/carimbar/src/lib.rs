//! Fachada pública do carimbar: `Emissor`, journal local, idempotência e
//! controles de produção.
//!
//! # Fluxo
//!
//! ```no_run
//! use carimbar::Journal;
//! use carimbar_types::Cnpj;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let empresa = Cnpj::parse("00000001000136")?;
//! let journal = Journal::abrir("journal.db")?;
//! journal.garantir_empresa(&empresa)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Controles
//!
//! - Produção só é habilitada por [`Emissor::promover_producao`], com
//!   `ConfirmacaoProducao` vinculada ao CNPJ e certificado da empresa.
//! - [`KillSwitchAmbiente`] lê `CARIMBAR_PRODUCAO` antes de qualquer operação
//!   de produção; o valor `0` bloqueia.
//! - O journal é local e não processa segredos: só XML fiscal, hashes,
//!   identificadores e estados.
//!
//! Status: implementação da Etapa 3 (`docs/plano-mvp.md`, issue #13).

#![deny(missing_docs)]

mod emissor;
mod erro;
mod journal;

pub use emissor::{
    Emissor, KillSwitch, KillSwitchAmbiente, KillSwitchFixo, OperacaoPreparada, ResultadoNfse,
};
pub use erro::ErroEmissor;
pub use journal::{EstadoOperacao, Journal, Operacao, TipoOperacao, hash_conteudo};
