//! Teste de protocolo MCP sobre stdio: initialize, tools/list e disciplina de stdout.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const KEK: &str = "0909090909090909090909090909090909090909090909090909090909090909";

fn diretorio() -> PathBuf {
    let unico = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duracao| duracao.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!("carimbar-mcp-protocolo-{unico}"));
    std::fs::create_dir_all(&dir).expect("diretório");
    dir
}

fn ler_resposta(receptor: &mpsc::Receiver<String>, id: u64) -> Value {
    for _ in 0..20 {
        let linha = receptor
            .recv_timeout(Duration::from_secs(10))
            .expect("resposta do servidor MCP");
        let valor: Value = serde_json::from_str(&linha).expect("stdout deve ser JSON");
        if valor.get("id").and_then(Value::as_u64) == Some(id) {
            return valor;
        }
    }
    panic!("resposta para id {id} não chegou");
}

#[test]
fn initialize_e_tools_list_respondem_em_stdout_limpo() {
    let dir = diretorio();
    let mut filho = Command::new(env!("CARGO_BIN_EXE_carimbar-mcp"))
        .env("CARIMBAR_KEK", KEK)
        .env("CARIMBAR_COFRE", dir.join("cofre.db"))
        .env("CARIMBAR_JOURNAL", dir.join("journal.db"))
        .env("CARIMBAR_MCP_TTL_SEGUNDOS", "60")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("inicia carimbar-mcp");

    let stdout = filho.stdout.take().expect("stdout");
    let (receptor, recebido) = mpsc::channel();
    std::thread::spawn(move || {
        let leitor = BufReader::new(stdout);
        for linha in leitor.lines() {
            let Ok(linha) = linha else { break };
            if receptor.send(linha).is_err() {
                break;
            }
        }
    });

    let mut entrada = filho.stdin.take().expect("stdin");
    let initialize = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2026-07-28",
            "capabilities": {},
            "clientInfo": {"name": "teste-carimbar", "version": "0.1.0"}
        }
    });
    writeln!(entrada, "{initialize}").expect("initialize");
    entrada.flush().expect("flush");
    let resposta = ler_resposta(&recebido, 1);
    assert!(resposta["result"]["protocolVersion"].is_string());
    assert!(resposta["result"]["capabilities"]["tools"].is_object());

    writeln!(
        entrada,
        "{}",
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
    )
    .expect("initialized");
    entrada.flush().expect("flush");

    let listar = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}});
    writeln!(entrada, "{listar}").expect("tools/list");
    entrada.flush().expect("flush");
    let resposta = ler_resposta(&recebido, 2);
    let ferramentas = resposta["result"]["tools"].as_array().expect("tools");
    let nomes = ferramentas
        .iter()
        .filter_map(|ferramenta| ferramenta["name"].as_str())
        .collect::<Vec<_>>();
    for esperado in [
        "preparar_emissao",
        "confirmar_emissao",
        "consultar_nfse",
        "preparar_cancelamento",
        "confirmar_cancelamento",
        "gerar_danfse",
        "sincronizar_adn",
        "consultar_convenio",
        "consultar_aliquota",
        "estado_operacao",
    ] {
        assert!(nomes.contains(&esperado), "{esperado}: {nomes:?}");
    }
    assert!(!nomes.iter().any(|nome| nome.contains("importar")));
    assert!(!nomes.iter().any(|nome| nome.contains("promover")));
    for ferramenta in ferramentas {
        assert!(ferramenta["inputSchema"].is_object(), "{ferramenta}");
    }

    drop(entrada);
    let _ = filho.kill();
    let _ = filho.wait();
    while let Ok(linha) = recebido.try_recv() {
        let valor: Value = serde_json::from_str(&linha).expect("stdout deve continuar JSON");
        assert!(valor.get("jsonrpc").is_some(), "{valor}");
    }
}
