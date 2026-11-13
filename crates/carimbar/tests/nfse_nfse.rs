//! Testes de integração da DPS, eventos e cliente Sefin com respostas
//! simuladas. Nada acessa SEFAZ, Sefin ou ADN.

use carimbar::dsig::{ErroDsig, Signer};
use carimbar::nfse::{
    ChaveNfse, ClienteSefin, CodigoMunicipio, DataFiscal, DataHoraFiscal, Dps, ErroFiscal,
    ErroNfse, EventoCancelamento, Executor, InscricaoFederal, LocalPrestacao, MotivoCancelamento,
    NumeroDps, Prestador, RegimeTributario, ResultadoEmissao, ResultadoEvento, SerieDps, Servico,
    TipoEmitente, TipoRetencaoIssqn, TotalTributos, TribIssqn, TributoMunicipal, Valores,
    comprimir_b64, descomprimir_b64,
};
use carimbar::transport::{ErroEnvio, ErroTransporte, Requisicao, Resposta};
use carimbar::types::{Dinheiro, TpAmb};
use std::cell::RefCell;
use std::collections::VecDeque;

const CNPJ: &str = "00000001000136";
const MUNICIPIO: &str = "3550308";

struct AssinanteFalso;

impl Signer for AssinanteFalso {
    fn certificado_der(&self) -> &[u8] {
        &[0x30, 0x82, 0x01, 0x00]
    }

    fn assinar_rsa_sha256(&self, _dados: &[u8]) -> Result<Vec<u8>, ErroDsig> {
        Ok(vec![0x30, 0x31, 0x32, 0x33])
    }
}

fn municipio() -> CodigoMunicipio {
    CodigoMunicipio::new(MUNICIPIO).expect("município")
}

fn dps(ambiente: TpAmb) -> Dps {
    Dps {
        ambiente,
        dh_emi: DataHoraFiscal::parse("2026-11-08T10:00:00-03:00").expect("dhEmi"),
        ver_aplic: "carimbar-0.0.2".to_owned(),
        serie: SerieDps::new(1).expect("série"),
        numero: NumeroDps::new(1).expect("número"),
        d_compet: DataFiscal::parse("2026-11-08").expect("dCompet"),
        tp_emit: TipoEmitente::Prestador,
        c_motivo_emis_ti: None,
        ch_nfse_rej: None,
        c_loc_emi: municipio(),
        substituicao: None,
        prestador: Prestador {
            inscricao: InscricaoFederal::parse(CNPJ).expect("CNPJ"),
            inscricao_municipal: None,
            nome: Some("Empresa Sintetica".to_owned()),
            email: None,
            regime: RegimeTributario::mei(),
        },
        tomador: None,
        intermediario: None,
        servico: Servico {
            local_prestacao: LocalPrestacao::Municipio(municipio()),
            codigo_tributacao_nacional: "010101".to_owned(),
            codigo_tributacao_municipal: None,
            codigo_nbs: None,
            codigo_interno: None,
            descricao: "Servico sintetico de teste".to_owned(),
            informacoes_complementares: None,
        },
        valores: Valores {
            valor_recebido: None,
            valor_servico: "10.00".parse::<Dinheiro>().expect("valor"),
            desconto_incondicional: None,
            desconto_condicional: None,
            tributo_municipal: TributoMunicipal {
                trib_issqn: TribIssqn::Tributavel,
                tipo_retencao: TipoRetencaoIssqn::NaoRetido,
                aliquota: None,
            },
            total_tributos: TotalTributos::IndicadorZero,
        },
        ibs_cbs: None,
    }
}

fn chave(ambiente: TpAmb) -> ChaveNfse {
    let valor = format!("{}{}{}", "355030", CNPJ, "0".repeat(30));
    ChaveNfse::parse(&valor, ambiente).expect("chave sintética")
}

fn evento(ambiente: TpAmb) -> EventoCancelamento {
    EventoCancelamento {
        ambiente,
        chave: chave(ambiente),
        autor: InscricaoFederal::parse(CNPJ).expect("CNPJ"),
        ver_aplic: "carimbar-0.0.2".to_owned(),
        dh_evento: DataHoraFiscal::parse("2026-11-08T11:00:00-03:00").expect("dhEvento"),
        motivo: MotivoCancelamento::ErroEmissao,
        descricao: "Erro na emissao da nota sintetica".to_owned(),
    }
}

#[derive(Default)]
struct ExecutorFalso {
    respostas: RefCell<VecDeque<Result<Resposta, ErroEnvio>>>,
    pedidos: RefCell<Vec<Requisicao>>,
}

impl ExecutorFalso {
    fn com(respostas: Vec<Result<Resposta, ErroEnvio>>) -> Self {
        Self {
            respostas: RefCell::new(respostas.into()),
            pedidos: RefCell::new(Vec::new()),
        }
    }

    fn pedido(&self, indice: usize) -> Requisicao {
        self.pedidos.borrow()[indice].clone()
    }
}

impl Executor for ExecutorFalso {
    fn executar(&self, requisicao: &Requisicao) -> Result<Resposta, ErroEnvio> {
        self.pedidos.borrow_mut().push(requisicao.clone());
        self.respostas
            .borrow_mut()
            .pop_front()
            .expect("resposta simulada disponível")
    }
}

fn resposta(status: u16, corpo: impl Into<Vec<u8>>) -> Resposta {
    Resposta {
        status,
        cabecalhos: vec![("content-type".to_owned(), "application/json".to_owned())],
        corpo: corpo.into(),
    }
}

fn corpo_autorizacao() -> Vec<u8> {
    let xml = "<NFSe xmlns=\"http://www.sped.fazenda.gov.br/nfse\"/>";
    serde_json::json!({
        "tipoAmbiente": 2,
        "versaoAplicativo": "SefinNac_1.0.0",
        "dataHoraProcessamento": "2026-11-08T10:00:01-03:00",
        "idDps": "DPS355030810000000100013600001000000000000001",
        "chaveAcesso": chave(TpAmb::Homologacao).as_str(),
        "nfseXmlGZipB64": comprimir_b64(xml).expect("comprime"),
        "alertas": []
    })
    .to_string()
    .into_bytes()
}

fn corpo_rejeicao() -> Vec<u8> {
    serde_json::json!({
        "tipoAmbiente": 2,
        "versaoAplicativo": "SefinNac_1.0.0",
        "dataHoraProcessamento": "2026-11-08T10:00:01-03:00",
        "idDps": "DPS355030810000000100013600001000000000000001",
        "erros": [{"codigo": "E0001", "descricao": "DPS invalida", "complemento": "campo X"}]
    })
    .to_string()
    .into_bytes()
}

#[test]
fn dps_minima_serializa_na_ordem_do_xsd_de_homologacao() {
    let xml = dps(TpAmb::Homologacao).para_xml().expect("XML");
    assert!(xml.starts_with("<?xml"));
    assert!(xml.contains("xmlns=\"http://www.sped.fazenda.gov.br/nfse\""));
    assert!(xml.contains("<infDPS Id=\"DPS355030810000000100013600001000000000000001\">"));
    assert!(xml.contains("<finNFSe>0</finNFSe>"));
    let pos = |trecho: &str| xml.find(trecho).expect(trecho);
    assert!(pos("<tpAmb>") < pos("<dhEmi>"));
    assert!(pos("<dhEmi>") < pos("<verAplic>"));
    assert!(pos("<prest>") < pos("<serv>"));
    assert!(pos("<serv>") < pos("<valores>"));
}

#[test]
fn dps_de_producao_omite_fin_nfse() {
    let xml = dps(TpAmb::Producao).para_xml().expect("XML");
    assert!(!xml.contains("<finNFSe>"));
    assert!(xml.contains("<tpAmb>1</tpAmb>"));
}

#[test]
fn dps_assinada_contem_signature() {
    let assinada = dps(TpAmb::Homologacao)
        .para_xml_assinado(&AssinanteFalso)
        .expect("assinada");
    assert!(assinada.contains("<ds:Signature"));
    assert!(assinada.contains("<ds:SignatureValue>"));
}

#[test]
fn emitir_assina_envia_e_interpreta_autorizacao() {
    let executor = ExecutorFalso::com(vec![Ok(resposta(201, corpo_autorizacao()))]);
    let cliente = ClienteSefin::new(TpAmb::Homologacao, &executor);
    let resultado = cliente
        .emitir(&dps(TpAmb::Homologacao), &AssinanteFalso)
        .expect("emissao");

    let ResultadoEmissao::Autorizada(autorizada) = resultado else {
        panic!("esperava autorização: {resultado:?}");
    };
    assert_eq!(
        autorizada.id_dps,
        "DPS355030810000000100013600001000000000000001"
    );
    assert_eq!(
        autorizada.chave.as_str(),
        chave(TpAmb::Homologacao).as_str()
    );
    assert_eq!(
        autorizada.xml_nfse,
        "<NFSe xmlns=\"http://www.sped.fazenda.gov.br/nfse\"/>"
    );

    let pedido = executor.pedido(0);
    assert_eq!(pedido.metodo, carimbar::transport::Metodo::Post);
    assert!(pedido.url.ends_with("/nfse"));
    let corpo: serde_json::Value = serde_json::from_slice(&pedido.corpo).expect("JSON do pedido");
    let b64 = corpo["dpsXmlGZipB64"].as_str().expect("dpsXmlGZipB64");
    let xml = descomprimir_b64(b64).expect("descomprime DPS");
    assert!(xml.contains("<ds:Signature"));
}

#[test]
fn emitir_interpreta_rejeicao_fiscal() {
    let executor = ExecutorFalso::com(vec![Ok(resposta(400, corpo_rejeicao()))]);
    let cliente = ClienteSefin::new(TpAmb::Homologacao, &executor);
    let resultado = cliente
        .emitir(&dps(TpAmb::Homologacao), &AssinanteFalso)
        .expect("resposta");
    let ResultadoEmissao::Rejeitada(rejeicao) = resultado else {
        panic!("esperava rejeição: {resultado:?}");
    };
    assert_eq!(
        rejeicao.erros,
        vec![ErroFiscal {
            codigo: "E0001".to_owned(),
            descricao: "DPS invalida".to_owned(),
            complemento: Some("campo X".to_owned()),
        }]
    );
}

#[test]
fn emitir_interpreta_duplicidade() {
    let executor = ExecutorFalso::com(vec![Ok(resposta(409, corpo_rejeicao()))]);
    let cliente = ClienteSefin::new(TpAmb::Homologacao, &executor);
    let resultado = cliente
        .emitir(&dps(TpAmb::Homologacao), &AssinanteFalso)
        .expect("resposta");
    assert!(matches!(resultado, ResultadoEmissao::Duplicidade(_)));
}

#[test]
fn emitir_preserva_resultado_desconhecido_e_antenvio() {
    let executor = ExecutorFalso::com(vec![Err(ErroEnvio::ResultadoDesconhecido(
        ErroTransporte::Timeout("leitura"),
    ))]);
    let cliente = ClienteSefin::new(TpAmb::Homologacao, &executor);
    let resultado = cliente
        .emitir(&dps(TpAmb::Homologacao), &AssinanteFalso)
        .expect("resultado desconhecido nao e erro local");
    assert!(matches!(resultado, ResultadoEmissao::Desconhecido { .. }));

    let executor = ExecutorFalso::com(vec![Err(ErroEnvio::AntesDoEnvio(ErroTransporte::Url(
        "https://exemplo.invalid".to_owned(),
    )))]);
    let cliente = ClienteSefin::new(TpAmb::Homologacao, &executor);
    let erro = cliente
        .emitir(&dps(TpAmb::Homologacao), &AssinanteFalso)
        .expect_err("falha antes do envio");
    assert!(erro.seguro_para_reenviar());
}

#[test]
fn consulta_de_nfse_e_dps_usa_get_e_placeholders() {
    let corpo_nfse = serde_json::json!({
        "chaveAcesso": chave(TpAmb::Homologacao).as_str(),
        "nfseXmlGZipB64": comprimir_b64("<NFSe/>").expect("comprime")
    })
    .to_string()
    .into_bytes();
    let corpo_dps = serde_json::json!({
        "idDps": "DPS355030810000000100013600001000000000000001",
        "chaveAcesso": chave(TpAmb::Homologacao).as_str()
    })
    .to_string()
    .into_bytes();
    let executor = ExecutorFalso::com(vec![
        Ok(resposta(200, corpo_nfse)),
        Ok(resposta(200, corpo_dps)),
    ]);
    let cliente = ClienteSefin::new(TpAmb::Homologacao, &executor);

    let nfse = cliente
        .consultar(&chave(TpAmb::Homologacao))
        .expect("consulta");
    assert_eq!(nfse.xml_nfse, "<NFSe/>");
    let pedido = executor.pedido(0);
    assert_eq!(pedido.metodo, carimbar::transport::Metodo::Get);
    assert!(
        pedido
            .url
            .ends_with(&format!("/nfse/{}", chave(TpAmb::Homologacao).as_str()))
    );

    let id = dps(TpAmb::Homologacao).id().expect("Id");
    let localizada = cliente.consultar_dps(&id).expect("consulta DPS");
    assert_eq!(localizada.id_dps, id.as_str());
    assert_eq!(
        localizada.chave.as_str(),
        chave(TpAmb::Homologacao).as_str()
    );
    assert!(
        executor
            .pedido(1)
            .url
            .ends_with(&format!("/dps/{}", id.as_str()))
    );
}

#[test]
fn consulta_inexistente_retorna_nao_encontrada() {
    let executor = ExecutorFalso::com(vec![Ok(resposta(404, b"{}".to_vec()))]);
    let cliente = ClienteSefin::new(TpAmb::Homologacao, &executor);
    assert_eq!(
        cliente.consultar(&chave(TpAmb::Homologacao)),
        Err(ErroNfse::NaoEncontrada)
    );
}

#[test]
fn registrar_cancelamento_envia_pedido_assinado() {
    let corpo = serde_json::json!({
        "eventoXmlGZipB64": comprimir_b64("<evento/>").expect("comprime")
    })
    .to_string()
    .into_bytes();
    let executor = ExecutorFalso::com(vec![Ok(resposta(201, corpo))]);
    let cliente = ClienteSefin::new(TpAmb::Homologacao, &executor);
    let resultado = cliente
        .registrar_cancelamento(&evento(TpAmb::Homologacao), &AssinanteFalso)
        .expect("evento");
    let ResultadoEvento::Registrado(registrado) = resultado else {
        panic!("esperava registro: {resultado:?}");
    };
    assert_eq!(registrado.xml_evento, "<evento/>");

    let pedido = executor.pedido(0);
    assert!(pedido.url.ends_with(&format!(
        "/nfse/{}/eventos",
        chave(TpAmb::Homologacao).as_str()
    )));
    let corpo: serde_json::Value = serde_json::from_slice(&pedido.corpo).expect("JSON do pedido");
    let b64 = corpo["pedidoRegistroEventoXmlGZipB64"]
        .as_str()
        .expect("pedidoRegistroEventoXmlGZipB64");
    let xml = descomprimir_b64(b64).expect("descomprime evento");
    assert!(xml.contains("<ds:Signature"));
    assert!(xml.contains("Cancelamento de NFS-e"));
}

#[test]
fn evento_rejeitado_e_interpretado() {
    let executor = ExecutorFalso::com(vec![Ok(resposta(400, corpo_rejeicao()))]);
    let cliente = ClienteSefin::new(TpAmb::Homologacao, &executor);
    let resultado = cliente
        .registrar_cancelamento(&evento(TpAmb::Homologacao), &AssinanteFalso)
        .expect("resposta");
    assert!(matches!(resultado, ResultadoEvento::Rejeitado(_)));
}

#[test]
fn cliente_recusa_dps_de_outro_ambiente() {
    let executor = ExecutorFalso::default();
    let cliente = ClienteSefin::new(TpAmb::Homologacao, &executor);
    let erro = cliente
        .emitir(&dps(TpAmb::Producao), &AssinanteFalso)
        .expect_err("ambiente diverge");
    assert!(matches!(
        erro,
        ErroNfse::Campo {
            campo: "dps.tpAmb",
            ..
        }
    ));
}
