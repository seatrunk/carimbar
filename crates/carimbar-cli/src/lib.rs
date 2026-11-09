//! Biblioteca interna do CLI e do servidor MCP do carimbar.
//!
//! Existe para compartilhar o código entre os dois binários (`carimbar` e
//! `carimbar-mcp`). A API não é pensada para uso por terceiros.

#![allow(missing_docs)]
// A CLI imprime o resultado dos comandos na saída padrão por desenho.
#![allow(clippy::print_stdout)]

pub mod cli;
pub mod replay;

#[cfg(feature = "mcp")]
pub mod mcp;
