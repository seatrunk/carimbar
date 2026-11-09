//! Assinatura e verificação XMLDSig com material sintético gerado em tempo de teste.
//! Os testes de interoperabilidade usam o CLI `xmlsec1`. Nada é versionado.

use base64::Engine;
use carimbar::cert::CertificadoA1;
use carimbar::dsig::{ErroDsig, assinar, verificar};
use carimbar::types::Cnpj;
use carimbar::xml::Documento;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SENHA: &str = "senha-sintetica-123";
const CNPJ: &str = "00000001000136";
const ID: &str = "DPS355030810000000100013600001000000000000001";

struct A1Sintetico {
    certificado: CertificadoA1,
    cert_pem: PathBuf,
}

fn diretorio(nome: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("dsig-{nome}"));
    std::fs::create_dir_all(&dir).expect("cria diretório temporário");
    dir
}

fn openssl(args: &[&str], envs: &[(&str, &str)]) {
    let saida = Command::new("openssl")
        .args(args)
        .envs(envs.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("openssl disponível no PATH");
    assert!(
        saida.status.success(),
        "openssl {args:?}: {}",
        String::from_utf8_lossy(&saida.stderr)
    );
}

fn a1(nome: &str) -> A1Sintetico {
    let dir = diretorio(nome);
    let chave = dir.join("chave.pem");
    let cert = dir.join("cert.pem");
    let pfx = dir.join("a1.p12");
    openssl(
        &[
            "req",
            "-x509",
            "-nodes",
            "-sha256",
            "-days",
            "3650",
            "-newkey",
            "rsa:2048",
            "-keyout",
            chave.to_str().expect("utf-8"),
            "-out",
            cert.to_str().expect("utf-8"),
            "-subj",
            &format!("/CN=EMPRESA SINTETICA {nome}"),
            "-addext",
            &format!("subjectAltName=otherName:2.16.76.1.3.3;UTF8:{CNPJ}"),
        ],
        &[],
    );
    openssl(
        &[
            "pkcs12",
            "-export",
            "-in",
            cert.to_str().expect("utf-8"),
            "-inkey",
            chave.to_str().expect("utf-8"),
            "-out",
            pfx.to_str().expect("utf-8"),
            "-passout",
            "env:CARIMBAR_SENHA_TESTE",
        ],
        &[("CARIMBAR_SENHA_TESTE", SENHA)],
    );
    let bytes = std::fs::read(pfx).expect("lê PKCS#12");
    let cnpj = Cnpj::parse(CNPJ).expect("CNPJ sintético");
    let certificado = CertificadoA1::abrir(&bytes, SENHA, &cnpj).expect("abre A1 sintético");
    A1Sintetico {
        certificado,
        cert_pem: cert,
    }
}

fn dps(id: &str) -> Documento {
    Documento::parse(dps_xml(id).as_bytes()).expect("DPS sintética")
}

fn dps_xml(id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<DPS xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.01"><infDPS Id="{id}"><tpAmb>2</tpAmb><dhEmi>2026-11-08T10:00:00-03:00</dhEmi><verAplic>carimbar-0.0.1</verAplic><serie>1</serie><nDPS>1</nDPS><dCompet>2026-11-08</dCompet><tpEmit>1</tpEmit><finNFSe>0</finNFSe><cLocEmi>3550308</cLocEmi><prest><CNPJ>{CNPJ}</CNPJ><regTrib><opSimpNac>1</opSimpNac><regEspTrib>0</regEspTrib></regTrib></prest><serv><locPrest><cLocPrestacao>3550308</cLocPrestacao></locPrest><cServ><cTribNac>010101</cTribNac><xDescServ>Servico sintetico</xDescServ></cServ></serv><valores><vServPrest><vServ>10.00</vServ></vServPrest><trib><tribMun><tribISSQN>1</tribISSQN><tpRetISSQN>1</tpRetISSQN></tribMun><totTrib><indTotTrib>0</indTotTrib></totTrib></trib></valores></infDPS></DPS>"#
    )
}

/// Substitui um trecho exato e exige que ele exista, para que o teste não passe por engano.
fn trocar(xml: &str, antigo: &str, novo: &str) -> String {
    assert_eq!(
        xml.matches(antigo).count(),
        1,
        "trecho não encontrado: {antigo}"
    );
    xml.replacen(antigo, novo, 1)
}

fn assinada(nome: &str) -> (A1Sintetico, String) {
    let a1 = a1(nome);
    let assinado = assinar(&dps(ID), ID, &a1.certificado).expect("assina");
    let xml = assinado.to_xml().expect("serializa");
    (a1, xml)
}

fn xmlsec1(args: &[&str]) -> std::process::Output {
    Command::new("xmlsec1")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("xmlsec1 disponível no PATH")
}

#[test]
fn assina_e_verifica_o_elemento_com_o_certificado_do_a1() {
    let a1 = a1("basico");
    let assinado = assinar(&dps(ID), ID, &a1.certificado).expect("assina");
    let verificacao = verificar(&assinado).expect("verifica");
    assert_eq!(verificacao.id(), ID);
    assert_eq!(
        verificacao.certificado_der(),
        a1.certificado.certificado_der()
    );
}

#[test]
fn assinatura_sobrevive_a_serializar_e_reler() {
    let (a1, xml) = assinada("reler");
    let relido = Documento::parse(xml.as_bytes()).expect("relê");
    let verificacao = verificar(&relido).expect("verifica após reler");
    assert_eq!(
        verificacao.certificado_der(),
        a1.certificado.certificado_der()
    );
}

#[test]
#[cfg(feature = "xsd")]
fn assinada_valida_no_xsd_oficial_da_dps() {
    let (_a1, xml) = assinada("xsd");
    let relido = Documento::parse(xml.as_bytes()).expect("relê");
    let esquema = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/nfse_v1.01_20261020/DPS_v1.01.xsd");
    relido
        .validar_xsd(esquema)
        .expect("DPS assinada é válida no XSD");
}

#[test]
fn alteracao_do_conteudo_assinado_e_rejeitada() {
    let (_a1, xml) = assinada("alterado");
    let alterado =
        Documento::parse(trocar(&xml, "<tpAmb>2</tpAmb>", "<tpAmb>1</tpAmb>").as_bytes())
            .expect("relê");
    assert_eq!(verificar(&alterado), Err(ErroDsig::DigestDivergente));
}

#[test]
fn assinatura_adulterada_e_rejeitada() {
    let (_a1, xml) = assinada("adulterada");
    let inicio =
        xml.find("<ds:SignatureValue>").expect("SignatureValue") + "<ds:SignatureValue>".len();
    let trecho = &xml[inicio..inicio + 1];
    let trocado = if trecho == "A" { "B" } else { "A" };
    let adulterado = format!("{}{trocado}{}", &xml[..inicio], &xml[inicio + 1..]);
    let relido = Documento::parse(adulterado.as_bytes()).expect("relê");
    assert_eq!(verificar(&relido), Err(ErroDsig::AssinaturaInvalida));
}

#[test]
fn id_duplicado_e_rejeitado_na_verificacao_e_na_assinatura() {
    let (_a1, xml) = assinada("duplicado");
    let duplicado = trocar(&xml, "<serv>", &format!("<serv Id=\"{ID}\">"));
    let relido = Documento::parse(duplicado.as_bytes()).expect("relê");
    assert_eq!(
        verificar(&relido),
        Err(ErroDsig::ReferenciaAmbigua {
            id: ID.to_owned(),
            ocorrencias: 2,
        })
    );

    let a1 = a1("duplicado-assina");
    let original = Documento::parse(
        dps_xml(ID)
            .replace("<serv>", &format!("<serv Id=\"{ID}\">"))
            .as_bytes(),
    )
    .expect("DPS com Id duplicado");
    assert_eq!(
        assinar(&original, ID, &a1.certificado).map(|_| ()),
        Err(ErroDsig::ReferenciaAmbigua {
            id: ID.to_owned(),
            ocorrencias: 2,
        })
    );
}

#[test]
fn referencia_divergente_e_rejeitada() {
    let (_a1, xml) = assinada("divergente");
    let trocado = trocar(&xml, &format!("URI=\"#{ID}\""), "URI=\"#OUTRO\"");
    let relido = Documento::parse(trocado.as_bytes()).expect("relê");
    assert_eq!(
        verificar(&relido),
        Err(ErroDsig::ReferenciaNaoEncontrada("OUTRO".to_owned()))
    );
}

#[test]
fn algoritmo_de_assinatura_nao_suportado_e_rejeitado() {
    let (_a1, xml) = assinada("sha1");
    let trocado = trocar(&xml, "xmldsig-more#rsa-sha256", "xmldsig#rsa-sha1");
    let relido = Documento::parse(trocado.as_bytes()).expect("relê");
    assert!(matches!(
        verificar(&relido),
        Err(ErroDsig::AlgoritmoNaoSuportado {
            campo: "SignatureMethod",
            ..
        })
    ));
}

#[test]
fn c14n_com_comentarios_e_rejeitada() {
    let (_a1, xml) = assinada("c14n");
    let trocado = trocar(
        &xml,
        "<ds:CanonicalizationMethod Algorithm=\"http://www.w3.org/TR/2001/REC-xml-c14n-20010315\"/>",
        "<ds:CanonicalizationMethod Algorithm=\"http://www.w3.org/TR/2001/REC-xml-c14n-20010315#WithComments\"/>",
    );
    let relido = Documento::parse(trocado.as_bytes()).expect("relê");
    assert!(matches!(
        verificar(&relido),
        Err(ErroDsig::AlgoritmoNaoSuportado {
            campo: "CanonicalizationMethod",
            ..
        })
    ));
}

#[test]
fn documento_sem_assinatura_e_rejeitado() {
    assert_eq!(verificar(&dps(ID)), Err(ErroDsig::SemAssinatura));
}

#[test]
fn duas_assinaturas_sao_rejeitadas() {
    let (_a1, xml) = assinada("duas");
    let inicio = xml.find("<ds:Signature ").expect("Signature");
    let assinatura = &xml[inicio..xml.find("</DPS>").expect("DPS")];
    let duplicado = xml.replace("</DPS>", &format!("{assinatura}</DPS>"));
    let relido = Documento::parse(duplicado.as_bytes()).expect("relê");
    assert_eq!(verificar(&relido), Err(ErroDsig::MultiplasAssinaturas(2)));
}

#[test]
fn assinar_documento_ja_assinado_e_rejeitado() {
    let (a1, xml) = assinada("ja-assinado");
    let relido = Documento::parse(xml.as_bytes()).expect("relê");
    assert!(matches!(
        assinar(&relido, ID, &a1.certificado),
        Err(ErroDsig::Estrutura(_))
    ));
}

#[test]
fn verificacao_nao_declara_o_certificado_confiavel() {
    // O A1 sintético é autoassinado: a verificação criptográfica passa, e a
    // confiança fica para quem usa o resultado.
    let (a1, xml) = assinada("confianca");
    let relido = Documento::parse(xml.as_bytes()).expect("relê");
    let verificacao = verificar(&relido).expect("verifica");
    assert_eq!(
        verificacao.certificado_der(),
        a1.certificado.certificado_der()
    );
}

#[test]
fn xml_assinado_nao_contem_a_chave_privada() {
    let a1 = a1("segredo");
    let assinado = assinar(&dps(ID), ID, &a1.certificado).expect("assina");
    let xml = assinado.to_xml().expect("serializa");
    let chave = base64::engine::general_purpose::STANDARD
        .encode(a1.certificado.identidade_tls().chave_pkcs8_der());
    assert!(!xml.contains(&chave), "chave privada apareceu no XML");
    assert!(!xml.contains(SENHA));
}

#[test]
fn certificado_com_bytes_excedentes_no_key_info_e_rejeitado() {
    let (_a1, xml) = assinada("der-excedente");
    let tag = "<ds:X509Certificate>";
    let inicio = xml.find(tag).expect("certificado") + tag.len();
    let fim = xml
        .find("</ds:X509Certificate>")
        .expect("fim do certificado");
    let base64 = base64::engine::general_purpose::STANDARD;
    let mut der = base64.decode(&xml[inicio..fim]).expect("DER base64");
    der.extend_from_slice(b"conteudo-excedente");
    // KeyInfo não é assinado: a assinatura continua válida, mas o DER é inválido.
    let alterado = format!("{}{}{}", &xml[..inicio], base64.encode(der), &xml[fim..]);
    let documento = Documento::parse(alterado.as_bytes()).expect("XML válido");
    assert_eq!(verificar(&documento), Err(ErroDsig::CertificadoInvalido));
}

#[test]
fn estrutura_e_transformadas_fora_do_perfil_sao_rejeitadas() {
    let (_a1, xml) = assinada("estrutura");
    let enveloped = format!(
        "<ds:Transform Algorithm=\"{}\"/>",
        carimbar::dsig::ALG_ENVELOPED
    );
    let c14n = format!("<ds:Transform Algorithm=\"{}\"/>", carimbar::dsig::ALG_C14N);
    let cadeia = format!("<ds:Transforms>{enveloped}{c14n}</ds:Transforms>");
    let variantes = [
        trocar(&xml, &cadeia, ""),
        trocar(&xml, &cadeia, "<ds:Transforms/>"),
        trocar(
            &xml,
            &cadeia,
            &format!("<ds:Transforms>{enveloped}</ds:Transforms>"),
        ),
        trocar(
            &xml,
            &cadeia,
            &format!("<ds:Transforms>{c14n}{enveloped}</ds:Transforms>"),
        ),
        trocar(
            &xml,
            &cadeia,
            &format!("<ds:Transforms>{enveloped}{enveloped}</ds:Transforms>"),
        ),
        trocar(&xml, &cadeia, &format!("{cadeia}{cadeia}")),
        trocar(
            &xml,
            &enveloped,
            &format!(
                "<ds:Transform Algorithm=\"{}\"><ds:XPath>1</ds:XPath></ds:Transform>",
                carimbar::dsig::ALG_ENVELOPED
            ),
        ),
        trocar(
            &xml,
            "<ds:SignedInfo>",
            "<ds:SignedInfo><externo xmlns=\"urn:outro\"/>",
        ),
        trocar(&xml, "<ds:DigestValue>", "<ds:DigestValue><ds:Inesperado/>"),
    ];
    for variante in variantes {
        let documento = Documento::parse(variante.as_bytes()).expect("XML bem formado");
        assert!(matches!(
            verificar(&documento),
            Err(ErroDsig::Estrutura(_)) | Err(ErroDsig::AlgoritmoNaoSuportado { .. })
        ));
    }
}

#[test]
fn alvo_movido_para_dentro_de_outro_elemento_e_rejeitado() {
    let (_a1, xml) = assinada("alvo-movido");
    let alterado = xml
        .replace("<infDPS ", "<embrulho><infDPS ")
        .replace("</infDPS>", "</infDPS></embrulho>");
    let documento = Documento::parse(alterado.as_bytes()).expect("XML bem formado");
    assert!(matches!(verificar(&documento), Err(ErroDsig::Estrutura(_))));
    let a1 = a1("alvo-movido-assina");
    let original = dps_xml(ID)
        .replace("<infDPS ", "<embrulho><infDPS ")
        .replace("</infDPS>", "</infDPS></embrulho>");
    let documento = Documento::parse(original.as_bytes()).expect("XML bem formado");
    assert!(matches!(
        assinar(&documento, ID, &a1.certificado),
        Err(ErroDsig::Estrutura(_))
    ));
}

#[test]
fn xmlsec1_confere_heranca_de_atributos_xml_na_c14n_10() {
    let a1 = a1("atributos-xml");
    let xml = dps_xml(ID).replace("<DPS ", "<DPS xml:lang=\"pt-BR\" xml:space=\"preserve\" xml:base=\"https://exemplo.invalid/\" xml:id=\"raiz\" ");
    let documento = Documento::parse(xml.as_bytes()).expect("XML válido");
    let assinado = assinar(&documento, ID, &a1.certificado).expect("assina");
    verificar(&assinado).expect("verifica localmente");
    let arquivo = diretorio("atributos-xml").join("assinado.xml");
    std::fs::write(&arquivo, assinado.to_xml().expect("serializa")).expect("escreve");
    let saida = xmlsec1(&[
        "--verify",
        "--id-attr:Id",
        "http://www.sped.fazenda.gov.br/nfse:infDPS",
        "--trusted-pem",
        a1.cert_pem.to_str().expect("utf-8"),
        arquivo.to_str().expect("utf-8"),
    ]);
    assert!(
        saida.status.success(),
        "xmlsec1: {}",
        String::from_utf8_lossy(&saida.stderr)
    );
}

#[test]
fn xmlsec1_verifica_assinatura_gerada_pelo_carimbar() {
    let (a1, xml) = assinada("interop-saida");
    let dir = diretorio("interop-saida-xmlsec");
    let arquivo = dir.join("assinado.xml");
    std::fs::write(&arquivo, xml).expect("grava XML");
    let saida = xmlsec1(&[
        "--verify",
        "--id-attr:Id",
        "http://www.sped.fazenda.gov.br/nfse:infDPS",
        "--trusted-pem",
        a1.cert_pem.to_str().expect("utf-8"),
        arquivo.to_str().expect("utf-8"),
    ]);
    assert!(
        saida.status.success(),
        "xmlsec1 rejeitou: {}{}",
        String::from_utf8_lossy(&saida.stdout),
        String::from_utf8_lossy(&saida.stderr)
    );
}

#[test]
fn verifica_assinatura_gerada_pelo_xmlsec1() {
    let a1 = a1("interop-entrada");
    let dir = diretorio("interop-entrada-xmlsec");
    let modelo = dir.join("modelo.xml");
    let assinado = dir.join("xmlsec.xml");
    let chave = diretorio("interop-entrada").join("chave.pem");
    let cert = a1.cert_pem.clone();
    std::fs::write(&modelo, modelo_xmlsec(&dps_xml(ID))).expect("grava modelo");

    let saida = xmlsec1(&[
        "--sign",
        "--lax-key-search",
        "--id-attr:Id",
        "infDPS",
        "--privkey-pem",
        &format!("{},{}", chave.display(), cert.display()),
        "--output",
        assinado.to_str().expect("utf-8"),
        modelo.to_str().expect("utf-8"),
    ]);
    assert!(
        saida.status.success(),
        "xmlsec1 não assinou: {}{}",
        String::from_utf8_lossy(&saida.stdout),
        String::from_utf8_lossy(&saida.stderr)
    );

    let documento = Documento::parse(&std::fs::read(assinado).expect("lê XML do xmlsec1"))
        .expect("XML do xmlsec1 é válido");
    let verificacao = verificar(&documento).expect("verifica assinatura do xmlsec1");
    assert_eq!(verificacao.id(), ID);
}

/// Template XMLDSig com placeholder de certificado, que o `xmlsec1` preenche.
fn modelo_xmlsec(xml: &str) -> String {
    let assinatura = format!(
        "<ds:Signature xmlns:ds=\"http://www.w3.org/2000/09/xmldsig#\"><ds:SignedInfo><ds:CanonicalizationMethod Algorithm=\"http://www.w3.org/TR/2001/REC-xml-c14n-20010315\"/><ds:SignatureMethod Algorithm=\"http://www.w3.org/2001/04/xmldsig-more#rsa-sha256\"/><ds:Reference URI=\"#{ID}\"><ds:Transforms><ds:Transform Algorithm=\"http://www.w3.org/2000/09/xmldsig#enveloped-signature\"/><ds:Transform Algorithm=\"http://www.w3.org/TR/2001/REC-xml-c14n-20010315\"/></ds:Transforms><ds:DigestMethod Algorithm=\"http://www.w3.org/2001/04/xmlenc#sha256\"/><ds:DigestValue/></ds:Reference></ds:SignedInfo><ds:SignatureValue/><ds:KeyInfo><ds:X509Data><ds:X509Certificate/></ds:X509Data></ds:KeyInfo></ds:Signature>"
    );
    xml.replacen("</DPS>", &format!("{assinatura}</DPS>"), 1)
}

#[cfg(feature = "nfe-sha1")]
mod nfe {
    use super::*;
    use carimbar::dsig::{PerfilAssinatura, Signer, assinar_com_perfil, verificar_com_perfil};

    const ID_NFE: &str = "NFe43261100000001000136550010000001231000000420";
    const PERFIL: PerfilAssinatura = PerfilAssinatura::NfeSha1;

    // XML sintético para testar a assinatura de infNFe. O modelo completo da
    // nota e a validação de seu conteúdo fiscal pertencem à issue #18.
    fn nfe() -> Documento {
        Documento::parse(format!(
            r#"<NFe xmlns="http://www.portalfiscal.inf.br/nfe"><infNFe Id="{ID_NFE}" versao="4.00"><ide><tpAmb>2</tpAmb></ide></infNFe></NFe>"#,
        ).as_bytes()).expect("XML sintético")
    }

    fn assinada(nome: &str) -> (A1Sintetico, String) {
        let a1 = a1(nome);
        let documento =
            assinar_com_perfil(&nfe(), ID_NFE, &a1.certificado, PERFIL).expect("assina NF-e");
        (a1, documento.to_xml().expect("serializa"))
    }

    #[test]
    fn sha1_exige_selecao_explicita_e_sobrevive_a_releitura() {
        let (_a1, xml) = assinada("nfe-reler");
        let documento = Documento::parse(xml.as_bytes()).expect("relê");
        assert_eq!(
            verificar_com_perfil(&documento, PERFIL)
                .expect("verifica NF-e")
                .id(),
            ID_NFE
        );
        assert!(matches!(
            verificar(&documento),
            Err(ErroDsig::AlgoritmoNaoSuportado {
                campo: "SignatureMethod",
                ..
            })
        ));
        let a1 = a1("nfe-padrao");
        let sha256 = assinar(&nfe(), ID_NFE, &a1.certificado).expect("padrão continua SHA-256");
        verificar(&sha256).expect("verifica padrão");
        assert!(matches!(
            verificar_com_perfil(&sha256, PERFIL),
            Err(ErroDsig::AlgoritmoNaoSuportado {
                campo: "SignatureMethod",
                ..
            })
        ));
    }

    #[test]
    #[cfg(feature = "xsd")]
    fn assinatura_nfe_passa_no_xsd_que_fixa_rsa_sha1_e_sha1() {
        let a1 = a1("nfe-schema");
        let esquema = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/PL_010b_v1.30/xmldsig-core-schema_v1.01.xsd");
        for (perfil, esperado) in [(PERFIL, true), (PerfilAssinatura::Sha256, false)] {
            let assinado =
                assinar_com_perfil(&nfe(), ID_NFE, &a1.certificado, perfil).expect("assina");
            let assinatura = assinado
                .raiz()
                .filhos()
                .iter()
                .find_map(|no| match no {
                    carimbar::xml::No::Elemento(e) if e.nome().local() == "Signature" => {
                        Some(e.clone())
                    }
                    _ => None,
                })
                .expect("Signature");
            let documento = Documento::new(assinatura);
            assert_eq!(
                documento.validar_xsd(&esquema).is_ok(),
                esperado,
                "perfil {perfil:?}"
            );
        }
    }

    #[test]
    fn xmlsec1_verifica_assinatura_nfe_gerada_pelo_carimbar() {
        let (a1, xml) = assinada("nfe-interop-saida");
        let arquivo = diretorio("nfe-interop-saida").join("assinado.xml");
        std::fs::write(&arquivo, xml).expect("grava");
        let saida = xmlsec1(&[
            "--verify",
            "--id-attr:Id",
            "http://www.portalfiscal.inf.br/nfe:infNFe",
            "--trusted-pem",
            a1.cert_pem.to_str().expect("utf-8"),
            arquivo.to_str().expect("utf-8"),
        ]);
        assert!(
            saida.status.success(),
            "xmlsec1: {}",
            String::from_utf8_lossy(&saida.stderr)
        );
    }

    #[test]
    fn verifica_assinatura_nfe_gerada_pelo_xmlsec1() {
        let a1 = a1("nfe-interop-entrada");
        let dir = diretorio("nfe-interop-entrada");
        let modelo = dir.join("modelo.xml");
        let arquivo = dir.join("assinado.xml");
        // Reutiliza só o esqueleto XMLDSig independente usado no teste SHA-256.
        let template = modelo_xmlsec("<DPS></DPS>")
            .replace("<DPS>", "<NFe xmlns=\"http://www.portalfiscal.inf.br/nfe\">")
            .replace("</DPS>", "</NFe>")
            .replace(ID, ID_NFE)
            .replace(carimbar::dsig::ALG_RSA_SHA256, carimbar::dsig::ALG_RSA_SHA1)
            .replace(carimbar::dsig::ALG_SHA256, carimbar::dsig::ALG_SHA1)
            .replacen("<ds:Signature ", &format!("<infNFe Id=\"{ID_NFE}\" versao=\"4.00\"><ide><tpAmb>2</tpAmb></ide></infNFe><ds:Signature "), 1);
        std::fs::write(&modelo, template).expect("grava modelo");
        let saida = xmlsec1(&[
            "--sign",
            "--lax-key-search",
            "--id-attr:Id",
            "infNFe",
            "--privkey-pem",
            &format!(
                "{},{}",
                dir.join("chave.pem").display(),
                a1.cert_pem.display()
            ),
            "--output",
            arquivo.to_str().expect("utf-8"),
            modelo.to_str().expect("utf-8"),
        ]);
        assert!(
            saida.status.success(),
            "xmlsec1: {}",
            String::from_utf8_lossy(&saida.stderr)
        );
        let documento =
            Documento::parse(&std::fs::read(arquivo).expect("lê XML")).expect("XML válido");
        assert_eq!(
            verificar_com_perfil(&documento, PERFIL)
                .expect("verifica assinatura externa")
                .id(),
            ID_NFE
        );
    }

    #[test]
    fn rejeita_conteudo_alterado_digest_misto_e_id_duplicado_na_nfe() {
        let (_a1, xml) = assinada("nfe-adulterada");
        let alterado =
            Documento::parse(trocar(&xml, "<tpAmb>2</tpAmb>", "<tpAmb>1</tpAmb>").as_bytes())
                .expect("XML válido");
        assert_eq!(
            verificar_com_perfil(&alterado, PERFIL),
            Err(ErroDsig::DigestDivergente)
        );
        let misto = Documento::parse(
            trocar(&xml, carimbar::dsig::ALG_SHA1, carimbar::dsig::ALG_SHA256).as_bytes(),
        )
        .expect("XML válido");
        assert!(matches!(
            verificar_com_perfil(&misto, PERFIL),
            Err(ErroDsig::AlgoritmoNaoSuportado {
                campo: "DigestMethod",
                ..
            })
        ));
        let duplicado =
            Documento::parse(trocar(&xml, "<ide>", &format!("<ide Id=\"{ID_NFE}\">")).as_bytes())
                .expect("XML válido");
        assert!(matches!(
            verificar_com_perfil(&duplicado, PERFIL),
            Err(ErroDsig::ReferenciaAmbigua { ocorrencias: 2, .. })
        ));
        let inicio =
            xml.find("<ds:SignatureValue>").expect("SignatureValue") + "<ds:SignatureValue>".len();
        let novo = if &xml[inicio..inicio + 1] == "A" {
            "B"
        } else {
            "A"
        };
        let invalido = format!("{}{novo}{}", &xml[..inicio], &xml[inicio + 1..]);
        assert_eq!(
            verificar_com_perfil(
                &Documento::parse(invalido.as_bytes()).expect("XML válido"),
                PERFIL
            ),
            Err(ErroDsig::AssinaturaInvalida)
        );
    }

    #[test]
    fn assinante_sem_sha1_nao_dispara_fallback_para_sha256() {
        struct ApenasSha256<'a>(&'a CertificadoA1);
        impl Signer for ApenasSha256<'_> {
            fn certificado_der(&self) -> &[u8] {
                self.0.certificado_der()
            }
            fn assinar_rsa_sha256(&self, _dados: &[u8]) -> Result<Vec<u8>, ErroDsig> {
                panic!("não deve tentar SHA-256 quando foi pedido SHA-1");
            }
        }
        let a1 = a1("nfe-sem-fallback");
        assert!(matches!(
            assinar_com_perfil(&nfe(), ID_NFE, &ApenasSha256(&a1.certificado), PERFIL),
            Err(ErroDsig::Assinante(_))
        ));
    }
}
