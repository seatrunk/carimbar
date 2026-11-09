//! Fachada pública do carimbar: `Emissor`, journal local, idempotência e
//! controles de produção, sobre os módulos de domínio consolidados.
//!
//! # Módulos e features
//!
//! O crate reúne o que antes eram dez crates de biblioteca como módulos
//! públicos, encadeados pela ordem topológica de dependências:
//!
//! - [`types`] está sempre ligado (tipos de domínio puros, sem I/O);
//! - `cert` e `endpoints` dependem de `types`;
//! - `xml` depende de `types`;
//! - `dsig` depende de `cert` e `xml`;
//! - `transport` depende de `cert`;
//! - `vault` depende de `cert`;
//! - `danfe` depende de `xml`;
//! - `nfse` depende de `dsig`, `endpoints`, `transport` e `xml`;
//! - `nfe` depende de `nfse`, `dsig`, `endpoints`, `transport` e `xml`.
//!
//! A fachada (`Emissor`, `Journal`, `ErroEmissor`) fica atrás da feature
//! `fachada`, que liga todos os módulos acima. A feature `default` liga a
//! `fachada`; `xsd` e `nfe-sha1` (com `nfe-sha1-vendored`) seguem opt-in.
//!
//! # Fluxo
//!
//! ```no_run
//! use carimbar::Journal;
//! use carimbar::types::Cnpj;
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
//!   de produção; só o valor `1` (depois de `trim()`) libera. Ausência, vazio
//!   e qualquer outro valor bloqueiam.
//! - O journal é local e não processa segredos: só XML fiscal, hashes,
//!   identificadores e estados.
//!
//! Status: implementação da Etapa 3 (`docs/plano-mvp.md`, issue #13).

#![deny(missing_docs)]

pub mod types;

#[cfg(feature = "cert")]
pub mod cert;
#[cfg(feature = "danfe")]
pub mod danfe;
#[cfg(feature = "dsig")]
pub mod dsig;
#[cfg(feature = "endpoints")]
pub mod endpoints;
#[cfg(feature = "nfe")]
pub mod nfe;
#[cfg(feature = "nfse")]
pub mod nfse;
#[cfg(feature = "transport")]
pub mod transport;
#[cfg(feature = "vault")]
pub mod vault;
#[cfg(feature = "xml")]
pub mod xml;

#[cfg(feature = "fachada")]
mod emissor;
#[cfg(feature = "fachada")]
mod erro;
#[cfg(feature = "fachada")]
mod journal;

#[cfg(feature = "fachada")]
pub use emissor::{
    Emissor, KillSwitch, KillSwitchAmbiente, KillSwitchFixo, OperacaoPreparada, ResultadoNfe,
    ResultadoNfse, ResultadoSincronizacao,
};
#[cfg(feature = "fachada")]
pub use erro::ErroEmissor;
#[cfg(feature = "fachada")]
pub use journal::{EstadoOperacao, Journal, Operacao, TipoOperacao, hash_conteudo};
