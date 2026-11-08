//! Validação XSD contra schema sintético e schemas oficiais vendorizados.

#![cfg(feature = "xsd")]

use carimbar_xml::{Documento, ErroXml};
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn esquema_oficial(pacote: &str, arquivo: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas")
        .join(pacote)
        .join(arquivo)
}

#[test]
fn schema_sintetico_aceita_documento_valido() {
    let xml = r#"<nota xmlns="urn:carimbar:teste" versao="1.00"><numero>1</numero><valor>10.50</valor></nota>"#;
    carimbar_xml::xsd::validar(xml.as_bytes(), fixtures().join("minimo.xsd"))
        .expect("documento válido");
}

#[test]
fn schema_sintetico_rejeita_documento_invalido_com_diagnostico() {
    let xml = r#"<nota xmlns="urn:carimbar:teste" versao="1.00"><valor>dez</valor></nota>"#;
    let erro = carimbar_xml::xsd::validar(xml.as_bytes(), fixtures().join("minimo.xsd"))
        .expect_err("documento inválido");
    match erro {
        ErroXml::Xsd(mensagem) => {
            assert!(mensagem.contains("cvc-"), "sem código cvc: {mensagem}");
        }
        outro => panic!("erro inesperado: {outro}"),
    }
}

#[test]
fn validador_compilado_pode_ser_reutilizado() {
    let validador =
        carimbar_xml::xsd::Validador::de_esquema(fixtures().join("minimo.xsd")).expect("compila");
    let valido = r#"<nota xmlns="urn:carimbar:teste" versao="1.00"><numero>1</numero><valor>1</valor></nota>"#;
    let invalido = r#"<nota xmlns="urn:carimbar:teste" versao="2.00"><numero>1</numero><valor>1</valor></nota>"#;
    assert!(validador.validar(valido.as_bytes()).is_ok());
    assert!(validador.validar(invalido.as_bytes()).is_err());
}

#[test]
fn carrega_e_valida_contra_dps_oficial() {
    let esquema = esquema_oficial("nfse_v1.01_20261020", "DPS_v1.01.xsd");
    let invalido =
        r#"<DPS xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.01"><infDPS/></DPS>"#;
    let erro = carimbar_xml::xsd::validar(invalido.as_bytes(), &esquema)
        .expect_err("DPS incompleta deve falhar");
    match erro {
        ErroXml::Xsd(mensagem) => {
            assert!(mensagem.contains("cvc-"), "sem código cvc: {mensagem}");
        }
        outro => panic!("erro inesperado: {outro}"),
    }
}

#[test]
fn dps_sintetica_completa_passa_no_xsd_oficial() {
    let xml = r#"<DPS xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.01">
  <infDPS Id="DPS355030810000000100013600001000000000000001">
    <tpAmb>2</tpAmb>
    <dhEmi>2026-11-08T10:00:00-03:00</dhEmi>
    <verAplic>carimbar-0.0.1</verAplic>
    <serie>1</serie>
    <nDPS>1</nDPS>
    <dCompet>2026-11-08</dCompet>
    <tpEmit>1</tpEmit>
    <finNFSe>0</finNFSe>
    <cLocEmi>3550308</cLocEmi>
    <prest>
      <CNPJ>00000001000136</CNPJ>
      <regTrib>
        <opSimpNac>1</opSimpNac>
        <regEspTrib>0</regEspTrib>
      </regTrib>
    </prest>
    <serv>
      <locPrest><cLocPrestacao>3550308</cLocPrestacao></locPrest>
      <cServ>
        <cTribNac>010101</cTribNac>
        <xDescServ>Servico sintetico de teste</xDescServ>
      </cServ>
    </serv>
    <valores>
      <vServPrest><vServ>10.00</vServ></vServPrest>
      <trib>
        <tribMun><tribISSQN>1</tribISSQN><tpRetISSQN>1</tpRetISSQN></tribMun>
        <totTrib><indTotTrib>0</indTotTrib></totTrib>
      </trib>
    </valores>
  </infDPS>
</DPS>"#;
    carimbar_xml::xsd::validar(
        xml.as_bytes(),
        esquema_oficial("nfse_v1.01_20261020", "DPS_v1.01.xsd"),
    )
    .expect("DPS sintética válida");
}

#[test]
fn carrega_e_valida_contra_nfe_oficial() {
    let esquema = esquema_oficial("PL_010b_v1.30", "nfe_v4.00.xsd");
    let invalido =
        r#"<NFe xmlns="http://www.portalfiscal.inf.br/nfe"><infNFe versao="4.00"/></NFe>"#;
    let erro = carimbar_xml::xsd::validar(invalido.as_bytes(), &esquema)
        .expect_err("NF-e incompleta deve falhar");
    match erro {
        ErroXml::Xsd(mensagem) => {
            assert!(mensagem.contains("cvc-"), "sem código cvc: {mensagem}");
        }
        outro => panic!("erro inesperado: {outro}"),
    }
}

#[test]
fn documento_serializado_valida_pelo_metodo_do_documento() {
    let xml = r#"<nota xmlns="urn:carimbar:teste" versao="1.00"><numero>7</numero><valor>0.01</valor></nota>"#;
    let documento = Documento::parse(xml.as_bytes()).expect("lê");
    documento
        .validar_xsd(fixtures().join("minimo.xsd"))
        .expect("valida");
}
