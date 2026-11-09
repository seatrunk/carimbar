//! Testes da fachada NF-e com PKI sintética e transporte replay.

#![cfg(feature = "nfe-sha1")]

use carimbar::{Emissor, Journal, ResultadoNfe};
use carimbar_endpoints::Uf;
use carimbar_nfe::{
    Destinatario, Emitente, Endereco, IbsCbsItem, Item, NotaFiscal, Pagamento, TotalIbsCbs,
};
use carimbar_nfse::Executor;
use carimbar_transport::{ErroEnvio, Requisicao, Resposta};
use carimbar_types::{Cnpj, Dinheiro, TpAmb};
use carimbar_vault::{ChaveMestra, Cofre};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::sync::Mutex;

const CNPJ: &str = "00000001000136";
const SENHA: &str = "senha-sintetica-123";
const KEK: [u8; 32] = [5; 32];

fn diretorio() -> PathBuf {
    let unico = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duracao| duracao.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!("carimbar-nfe-{unico}"));
    std::fs::create_dir_all(&dir).expect("diretório");
    dir
}

fn openssl(args: &[&str]) {
    let saida = Command::new("openssl")
        .args(args)
        .env("CARIMBAR_SENHA_TESTE", SENHA)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("openssl");
    assert!(
        saida.status.success(),
        "openssl {args:?}: {}",
        String::from_utf8_lossy(&saida.stderr)
    );
}

fn pfx(dir: &Path) -> Vec<u8> {
    let chave = dir.join("chave.pem");
    let cert = dir.join("cert.pem");
    let pfx = dir.join("a1.p12");
    openssl(&[
        "req",
        "-x509",
        "-nodes",
        "-sha256",
        "-days",
        "3650",
        "-newkey",
        "rsa:2048",
        "-keyout",
        chave.to_str().expect("utf8"),
        "-out",
        cert.to_str().expect("utf8"),
        "-subj",
        "/CN=EMPRESA SINTETICA NFE",
        "-addext",
        &format!("subjectAltName=otherName:2.16.76.1.3.3;UTF8:{CNPJ}"),
    ]);
    openssl(&[
        "pkcs12",
        "-export",
        "-in",
        cert.to_str().expect("utf8"),
        "-inkey",
        chave.to_str().expect("utf8"),
        "-out",
        pfx.to_str().expect("utf8"),
        "-passout",
        "env:CARIMBAR_SENHA_TESTE",
    ]);
    std::fs::read(pfx).expect("pfx")
}

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
            cnpj: Cnpj::parse(CNPJ).expect("CNPJ"),
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
            quantidade: carimbar_types::Decimal::from_str("1").expect("qtd"),
            valor_unitario: carimbar_types::Decimal::from_str("10.00").expect("valor"),
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
        inf_cpl: None,
        total_ibs_cbs: Some(TotalIbsCbs {
            v_bc: Dinheiro::from_centavos(1000),
            v_ibs: Dinheiro::ZERO,
            v_cbs: Dinheiro::ZERO,
        }),
    }
}

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
            .expect("resposta"))
    }
}

fn resposta_autorizacao(chave: &str) -> Resposta {
    let corpo = format!(
        r#"<retEnviNFe xmlns="http://www.portalfiscal.inf.br/nfe" versao="4.00"><tpAmb>2</tpAmb><verAplic>teste</verAplic><cStat>104</cStat><xMotivo>Lote processado</xMotivo><cUF>35</cUF><dhRecbto>2026-11-09T10:00:00-03:00</dhRecbto><protNFe versao="4.00"><infProt><tpAmb>2</tpAmb><verAplic>teste</verAplic><chNFe>{chave}</chNFe><dhRecbto>2026-11-09T10:00:00-03:00</dhRecbto><nProt>135260000000001</nProt><digVal>AAAA</digVal><cStat>100</cStat><xMotivo>Autorizado</xMotivo></infProt></protNFe></retEnviNFe>"#
    );
    Resposta {
        status: 200,
        cabecalhos: vec![("content-type".to_owned(), "application/soap+xml".to_owned())],
        corpo: corpo.into_bytes(),
    }
}

#[test]
fn fluxo_nfe_prepara_assina_envia_e_persiste() {
    let dir = diretorio();
    let empresa = Cnpj::parse(CNPJ).expect("CNPJ");
    let mut cofre = Cofre::criar_em_memoria(Box::new(ChaveMestra::de_bytes(KEK))).expect("cofre");
    cofre
        .importar(&empresa, "a1", &pfx(&dir), SENHA)
        .expect("A1");
    let journal = Journal::em_memoria().expect("journal");
    let nota = nota();
    let chave = nota.chave().expect("chave");
    let executor = ExecutorFalso::com(vec![resposta_autorizacao(chave.as_str())]);
    let mut emissor = Emissor::novo(cofre, journal, empresa, &executor).expect("emissor");

    let preparada = emissor
        .preparar_nfe_xml(&nota.para_xml().expect("XML"))
        .expect("prepara");
    let resultado = emissor.confirmar_nfe(preparada.id()).expect("confirma");
    let ResultadoNfe::Autorizada {
        chave: retorno,
        xml,
        ..
    } = resultado
    else {
        panic!("esperava autorização: {resultado:?}");
    };
    assert_eq!(retorno.as_str(), chave.as_str());
    assert!(xml.contains("<nfeProc"));
    assert!(xml.contains("<protNFe"));
    let pedido = &executor.pedidos()[0];
    assert!(pedido.url.to_ascii_lowercase().contains("nfeautorizacao4"));
    let corpo = String::from_utf8_lossy(&pedido.corpo);
    assert!(corpo.contains("Signature"));
    let estado = emissor.journal().operacao(preparada.id()).expect("op");
    assert_eq!(estado.estado, carimbar::EstadoOperacao::Autorizada);
}

#[test]
fn numeracao_reserva_de_forma_atomica() {
    let journal = Journal::em_memoria().expect("journal");
    let empresa = Cnpj::parse(CNPJ).expect("CNPJ");
    journal.garantir_empresa(&empresa).expect("empresa");
    assert_eq!(
        journal
            .reservar_numero_nfe(&empresa, TpAmb::Homologacao, 1)
            .expect("1"),
        1
    );
    assert_eq!(
        journal
            .reservar_numero_nfe(&empresa, TpAmb::Homologacao, 1)
            .expect("2"),
        2
    );
    assert_eq!(
        journal
            .reservar_numero_nfe(&empresa, TpAmb::Homologacao, 2)
            .expect("série 2"),
        1
    );
}
