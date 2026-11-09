//! Executor de replay para testes de processo.
//!
//! O arquivo JSON contém uma lista de respostas. Cada requisição consome a
//! primeira resposta cujo `url_contem` casa (ou sem filtro). O recurso é
//! aceito apenas em builds de depuração; em release, o CLI recusa `--replay`.

use carimbar_nfse::Executor;
use carimbar_transport::{ErroEnvio, ErroTransporte, Requisicao, Resposta};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;

/// Resposta simulada.
#[derive(Debug, Clone)]
struct RespostaReplay {
    url_contem: Option<String>,
    status: u16,
    corpo: String,
}

/// Executor que devolve respostas de um arquivo JSON.
#[derive(Debug)]
pub struct ReplayExecutor {
    respostas: Mutex<VecDeque<RespostaReplay>>,
}

impl ReplayExecutor {
    /// Lê o arquivo `{"respostas":[{"url_contem":"/nfse","status":201,"corpo":"..."}]}`.
    pub fn de_arquivo(caminho: impl AsRef<Path>) -> Result<Self, String> {
        if !cfg!(debug_assertions) {
            return Err("replay disponível apenas em build de depuração".to_owned());
        }
        let texto = std::fs::read_to_string(caminho)
            .map_err(|erro| format!("replay: não foi possível ler: {erro}"))?;
        let valor: Value =
            serde_json::from_str(&texto).map_err(|erro| format!("replay: JSON: {erro}"))?;
        let lista = valor
            .get("respostas")
            .and_then(Value::as_array)
            .ok_or_else(|| "replay: campo respostas ausente".to_owned())?;
        let mut respostas = VecDeque::new();
        for item in lista {
            let objeto = item
                .as_object()
                .ok_or_else(|| "replay: resposta não é objeto".to_owned())?;
            let status = objeto
                .get("status")
                .and_then(Value::as_u64)
                .and_then(|valor| u16::try_from(valor).ok())
                .ok_or_else(|| "replay: status inválido".to_owned())?;
            let corpo = objeto
                .get("corpo")
                .and_then(Value::as_str)
                .ok_or_else(|| "replay: corpo ausente".to_owned())?
                .to_owned();
            respostas.push_back(RespostaReplay {
                url_contem: objeto
                    .get("url_contem")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                status,
                corpo,
            });
        }
        Ok(Self {
            respostas: Mutex::new(respostas),
        })
    }
}

impl Executor for ReplayExecutor {
    fn executar(&self, requisicao: &Requisicao) -> Result<Resposta, ErroEnvio> {
        let mut respostas = self.respostas.lock().expect("replay");
        let indice = respostas.iter().position(|resposta| {
            resposta
                .url_contem
                .as_deref()
                .is_none_or(|trecho| requisicao.url.contains(trecho))
        });
        let Some(indice) = indice else {
            return Err(ErroEnvio::AntesDoEnvio(ErroTransporte::Configuracao(
                format!("replay sem resposta para {}", requisicao.url),
            )));
        };
        let resposta = respostas.remove(indice).expect("resposta encontrada");
        Ok(Resposta {
            status: resposta.status,
            cabecalhos: vec![("content-type".to_owned(), "application/json".to_owned())],
            corpo: resposta.corpo.into_bytes(),
        })
    }
}
