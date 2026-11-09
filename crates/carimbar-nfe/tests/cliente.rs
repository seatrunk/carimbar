//! Testes do cliente SOAP NF-e com respostas simuladas.

use carimbar_endpoints::Uf;
use carimbar_nfe::{
    ClienteNfe, Contingencia, EventoCancelamento, Executor, Inutilizacao, ResultadoAutorizacao,
    ResultadoEvento,
};
use carimbar_transport::{ErroEnvio, Requisicao, Resposta};
use carimbar_types::{Cnpj, TpAmb};
use std::collections::VecDeque;
use std::sync::Mutex;

const CHAVE: &str = "35261100000001000136550010000000001123456780";
const NFE: &str = r#"<NFe xmlns="http://www.portalfiscal.inf.br/nfe"><infNFe versao="4.00" Id="NFe35261100000001000136550010000000001123456780"><ide><cUF>35</cUF></ide></infNFe></NFe>"#;

#[derive(Default)]
struct ExecutorFalso {
    respostas: Mutex<VecDeque<Resposta>>,
    pedidos: Mutex<Vec<Requisicao>>,
}

impl ExecutorFalso {
    fn com(respostas: Vec<Resposta>) -> Self {
        Self {
            respostas: Mutex::new(respostas.into()),
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
        Ok(self
            .respostas
            .lock()
            .expect("respostas")
            .pop_front()
            .expect("resposta simulada"))
    }
}

fn resposta(corpo: impl Into<Vec<u8>>) -> Resposta {
    Resposta {
        status: 200,
        cabecalhos: vec![("content-type".to_owned(), "application/soap+xml".to_owned())],
        corpo: corpo.into(),
    }
}

fn envelope(conteudo: &str) -> Vec<u8> {
    format!(
        r#"<soap12:Envelope xmlns:soap12="http://www.w3.org/2003/05/soap-envelope"><soap12:Body>{conteudo}</soap12:Body></soap12:Envelope>"#
    )
    .into_bytes()
}

fn ret_autorizacao(cstat: &str, com_protocolo: bool) -> Vec<u8> {
    let protocolo = if com_protocolo {
        format!(
            r#"<protNFe versao="4.00"><infProt><tpAmb>2</tpAmb><verAplic>teste</verAplic><chNFe>{CHAVE}</chNFe><dhRecbto>2026-11-09T10:00:00-03:00</dhRecbto><nProt>135260000000001</nProt><digVal>AAAA</digVal><cStat>100</cStat><xMotivo>Autorizado o uso da NF-e</xMotivo></infProt></protNFe>"#
        )
    } else {
        String::new()
    };
    envelope(&format!(
        r#"<retEnviNFe xmlns="http://www.portalfiscal.inf.br/nfe" versao="4.00"><tpAmb>2</tpAmb><verAplic>teste</verAplic><cStat>{cstat}</cStat><xMotivo>Lote processado</xMotivo><cUF>35</cUF><dhRecbto>2026-11-09T10:00:00-03:00</dhRecbto>{protocolo}</retEnviNFe>"#
    ))
}

#[test]
fn autorizacao_sincrona_extrai_protocolo_e_xml() {
    let executor = ExecutorFalso::com(vec![resposta(ret_autorizacao("104", true))]);
    let cliente = ClienteNfe::novo(
        TpAmb::Homologacao,
        Uf::SaoPaulo,
        Contingencia::Normal,
        &executor,
    );
    let resultado = cliente.autorizar_xml(NFE).expect("autorização");
    let ResultadoAutorizacao::Autorizada(protocolo) = resultado else {
        panic!("esperava autorização: {resultado:?}");
    };
    assert_eq!(protocolo.chave, CHAVE);
    assert_eq!(protocolo.numero, "135260000000001");
    assert!(protocolo.xml.contains("<nfeProc"));
    assert!(protocolo.xml.contains("<protNFe"));
    let pedido = &executor.pedidos()[0];
    assert!(pedido.url.to_ascii_lowercase().contains("nfeautorizacao4"));
    assert!(
        pedido
            .tipo_conteudo
            .as_deref()
            .expect("content-type")
            .contains("nfeAutorizacaoLote")
    );
    let texto = String::from_utf8_lossy(&pedido.corpo);
    assert!(texto.contains("enviNFe"));
    assert!(texto.contains("indSinc>1"));
}

#[test]
fn autorizacao_assincrona_devolve_recibo() {
    let executor = ExecutorFalso::com(vec![resposta(ret_autorizacao("103", false))]);
    let cliente = ClienteNfe::novo(
        TpAmb::Homologacao,
        Uf::SaoPaulo,
        Contingencia::Normal,
        &executor,
    );
    let resultado = cliente.autorizar_xml(NFE).expect("autorização");
    assert!(matches!(resultado, ResultadoAutorizacao::Recibo(_)));
}

#[test]
fn rejeicao_fiscal_e_interpretada() {
    let executor = ExecutorFalso::com(vec![resposta(ret_autorizacao("204", false))]);
    let cliente = ClienteNfe::novo(
        TpAmb::Homologacao,
        Uf::SaoPaulo,
        Contingencia::Normal,
        &executor,
    );
    let resultado = cliente.autorizar_xml(NFE).expect("resposta");
    let ResultadoAutorizacao::Rejeitada { codigo, .. } = resultado else {
        panic!("esperava rejeição");
    };
    assert_eq!(codigo, "204");
}

#[test]
fn consulta_por_chave_retorna_protocolo() {
    let corpo = envelope(&format!(
        r#"<retConsSitNFe xmlns="http://www.portalfiscal.inf.br/nfe" versao="4.00"><tpAmb>2</tpAmb><verAplic>teste</verAplic><cStat>100</cStat><xMotivo>Autorizado</xMotivo><cUF>35</cUF><dhRecbto>2026-11-09T10:00:00-03:00</dhRecbto><protNFe versao="4.00"><infProt><chNFe>{CHAVE}</chNFe><nProt>135</nProt><cStat>100</cStat><xMotivo>Autorizado</xMotivo></infProt></protNFe></retConsSitNFe>"#
    ));
    let executor = ExecutorFalso::com(vec![resposta(corpo)]);
    let cliente = ClienteNfe::novo(
        TpAmb::Homologacao,
        Uf::SaoPaulo,
        Contingencia::Normal,
        &executor,
    );
    let resultado = cliente.consultar(CHAVE).expect("consulta");
    assert_eq!(resultado.codigo, "100");
    assert_eq!(resultado.protocolo.expect("protocolo").numero, "135");
    assert!(
        executor.pedidos()[0]
            .url
            .to_ascii_lowercase()
            .contains("nfeconsultaprotocolo4")
    );
}

#[test]
fn inutilizacao_e_interpretada() {
    let corpo = envelope(
        r#"<retInutNFe xmlns="http://www.portalfiscal.inf.br/nfe" versao="4.00"><infInut><tpAmb>2</tpAmb><verAplic>teste</verAplic><cStat>102</cStat><xMotivo>Inutilizacao homologada</xMotivo><nProt>135</nProt></infInut></retInutNFe>"#,
    );
    let executor = ExecutorFalso::com(vec![resposta(corpo)]);
    let cliente = ClienteNfe::novo(
        TpAmb::Homologacao,
        Uf::SaoPaulo,
        Contingencia::Normal,
        &executor,
    );
    let resultado = cliente
        .enviar_inutilizacao("<inutNFe/>")
        .expect("inutilização");
    assert_eq!(resultado.codigo, "102");
    assert_eq!(resultado.protocolo.as_deref(), Some("135"));
    assert!(
        executor.pedidos()[0]
            .url
            .to_ascii_lowercase()
            .contains("nfeinutilizacao4")
    );
}

#[test]
fn evento_aceito_e_interpretado() {
    let corpo = envelope(
        r#"<retEnvEvento xmlns="http://www.portalfiscal.inf.br/nfe" versao="1.00"><idLote>1</idLote><tpAmb>2</tpAmb><verAplic>teste</verAplic><cOrgao>35</cOrgao><cStat>128</cStat><xMotivo>Lote de evento processado</xMotivo><retEvento versao="1.00"><infEvento><tpAmb>2</tpAmb><verAplic>teste</verAplic><cOrgao>35</cOrgao><cStat>135</cStat><xMotivo>Evento registrado</xMotivo><chNFe>35261100000001000136550010000000001123456780</chNFe><tpEvento>110111</tpEvento><nProt>135</nProt></infEvento></retEvento></retEnvEvento>"#,
    );
    let executor = ExecutorFalso::com(vec![resposta(corpo)]);
    let cliente = ClienteNfe::novo(
        TpAmb::Homologacao,
        Uf::SaoPaulo,
        Contingencia::Normal,
        &executor,
    );
    let resultado = cliente.enviar_evento("<envEvento/>").expect("evento");
    assert!(matches!(resultado, ResultadoEvento::Registrado(_)));
    assert!(
        executor.pedidos()[0]
            .url
            .to_ascii_lowercase()
            .contains("recepcaoevento4")
    );
}

#[test]
fn contingencia_svc_usa_endpoint_configurado() {
    let executor = ExecutorFalso::com(vec![resposta(ret_autorizacao("103", false))]);
    let cliente = ClienteNfe::novo(
        TpAmb::Homologacao,
        Uf::SaoPaulo,
        Contingencia::SvcAn,
        &executor,
    );
    let _ = cliente.autorizar_xml(NFE).expect("autorização");
    let url = &executor.pedidos()[0].url;
    assert!(!url.is_empty());
}

#[test]
fn eventos_e_inutilizacao_modelam_campos() {
    let _ = EventoCancelamento {
        ambiente: TpAmb::Homologacao,
        uf: Uf::SaoPaulo,
        cnpj: Cnpj::parse("00000001000136").expect("CNPJ"),
        chave: CHAVE.to_owned(),
        protocolo: "135".to_owned(),
        dh_evento: "2026-11-09T10:00:00-03:00".to_owned(),
        justificativa: "Cancelamento sintetico de teste".to_owned(),
        n_seq_evento: 1,
    };
    let _ = Inutilizacao {
        ambiente: TpAmb::Homologacao,
        uf: Uf::SaoPaulo,
        cnpj: Cnpj::parse("00000001000136").expect("CNPJ"),
        ano: 26,
        serie: 1,
        numero_inicial: 1,
        numero_final: 2,
        justificativa: "Inutilizacao sintetica de teste".to_owned(),
    };
}
