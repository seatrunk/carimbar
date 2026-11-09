//! Testes de extração e geração do DANFE.

use carimbar_danfe::{
    ErroDanfe, OpcoesDanfeNfe, extrair_dados_nfe, gerar_danfe, gerar_danfe_com_evento,
    gerar_danfe_com_opcoes,
};
use printpdf::{PdfDocument, PdfParseOptions};

const CHAVE: &str = "35261100000001000136550010000000001123456780";

fn nfe_proc(cstat: &str, itens: usize, cancelada: bool) -> String {
    let mut det = String::new();
    for indice in 1..=itens {
        det.push_str(&format!(
            r#"<det nItem="{indice}"><prod><cProd>{indice}</cProd><xProd>Produto {indice}</xProd><NCM>00000000</NCM><CFOP>5102</CFOP><uCom>UN</uCom><qCom>1.0000</qCom><vUnCom>10.0000000000</vUnCom><vProd>10.00</vProd></prod><imposto></imposto></det>"#
        ));
    }
    let out_inf = if cancelada {
        "<xOutInf>NF-e cancelada</xOutInf>"
    } else {
        ""
    };
    format!(
        r#"<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe" versao="4.00"><NFe><infNFe versao="4.00" Id="NFe{CHAVE}"><ide><cUF>35</cUF><natOp>Venda</natOp><mod>55</mod><serie>1</serie><nNF>1</nNF><dhEmi>2026-11-09T10:00:00-03:00</dhEmi><tpNF>1</tpNF><tpEmis>1</tpEmis><tpAmb>2</tpAmb></ide><emit><CNPJ>00000001000136</CNPJ><xNome>Empresa Sintetica</xNome><enderEmit><xLgr>Rua A</xLgr><nro>1</nro><xBairro>Centro</xBairro><cMun>3550308</cMun><xMun>Sao Paulo</xMun><UF>SP</UF><CEP>01001000</CEP></enderEmit><IE>123456789</IE>{out_inf}</emit><dest><CNPJ>12345678000195</CNPJ><xNome>Cliente Sintetico</xNome><enderDest><xLgr>Rua B</xLgr><nro>2</nro><xBairro>Centro</xBairro><cMun>3550308</cMun><xMun>Sao Paulo</xMun><UF>SP</UF><CEP>01001000</CEP></enderDest><indIEDest>9</indIEDest></dest>{det}<total><ICMSTot><vBC>0.00</vBC><vICMS>0.00</vICMS><vProd>10.00</vProd><vFrete>0.00</vFrete><vSeg>0.00</vSeg><vDesc>0.00</vDesc><vIPI>0.00</vIPI><vPIS>0.00</vPIS><vCOFINS>0.00</vCOFINS><vNF>10.00</vNF></ICMSTot></total><transp><modFrete>9</modFrete></transp><pag><detPag><tPag>01</tPag><vPag>10.00</vPag></detPag></pag><infAdic><infCpl>Teste sintetico</infCpl></infAdic></infNFe></NFe><protNFe versao="4.00"><infProt><chNFe>{CHAVE}</chNFe><nProt>135260000000001</nProt><cStat>{cstat}</cStat><xMotivo>Autorizado</xMotivo></infProt></protNFe></nfeProc>"#
    )
}

fn texto_pdf(bytes: &[u8]) -> String {
    let mut avisos = Vec::new();
    let documento =
        PdfDocument::parse(bytes, &PdfParseOptions::default(), &mut avisos).expect("lê PDF");
    documento
        .extract_text()
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn extrai_campos_do_nfe_proc() {
    let dados = extrair_dados_nfe(&nfe_proc("100", 1, false)).expect("dados");
    assert_eq!(dados.chave, CHAVE);
    assert_eq!(dados.protocolo, "135260000000001");
    assert_eq!(dados.emitente.nome, "Empresa Sintetica");
    assert_eq!(dados.destinatario.expect("dest").nome, "Cliente Sintetico");
    assert_eq!(dados.itens.len(), 1);
    assert_eq!(dados.totais.v_nf, "10.00");
    assert!(!dados.cancelada);
}

#[test]
fn gera_pdf_com_dados_e_barcode() {
    let bytes = gerar_danfe(&nfe_proc("100", 1, false)).expect("PDF");
    assert!(bytes.starts_with(b"%PDF"));
    let texto = texto_pdf(&bytes);
    assert!(texto.contains("DANFE"), "{texto}");
    assert!(texto.contains(CHAVE), "{texto}");
    assert!(texto.contains("Empresa Sintetica"), "{texto}");
    assert!(texto.contains("Produto 1"), "{texto}");
    assert!(texto.contains("10.00"), "{texto}");
    assert!(texto.contains("HOMOLOGA"), "{texto}");
}

#[test]
fn xml_sem_protocolo_nao_e_autorizado() {
    let erro =
        gerar_danfe(r#"<NFe xmlns="http://www.portalfiscal.inf.br/nfe"><infNFe Id="NFe1"/></NFe>"#)
            .expect_err("sem protocolo");
    assert!(matches!(erro, ErroDanfe::NaoAutorizada(_)));
}

#[test]
fn cstat_invalido_nao_e_autorizado() {
    let erro = extrair_dados_nfe(&nfe_proc("999", 1, false)).expect_err("cStat");
    assert!(matches!(erro, ErroDanfe::NaoAutorizada(_)));
}

#[test]
fn cancelamento_por_status_evento_e_opcao() {
    let dados = extrair_dados_nfe(&nfe_proc("101", 1, false)).expect("dados");
    assert!(dados.cancelada);

    let evento = r#"<evento xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.00"><infEvento Id="EVT1"><pedRegEvento><infPedReg><e101101><xDesc>Cancelamento de NFS-e</xDesc></e101101></infPedReg></pedRegEvento></infEvento></evento>"#;
    let bytes = gerar_danfe_com_evento(&nfe_proc("100", 1, false), Some(evento)).expect("PDF");
    assert!(texto_pdf(&bytes).contains("CANCELADA"));

    let bytes = gerar_danfe_com_opcoes(
        &nfe_proc("100", 1, false),
        &OpcoesDanfeNfe { cancelada: true },
    )
    .expect("PDF");
    assert!(texto_pdf(&bytes).contains("CANCELADA"));
}

#[test]
fn muitos_itens_geram_multiplas_paginas() {
    let bytes = gerar_danfe(&nfe_proc("100", 100, false)).expect("PDF");
    let mut avisos = Vec::new();
    let documento =
        PdfDocument::parse(&bytes, &PdfParseOptions::default(), &mut avisos).expect("lê PDF");
    assert!(documento.page_count() > 1);
    let texto = texto_pdf(&bytes);
    assert!(texto.contains("Folha 1/"), "{texto}");
}
