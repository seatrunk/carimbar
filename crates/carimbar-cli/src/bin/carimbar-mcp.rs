//! Servidor MCP stdio do carimbar.
//!
//! Status: implementação da Etapa 4 (`docs/plano-mvp.md`, issue #17).

use carimbar_cli::mcp::{Config, ServidorCarimbar};
use rmcp::ServiceExt;

#[tokio::main]
async fn main() {
    if let Err(erro) = executar().await {
        eprintln!("carimbar-mcp: {erro}");
        std::process::exit(1);
    }
}

async fn executar() -> Result<(), String> {
    let config = Config::de_ambiente()?;
    let servidor = ServidorCarimbar::novo(config);
    let servico = servidor
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|erro| format!("falha ao iniciar MCP: {erro}"))?;
    servico
        .waiting()
        .await
        .map_err(|erro| format!("falha no servidor MCP: {erro}"))?;
    Ok(())
}
