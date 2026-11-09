//! Tipos de domínio puros do carimbar. Sem I/O, sem `f64` em valores.
//!
//! - [`Cnpj`]: 14 posições com DV (numérico e alfanumérico, IN RFB 2.229/2024).
//! - [`ChaveAcesso`]: 44 posições com cDV (módulo 11).
//! - [`Ambiente`]: sem `Default`; produção exige [`ConfirmacaoProducao`].
//! - [`Dinheiro`]: `rust_decimal::Decimal` com até 2 casas.

mod ambiente;
mod chave;
mod cnpj;
mod dinheiro;
mod erro;
mod modulo11;

pub use ambiente::{Ambiente, ConfirmacaoProducao, TpAmb};
pub use chave::{ChaveAcesso, ModeloDocumento};
pub use cnpj::Cnpj;
pub use dinheiro::Dinheiro;
pub use erro::ErroTipo;
pub use rust_decimal::Decimal;
