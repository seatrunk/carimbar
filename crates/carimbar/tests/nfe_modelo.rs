//! Testes do modelo NF-e e da validação XSD.

use carimbar::endpoints::Uf;
use carimbar::nfe::{
    Destinatario, Emitente, Endereco, IbsCbsItem, Item, NotaFiscal, Pagamento, TotalIbsCbs,
};
use carimbar::types::{Cnpj, Dinheiro, TpAmb};
use std::str::FromStr;

fn endereco() -> Endereco {
    Endereco {
        logradouro: "Rua A".to_owned(),
        numero: "1".to_owned(),
        complemento: None,
        bairro: "Centro".to_owned(),
        codigo_municipio: "3550308".to_owned(),
        municipio: "Sao Paulo".to_owned(),
        uf: Uf::SaoPaulo,
        cep: "01001000".to_owned(),
    }
}

fn nota() -> NotaFiscal {
    NotaFiscal {
        ambiente: TpAmb::Homologacao,
        uf: Uf::SaoPaulo,
        serie: 1,
        numero: 1,
        c_nf: "12345678".to_owned(),
        nat_op: "Venda".to_owned(),
        dh_emi: "2026-11-09T10:00:00-03:00".to_owned(),
        tp_nf: 1,
        id_dest: 1,
        c_mun_fg: "3550308".to_owned(),
        tp_imp: 1,
        tp_emis: 1,
        fin_nfe: 1,
        ind_final: 1,
        ind_pres: 1,
        ver_proc: "carimbar-0.0.2".to_owned(),
        dh_cont: None,
        x_just: None,
        emitente: Emitente {
            cnpj: Cnpj::parse("00000001000136").expect("CNPJ"),
            nome: "Empresa Sintetica".to_owned(),
            fantasia: None,
            endereco: endereco(),
            ie: "123456789".to_owned(),
            crt: 3,
        },
        destinatario: Some(Destinatario {
            inscricao: "12345678000195".to_owned(),
            nome: "Cliente Sintetico".to_owned(),
            endereco: endereco(),
            ind_ie_dest: 9,
            ie: None,
        }),
        itens: vec![Item {
            codigo: "1".to_owned(),
            ean: "SEM GTIN".to_owned(),
            descricao: "Produto".to_owned(),
            ncm: "00000000".to_owned(),
            cfop: "5102".to_owned(),
            unidade: "UN".to_owned(),
            quantidade: carimbar::types::Decimal::from_str("1").expect("qtd"),
            valor_unitario: carimbar::types::Decimal::from_str("10.00").expect("valor"),
            valor_total: Dinheiro::from_centavos(1000),
            ibs_cbs: Some(IbsCbsItem {
                cst: "000".to_owned(),
                c_class_trib: "000001".to_owned(),
            }),
        }],
        mod_frete: 9,
        pagamento: Pagamento {
            tipo: "01".to_owned(),
            valor: Dinheiro::from_centavos(1000),
        },
        inf_cpl: Some("Teste sintetico".to_owned()),
        total_ibs_cbs: Some(TotalIbsCbs {
            v_bc: Dinheiro::from_centavos(1000),
            v_ibs: Dinheiro::ZERO,
            v_cbs: Dinheiro::ZERO,
        }),
    }
}

#[test]
fn chave_tem_44_posicoes_e_dv() {
    let chave = nota().chave().expect("chave");
    assert_eq!(chave.as_str().len(), 44);
    assert_eq!(chave.modelo(), Some(carimbar::types::ModeloDocumento::NFe));
    assert_eq!(chave.serie(), "001");
    assert_eq!(chave.numero(), "000000001");
}

#[test]
fn xml_contem_campos_obrigatorios_e_ibs_cbs() {
    let xml = nota().para_xml().expect("XML");
    assert!(xml.contains("xmlns=\"http://www.portalfiscal.inf.br/nfe\""));
    assert!(xml.contains("<infNFe versao=\"4.00\" Id=\"NFe"));
    assert!(xml.contains("<nNF>1</nNF>"));
    assert!(xml.contains("<vNF>10.00</vNF>"));
    assert!(xml.contains("<IBSCBS><CST>000</CST><cClassTrib>000001</cClassTrib></IBSCBS>"));
    assert!(xml.contains("<IBSCBSTot>"));
}

#[cfg(feature = "nfe-sha1")]
mod assinatura {
    use super::*;
    use carimbar::dsig::{ErroDsig, Signer};
    #[cfg(feature = "xsd")]
    use std::path::Path;

    struct AssinanteFalso;

    impl Signer for AssinanteFalso {
        fn certificado_der(&self) -> &[u8] {
            &[0x30, 0x82, 0x01, 0x00]
        }
        fn assinar_rsa_sha256(&self, _dados: &[u8]) -> Result<Vec<u8>, ErroDsig> {
            Ok(vec![0x30, 0x31, 0x32, 0x33])
        }
        fn assinar_rsa_sha1_nfe(&self, _dados: &[u8]) -> Result<Vec<u8>, ErroDsig> {
            Ok(vec![0x30, 0x31, 0x32, 0x33])
        }
    }

    #[test]
    #[cfg(feature = "xsd")]
    fn assinada_passa_no_xsd_oficial() {
        let assinada = nota().assinar(&AssinanteFalso).expect("assina");
        let xml = assinada.to_xml().expect("serializa");
        let esquema =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/PL_010b_v1.30/nfe_v4.00.xsd");
        carimbar::xml::xsd::validar(xml.as_bytes(), esquema).expect("NF-e assinada válida no XSD");
        assert!(xml.contains("Signature"));
    }
}
