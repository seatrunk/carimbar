//! Testes dos clientes ADN e de parâmetros municipais com respostas simuladas.

use carimbar::nfse::{
    CacheParametros, ClienteAdn, ClienteParametros, StatusDistribuicao, comprimir_b64,
};
use carimbar::nfse::{ErroNfse, Executor};
use carimbar::transport::{ErroEnvio, ErroTransporte, Requisicao, Resposta};
use carimbar::types::TpAmb;
use std::sync::Mutex;
use std::time::Duration;

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

fn resposta(status: u16, corpo: impl Into<Vec<u8>>) -> Resposta {
    Resposta {
        status,
        cabecalhos: vec![("content-type".to_owned(), "application/json".to_owned())],
        corpo: corpo.into(),
    }
}

fn lote(status: &str, docs: Vec<serde_json::Value>, ambiente: u8, ultimo: Option<u64>) -> Vec<u8> {
    let mut objeto = serde_json::json!({
        "StatusProcessamento": status,
        "LoteDFe": docs,
        "Alertas": [],
        "Erros": [],
        "TipoAmbiente": ambiente,
        "VersaoAplicativo": "ADN_1.0",
        "DataHoraProcessamento": "2026-11-09T10:00:00-03:00"
    });
    if let Some(ultimo) = ultimo {
        objeto["ultimoNsu"] = serde_json::json!(ultimo);
    }
    objeto.to_string().into_bytes()
}

fn documento(nsu: u64, chave: &str, tipo: &str, xml: &str) -> serde_json::Value {
    serde_json::json!({
        "NSU": nsu,
        "ChaveAcesso": chave,
        "TipoDocumento": tipo,
        "TipoEvento": null,
        "ArquivoXml": comprimir_b64(xml).expect("comprime"),
        "DataHoraGeracao": "2026-11-09T09:00:00-03:00"
    })
}

#[test]
fn consulta_nsu_descompacta_xml_e_avanca_cursor() {
    let executor = ExecutorFalso::com(vec![Ok(resposta(
        200,
        lote(
            "DOCUMENTOS_LOCALIZADOS",
            vec![documento(7, "chave-7", "NFSE", "<NFSe/>")],
            2,
            None,
        ),
    ))]);
    let cliente = ClienteAdn::new(TpAmb::Homologacao, &executor);
    let resultado = cliente.consultar(6, None, None).expect("consulta");
    assert_eq!(resultado.status, StatusDistribuicao::DocumentosLocalizados);
    assert_eq!(resultado.documentos.len(), 1);
    assert_eq!(resultado.documentos[0].nsu, 7);
    assert_eq!(resultado.documentos[0].xml, "<NFSe/>");
    assert_eq!(resultado.ultimo_nsu, 7);
    assert_eq!(resultado.ambiente, TpAmb::Homologacao);
    assert!(executor.pedidos()[0].url.ends_with("/contribuintes/DFe/6"));
}

#[test]
fn adn_400_com_corpo_valido_e_sem_documentos() {
    let executor = ExecutorFalso::com(vec![Ok(resposta(
        400,
        lote("NENHUM_DOCUMENTO_LOCALIZADO", vec![], 2, None),
    ))]);
    let cliente = ClienteAdn::new(TpAmb::Homologacao, &executor);
    let resultado = cliente
        .consultar(10, Some("00000001000136"), Some(true))
        .expect("consulta");
    assert_eq!(resultado.status, StatusDistribuicao::NenhumDocumento);
    assert!(resultado.documentos.is_empty());
    assert_eq!(resultado.ultimo_nsu, 10);
    let url = &executor.pedidos()[0].url;
    assert!(url.contains("cnpjConsulta=00000001000136"));
    assert!(url.contains("lote=true"));
}

fn corpo_convenio() -> Vec<u8> {
    serde_json::json!({
        "parametrosConvenio": {
            "tipoConvenioDeserializationSetter": 1,
            "aderenteAmbienteNacional": 1,
            "aderenteEmissorNacional": 1,
            "situacaoEmissaoPadraoContribuintesRFB": 1,
            "aderenteMAN": 0,
            "permiteAproveitametoDeCreditos": true
        }
    })
    .to_string()
    .into_bytes()
}

fn corpo_aliquota() -> Vec<u8> {
    serde_json::json!({
        "aliquotas": {
            "010101": [
                {"Incidencia": "1", "Aliq": 2.5, "DtIni": "2026-01-01", "DtFim": null}
            ]
        }
    })
    .to_string()
    .into_bytes()
}

#[test]
fn parametros_com_cache_e_atualizacao_explicita() {
    let executor = ExecutorFalso::com(vec![
        Ok(resposta(200, corpo_convenio())),
        Ok(resposta(200, corpo_convenio())),
        Ok(resposta(200, corpo_aliquota())),
    ]);
    let cache = CacheParametros::novo(Duration::from_secs(60));
    let cliente = ClienteParametros::new(TpAmb::Homologacao, &executor, Some(&cache));
    let convenio = cliente
        .consultar_convenio("3550308", false)
        .expect("convênio");
    assert_eq!(convenio.tipo_convenio, "1");
    assert_eq!(convenio.permite_aproveitamento_creditos, Some(true));
    assert_eq!(executor.pedidos().len(), 1);

    let repetido = cliente.consultar_convenio("3550308", false).expect("cache");
    assert_eq!(repetido, convenio);
    assert_eq!(executor.pedidos().len(), 1);

    let atualizado = cliente
        .consultar_convenio("3550308", true)
        .expect("força atualização");
    assert_eq!(atualizado, convenio);
    assert_eq!(executor.pedidos().len(), 2);

    let aliquota = cliente
        .consultar_aliquota("3550308", "010101", "2026-11-01", false)
        .expect("alíquota");
    assert_eq!(
        aliquota.aliquotas["010101"][0].aliquota.as_deref(),
        Some("2.5")
    );
    assert_eq!(aliquota.aliquotas["010101"][0].data_inicio, "2026-01-01");
    assert_eq!(executor.pedidos().len(), 3);
}

#[test]
fn parametro_ausente_retorna_erro() {
    let executor = ExecutorFalso::com(vec![Ok(resposta(
        404,
        serde_json::json!({"mensagem": "município sem parâmetro"})
            .to_string()
            .into_bytes(),
    ))]);
    let cliente = ClienteParametros::new(TpAmb::Homologacao, &executor, None);
    let erro = cliente
        .consultar_convenio("9999999", false)
        .expect_err("sem convênio");
    assert!(matches!(erro, ErroNfse::ParametroAusente(_)));
}

#[test]
fn erro_de_transporte_nao_mistura_ambiente() {
    let executor = ExecutorFalso::com(vec![Err(ErroEnvio::ResultadoDesconhecido(
        ErroTransporte::Timeout("leitura"),
    ))]);
    let cliente = ClienteAdn::new(TpAmb::Producao, &executor);
    let erro = cliente.consultar(0, None, None).expect_err("timeout");
    assert!(matches!(erro, ErroNfse::ResultadoDesconhecido(_)));
    assert!(
        executor.pedidos()[0]
            .url
            .starts_with("https://adn.nfse.gov.br/")
    );
}

#[test]
fn cache_separa_ambiente_e_municipio() {
    let corpo = |man: u8| {
        serde_json::json!({
            "parametrosConvenio": {
                "tipoConvenioDeserializationSetter": 1,
                "aderenteAmbienteNacional": 1,
                "aderenteEmissorNacional": 1,
                "situacaoEmissaoPadraoContribuintesRFB": 1,
                "aderenteMAN": man
            }
        })
        .to_string()
        .into_bytes()
    };
    let executor = ExecutorFalso::com(vec![
        Ok(resposta(200, corpo(0))),
        Ok(resposta(200, corpo(1))),
        Ok(resposta(200, corpo(0))),
    ]);
    let cache = CacheParametros::novo(Duration::from_secs(60));
    let homologacao = ClienteParametros::new(TpAmb::Homologacao, &executor, Some(&cache));
    let producao = ClienteParametros::new(TpAmb::Producao, &executor, Some(&cache));
    let a = homologacao
        .consultar_convenio("3550308", false)
        .expect("homologação");
    assert_eq!(a.aderente_man, "0");
    let b = producao
        .consultar_convenio("3550308", false)
        .expect("produção");
    assert_eq!(b.aderente_man, "1");
    let c = homologacao
        .consultar_convenio("3304557", false)
        .expect("outro município");
    assert_eq!(c.aderente_man, "0");
    assert_eq!(executor.pedidos().len(), 3);
    assert!(executor.pedidos()[0].url.contains("producaorestrita"));
    assert!(
        executor.pedidos()[1]
            .url
            .starts_with("https://adn.nfse.gov.br/")
    );
}
