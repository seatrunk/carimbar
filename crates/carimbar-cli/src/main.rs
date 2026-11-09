//! CLI do carimbar.
//!
//! A saída padrão é o canal dos comandos; logs e erros vão para stderr.
//! O código de verdade mora na biblioteca interna (`carimbar_cli`), compartilhada
//! com o binário `carimbar-mcp`.
//!
//! Status: implementação da Etapa 4 (`docs/plano-mvp.md`, issue #16).

use carimbar_cli::cli;

fn main() {
    let codigo = match cli::executar() {
        Ok(()) => 0,
        Err(erro) => {
            eprintln!("carimbar: {erro}");
            erro.codigo()
        }
    };
    std::process::exit(i32::from(codigo));
}
