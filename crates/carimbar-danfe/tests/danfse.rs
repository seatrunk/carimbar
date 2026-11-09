//! Testes de extração e geração do DANFSe com XML sintético.

use carimbar_danfe::{
    ErroDanfe, OpcoesDanfse, extrair_dados, gerar_danfse, gerar_danfse_com_evento,
};
use printpdf::{PdfDocument, PdfParseOptions};

const ID_NFSE: &str = "NFS35503080000000010001360000000000000000000000000";

fn nfse(cstat: &str, descricao: &str) -> String {
    format!(
        r#"<NFSe xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.01">
  <infNFSe Id="{ID_NFSE}">
    <xLocEmi>Sao Paulo</xLocEmi>
    <xLocPrestacao>Sao Paulo</xLocPrestacao>
    <nNFSe>123</nNFSe>
    <cStat>{cstat}</cStat>
    <dhProc>2026-11-09T10:00:00-03:00</dhProc>
    <nDFSe>1</nDFSe>
    <emit>
      <CNPJ>00000001000136</CNPJ>
      <IM>12345</IM>
      <xNome>Empresa Sintetica</xNome>
      <enderNac>
        <xLgr>Rua A</xLgr><nro>1</nro><xBairro>Centro</xBairro>
        <cMun>3550308</cMun><UF>SP</UF><CEP>01001000</CEP>
      </enderNac>
    </emit>
    <valores>
      <vBC>10.00</vBC><pAliqAplic>2.00</pAliqAplic><vISSQN>0.20</vISSQN><vLiq>9.80</vLiq>
    </valores>
    <DPS versao="1.01">
      <infDPS Id="DPS355030810000000100013600001000000000000001">
        <tpAmb>2</tpAmb>
        <toma><CNPJ>12345678000195</CNPJ><xNome>Tomador Sintetico</xNome></toma>
        <serv><cServ><cTribNac>010101</cTribNac><xDescServ>{descricao}</xDescServ></cServ></serv>
        <valores>
          <vServPrest><vServ>10.00</vServ></vServPrest>
          <trib><tribMun><tribISSQN>1</tribISSQN></tribMun></trib>
        </valores>
      </infDPS>
    </DPS>
  </infNFSe>
</NFSe>"#
    )
}

fn texto_pdf(bytes: &[u8]) -> String {
    let mut avisos = Vec::new();
    let documento =
        PdfDocument::parse(bytes, &PdfParseOptions::default(), &mut avisos).expect("lê PDF gerado");
    documento
        .extract_text()
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn extrai_campos_da_nfse_autorizada() {
    let dados = extrair_dados(&nfse("100", "Servico sintetico")).expect("dados");
    assert_eq!(dados.numero, "123");
    assert_eq!(dados.chave, &ID_NFSE[3..]);
    assert_eq!(dados.emitente.inscricao, "00000001000136");
    assert_eq!(dados.tomador.expect("tomador").nome, "Tomador Sintetico");
    assert_eq!(dados.valor_servico, "10.00");
    assert_eq!(dados.valor_liquido, "9.80");
    assert_eq!(dados.aliquota.as_deref(), Some("2.00"));
    assert!(!dados.cancelada);
}

#[test]
fn gera_pdf_com_dados_fiscais() {
    let bytes = gerar_danfse(&nfse("100", "Serviço sintético")).expect("PDF");
    assert!(bytes.starts_with(b"%PDF"));
    let texto = texto_pdf(&bytes);
    assert!(texto.contains("DANFSe"), "{texto}");
    assert!(texto.contains("123"), "{texto}");
    assert!(texto.contains("Empresa Sintetica"), "{texto}");
    assert!(texto.contains("Serviço sintético"), "{texto}");
    assert!(texto.contains("10.00"), "{texto}");
    assert!(texto.contains("9.80"), "{texto}");
    assert!(texto.contains("AMBIENTE DE HOMOLOGA"), "{texto}");
}

#[test]
fn dps_isolada_nao_vira_nota_autorizada() {
    let erro = gerar_danfse(
        r#"<DPS xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.01"><infDPS Id="DPS1"/></DPS>"#,
    )
    .expect_err("DPS não autorizada");
    assert!(matches!(erro, ErroDanfe::NaoAutorizada(_)));
}

#[test]
fn nfse_nao_autorizada_e_rejeitada() {
    let erro = extrair_dados(&nfse("999", "Servico")).expect_err("cStat inválido");
    assert!(matches!(erro, ErroDanfe::NaoAutorizada(_)));
}

#[test]
fn descricao_longa_gera_multiplas_paginas() {
    let descricao = "servico sintetico com descricao muito longa ".repeat(120);
    let bytes = gerar_danfse(&nfse("100", &descricao)).expect("PDF");
    let mut avisos = Vec::new();
    let documento =
        PdfDocument::parse(&bytes, &PdfParseOptions::default(), &mut avisos).expect("lê PDF");
    assert!(documento.page_count() > 1);
    let texto = texto_pdf(&bytes);
    assert!(texto.contains("Página 1/"), "{texto}");
}

#[test]
fn cancelamento_pelo_status_e_pelo_evento() {
    let dados = extrair_dados(&nfse("101", "Servico")).expect("dados");
    assert!(dados.cancelada);

    let evento = r#"<evento xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.01"><infEvento Id="EVT1"><pedRegEvento versao="1.01"><infPedReg Id="PRE1"><e101101><xDesc>Cancelamento de NFS-e</xDesc></e101101></infPedReg></pedRegEvento></infEvento></evento>"#;
    let bytes = gerar_danfse_com_evento(&nfse("100", "Servico"), Some(evento)).expect("PDF");
    let texto = texto_pdf(&bytes);
    assert!(texto.contains("CANCELADA"), "{texto}");
}

#[test]
fn opcoes_explicitas_marcam_cancelamento() {
    let bytes = carimbar_danfe::gerar_danfse_com_opcoes(
        &nfse("100", "Servico"),
        &OpcoesDanfse { cancelada: true },
    )
    .expect("PDF");
    let texto = texto_pdf(&bytes);
    assert!(texto.contains("CANCELADA"), "{texto}");
}
