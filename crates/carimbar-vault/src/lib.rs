//! Cofre multiempresa de certificados A1 com envelope encryption e auditoria.
//!
//! # Modelo de proteção
//!
//! - **DEK por empresa:** cada empresa tem uma chave de dados de 32 bytes.
//! - **Envelope:** o PFX e a senha do certificado são cifrados com AES-256-GCM
//!   sob a DEK, com CNPJ, identificador e versão no contexto autenticado.
//! - **KEK:** a DEK é embrulhada por uma chave mestra que **não** fica no
//!   arquivo. Ela pode vir do chamador (keyring, env ou KMS via
//!   [`ProvedorKek`]) ou ser derivada de uma senha com Argon2id, guardando só o
//!   sal e o custo.
//! - **SQLite:** o arquivo guarda metadados públicos em claro (CNPJ,
//!   identificador, validade, hash do certificado folha e auditoria) e os
//!   segredos cifrados.
//!
//! # Exemplo
//!
//! ```no_run
//! use carimbar_types::Cnpj;
//! use carimbar_vault::{ChaveMestra, Cofre};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let empresa = Cnpj::parse("00000001000136")?;
//! let chave = ChaveMestra::gerar()?;
//! let pfx: Vec<u8> = Vec::new();
//!
//! let mut cofre = Cofre::criar("cofre.db", Box::new(chave))?;
//! cofre.importar(&empresa, "a1-2026", &pfx, "senha-do-pfx")?;
//! for meta in cofre.listar(&empresa)? {
//!     assert!(!meta.id.is_empty());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Limites
//!
//! - O arquivo SQLite não é cifrado como um todo: apenas os segredos. Os
//!   metadados públicos permitem listar certificados sem desbloquear o cofre.
//! - A cadeia de auditoria detecta alteração e remoção intermediária, mas não
//!   prova nada contra quem reescreve o arquivo inteiro; ancore o último hash
//!   fora do cofre para garantia mais forte.
//! - Não há integração com keyring do sistema nem provedores de KMS prontos; a
//!   interface [`ProvedorKek`] está definida para incremento posterior.
//! - O SQLite é local e serializa escritas; o cofre não substitui backup nem
//!   replicação.

#![deny(missing_docs)]

mod auditoria;
mod cofre;
mod crypto;
mod erro;
mod kdf;
mod modelo;

pub use auditoria::{EventoAuditoria, RegistroAuditoria};
pub use cofre::{Cofre, FORMATO};
pub use crypto::{ChaveMestra, ProvedorKek, TAMANHO_CHAVE};
pub use erro::ErroCofre;
pub use kdf::{CustoKdf, Sal, TAMANHO_SAL};
pub use modelo::MetadadosCertificado;
