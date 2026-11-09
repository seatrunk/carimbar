//! CLI do carimbar.
//!
//! A saída padrão é o canal dos comandos; logs e erros vão para stderr.
#![allow(clippy::print_stdout)]
//!
//! Status: implementação da Etapa 4 (`docs/plano-mvp.md`, issue #16).

mod cli;
mod replay;

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
