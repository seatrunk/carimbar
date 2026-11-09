//! Testes de sincronização ADN pela fachada, com respostas simuladas.

use carimbar::Emissor;
use carimbar::Journal;
use carimbar_nfse::{Executor, comprimir_b64};
use carimbar_transport::{ErroEnvio, ErroTransporte, Requisicao, Resposta};
use carimbar_types::{Cnpj, TpAmb};
use carimbar_vault::{ChaveMestra, Cofre};
use std::sync::Mutex;

const CNPJ: &str = "00000001000136";

#[derive(Default)]
struct ExecutorFalso {
    respostas: Mutex<Vec<Result<Resposta, ErroEnvio>>>,
    pedidos: Mutex<Vec<Requisicao>>,
}

impl ExecutorFalso {
    fn com(respostas: Vec<Result<Resposta, ErroEnvio>>) -> Self {
        Self {
            respostas: Mutex::new(respostas),
            pedidos: Mutex::new(Vec::new()),
        }
    }
    fn pedidos(&self) -> Vec<Requisicao> {
        self.pedidos.lock().expect("pedidos").clone()
    }
}

impl Executor for ExecutorFalso {
    fn executar(&self, requisicao: &Requisicao) -> Result<Resposta, ErroEnvio> {
        self.pedidos
            .lock()
            .expect("pedidos")
            .push(requisicao.clone());
        self.respostas.lock().expect("respostas").remove(0)
    }
}

fn resposta(corpo: impl Into<Vec<u8>>) -> Resposta {
    Resposta {
        status: 200,
        cabecalhos: vec![("content-type".to_owned(), "application/json".to_owned())],
        corpo: corpo.into(),
    }
}

fn documento(nsu: u64, xml: &str) -> serde_json::Value {
    serde_json::json!({
        "NSU": nsu,
        "ChaveAcesso": format!("chave-{nsu}"),
        "TipoDocumento": "NFSE",
        "TipoEvento": null,
        "ArquivoXml": comprimir_b64(xml).expect("comprime"),
        "DataHoraGeracao": "2026-11-09T09:00:00-03:00"
    })
}

fn lote(docs: Vec<serde_json::Value>) -> Vec<u8> {
    serde_json::json!({
        "StatusProcessamento": if docs.is_empty() { "NENHUM_DOCUMENTO_LOCALIZADO" } else { "DOCUMENTOS_LOCALIZADOS" },
        "LoteDFe": docs,
        "Alertas": [],
        "Erros": [],
        "TipoAmbiente": 2,
        "VersaoAplicativo": "ADN_1.0",
        "DataHoraProcessamento": "2026-11-09T10:00:00-03:00"
    })
    .to_string()
    .into_bytes()
}

fn emissor<'a>(executor: &'a ExecutorFalso, journal: Journal) -> Emissor<'a> {
    let cofre = Cofre::criar_em_memoria(Box::new(ChaveMestra::de_bytes([3; 32]))).expect("cofre");
    let empresa = Cnpj::parse(CNPJ).expect("CNPJ");
    Emissor::novo(cofre, journal, empresa, executor).expect("emissor")
}

#[test]
fn sincroniza_paginas_persiste_e_retoma_cursor() {
    let executor = ExecutorFalso::com(vec![
        Ok(resposta(lote(vec![
            documento(1, "<a/>"),
            documento(2, "<b/>"),
        ]))),
        Ok(resposta(lote(vec![documento(3, "<c/>")]))),
        Ok(resposta(lote(vec![]))),
        Ok(resposta(lote(vec![]))),
    ]);
    let journal = Journal::em_memoria().expect("journal");
    let mut emissor = emissor(&executor, journal);
    let resultado = emissor.sincronizar_adn(10).expect("sincroniza");
    assert_eq!(resultado.inicio_nsu, 0);
    assert_eq!(resultado.ultimo_nsu, 3);
    assert_eq!(resultado.documentos_novos, 3);
    assert_eq!(resultado.paginas, 2);
    let documentos = emissor.documentos_adn().expect("documentos");
    assert_eq!(documentos.len(), 3);
    assert_eq!(documentos[0].xml, "<a/>");
    assert_eq!(executor.pedidos().len(), 3);
    assert!(executor.pedidos()[0].url.ends_with("/DFe/0"));
    assert!(executor.pedidos()[1].url.ends_with("/DFe/2"));

    let novamente = emissor.sincronizar_adn(10).expect("retoma");
    assert_eq!(novamente.inicio_nsu, 3);
    assert_eq!(novamente.documentos_novos, 0);
    assert!(executor.pedidos()[3].url.ends_with("/DFe/3"));
}

#[test]
fn limite_de_paginas_interrompe_sem_perder_cursor() {
    let executor = ExecutorFalso::com(vec![Ok(resposta(lote(vec![
        documento(1, "<a/>"),
        documento(2, "<b/>"),
    ])))]);
    let journal = Journal::em_memoria().expect("journal");
    let mut emissor = emissor(&executor, journal);
    let resultado = emissor.sincronizar_adn(1).expect("uma página");
    assert_eq!(resultado.paginas, 1);
    assert_eq!(resultado.ultimo_nsu, 2);
    assert_eq!(executor.pedidos().len(), 1);
    assert_eq!(emissor.documentos_adn().expect("docs").len(), 2);
}

#[test]
fn falha_de_transporte_nao_avanca_cursor() {
    let executor = ExecutorFalso::com(vec![
        Err(ErroEnvio::ResultadoDesconhecido(ErroTransporte::Timeout(
            "leitura",
        ))),
        Ok(resposta(lote(vec![]))),
    ]);
    let journal = Journal::em_memoria().expect("journal");
    let mut emissor = emissor(&executor, journal);
    let erro = emissor.sincronizar_adn(10).expect_err("timeout");
    assert!(matches!(erro, carimbar::ErroEmissor::Nfse(_)));
    let resultado = emissor.sincronizar_adn(10).expect("retoma do zero");
    assert_eq!(resultado.inicio_nsu, 0);
    assert_eq!(resultado.documentos_novos, 0);
    assert!(executor.pedidos()[1].url.ends_with("/DFe/0"));
}

#[test]
fn nsu_repetido_e_deduplicado() {
    let executor = ExecutorFalso::com(vec![
        Ok(resposta(lote(vec![
            documento(1, "<a/>"),
            documento(2, "<b/>"),
        ]))),
        Ok(resposta(lote(vec![
            documento(2, "<b/>"),
            documento(3, "<c/>"),
        ]))),
        Ok(resposta(lote(vec![]))),
    ]);
    let journal = Journal::em_memoria().expect("journal");
    let mut emissor = emissor(&executor, journal);
    let resultado = emissor.sincronizar_adn(10).expect("sincroniza");
    assert_eq!(resultado.documentos_novos, 3);
    let documentos = emissor.documentos_adn().expect("docs");
    assert_eq!(documentos.len(), 3);
    assert_eq!(documentos[2].nsu, 3);
}

#[test]
fn ambiente_de_producao_usa_host_de_producao() {
    let executor = ExecutorFalso::com(vec![Ok(resposta(lote(vec![])))]);
    let journal = Journal::em_memoria().expect("journal");
    let mut emissor = emissor(&executor, journal);
    // Sem promoção explícita a fachada está em homologação.
    let _ = emissor.sincronizar_adn(1).expect("sync");
    assert!(
        executor.pedidos()[0]
            .url
            .starts_with("https://adn.producaorestrita.nfse.gov.br/")
    );
    assert_eq!(emissor.ambiente().tp_amb(), TpAmb::Homologacao);
}
