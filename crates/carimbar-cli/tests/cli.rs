//! Testes de processo da CLI com certificado sintético e transporte replay.

use carimbar_nfse::comprimir_b64;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const CNPJ: &str = "00000001000136";
const SENHA: &str = "senha-sintetica-123";
const KEK: &str = "0707070707070707070707070707070707070707070707070707070707070707";
const ID_DPS: &str = "DPS355030810000000100013600001000000000000001";

fn chave() -> String {
    format!("{}{}{}", "355030", CNPJ, "0".repeat(30))
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_carimbar")
}

fn diretorio(nome: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("cli-{nome}"));
    std::fs::create_dir_all(&dir).expect("diretório");
    for arquivo in [
        "cofre.db",
        "cofre.db-wal",
        "cofre.db-shm",
        "journal.db",
        "journal.db-wal",
        "journal.db-shm",
    ] {
        let _ = std::fs::remove_file(dir.join(arquivo));
    }
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
        .expect("openssl disponível");
    assert!(
        saida.status.success(),
        "openssl {args:?}: {}",
        String::from_utf8_lossy(&saida.stderr)
    );
}

fn pfx(dir: &Path) -> PathBuf {
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
        chave.to_str().expect("utf-8"),
        "-out",
        cert.to_str().expect("utf-8"),
        "-subj",
        "/CN=EMPRESA SINTETICA CLI",
        "-addext",
        &format!("subjectAltName=otherName:2.16.76.1.3.3;UTF8:{CNPJ}"),
    ]);
    openssl(&[
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
    ]);
    pfx
}

fn comando(dir: &Path, args: &[&str]) -> Command {
    let mut comando = Command::new(bin());
    comando
        .current_dir(dir)
        .env("CARIMBAR_KEK", KEK)
        .env("CARIMBAR_COFRE", dir.join("cofre.db"))
        .env("CARIMBAR_JOURNAL", dir.join("journal.db"))
        .env("CARIMBAR_SENHA_PFX", SENHA)
        .env_remove("CARIMBAR_PRODUCAO")
        .args(args);
    comando
}

fn rodar(comando: &mut Command) -> Output {
    comando
        .stdin(Stdio::null())
        .output()
        .expect("executa carimbar")
}

fn dps_xml() -> String {
    format!(
        r#"<DPS xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.01"><infDPS Id="{ID_DPS}"><tpAmb>2</tpAmb><prest><CNPJ>{CNPJ}</CNPJ></prest></infDPS></DPS>"#
    )
}

fn nfse_xml() -> String {
    format!(
        r#"<NFSe xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.01"><infNFSe Id="NFS{}"><xLocEmi>Sao Paulo</xLocEmi><xLocPrestacao>Sao Paulo</xLocPrestacao><nNFSe>123</nNFSe><cStat>100</cStat><dhProc>2026-11-09T10:00:00-03:00</dhProc><nDFSe>1</nDFSe><emit><CNPJ>{CNPJ}</CNPJ><IM>12345</IM><xNome>Empresa Sintetica</xNome><enderNac><xLgr>Rua A</xLgr><nro>1</nro><xBairro>Centro</xBairro><cMun>3550308</cMun><UF>SP</UF><CEP>01001000</CEP></enderNac></emit><valores><vBC>10.00</vBC><pAliqAplic>2.00</pAliqAplic><vISSQN>0.20</vISSQN><vLiq>9.80</vLiq></valores><DPS versao="1.01"><infDPS Id="{ID_DPS}"><tpAmb>2</tpAmb><serv><cServ><cTribNac>010101</cTribNac><xDescServ>Servico sintetico</xDescServ></cServ></serv><valores><vServPrest><vServ>10.00</vServ></vServPrest><trib><tribMun><tribISSQN>1</tribISSQN></tribMun></trib></valores></infDPS></DPS></infNFSe></NFSe>"#,
        chave()
    )
}

fn preparar_empresa(dir: &Path) {
    let pfx = pfx(dir);
    let saida = rodar(&mut comando(
        dir,
        &[
            "--empresa",
            CNPJ,
            "empresa",
            "importar",
            "--id",
            "a1",
            "--pfx",
            pfx.to_str().expect("utf-8"),
        ],
    ));
    assert!(
        saida.status.success(),
        "importar: {}",
        String::from_utf8_lossy(&saida.stderr)
    );
}

#[test]
fn help_e_version_funcionam() {
    let dir = diretorio("help");
    let ajuda = rodar(&mut comando(&dir, &["--help"]));
    assert!(ajuda.status.success());
    assert!(String::from_utf8_lossy(&ajuda.stdout).contains("empresa"));
    let versao = rodar(&mut comando(&dir, &["--version"]));
    assert!(versao.status.success());
    assert!(String::from_utf8_lossy(&versao.stdout).contains("carimbar"));
}

#[test]
fn falta_de_empresa_tem_codigo_de_uso() {
    let dir = diretorio("sem-empresa");
    let saida = rodar(&mut comando(&dir, &["empresa", "listar"]));
    assert_eq!(saida.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&saida.stderr).contains("--empresa"));
}

#[test]
fn fluxo_de_emissao_simulado_produz_xml_e_pdf() {
    let dir = diretorio("emissao");
    preparar_empresa(&dir);
    std::fs::write(dir.join("dps.xml"), dps_xml()).expect("dps");
    let corpo = json!({
        "idDps": ID_DPS,
        "chaveAcesso": chave(),
        "nfseXmlGZipB64": comprimir_b64(&nfse_xml()).expect("gzip"),
        "dataHoraProcessamento": "2026-11-09T10:00:00-03:00",
        "alertas": []
    })
    .to_string();
    let replay = json!({
        "respostas": [
            {"url_contem": "/nfse", "status": 201, "corpo": corpo}
        ]
    });
    std::fs::write(dir.join("replay.json"), replay.to_string()).expect("replay");

    let saida = rodar(&mut comando(
        &dir,
        &[
            "--empresa",
            CNPJ,
            "--json",
            "emitir",
            "--dps",
            "dps.xml",
            "--xml-saida",
            "autorizada.xml",
            "--pdf-saida",
            "danfse.pdf",
            "--replay",
            "replay.json",
        ],
    ));
    assert!(
        saida.status.success(),
        "emitir: stderr={} stdout={}",
        String::from_utf8_lossy(&saida.stderr),
        String::from_utf8_lossy(&saida.stdout)
    );
    let json: serde_json::Value = serde_json::from_slice(&saida.stdout).expect("JSON de saída");
    assert_eq!(json["estado"], "autorizada");
    assert_eq!(json["chave"], chave());
    let xml = std::fs::read(dir.join("autorizada.xml")).expect("xml");
    assert!(String::from_utf8_lossy(&xml).contains("<NFSe"));
    let pdf = std::fs::read(dir.join("danfse.pdf")).expect("pdf");
    assert!(pdf.starts_with(b"%PDF"));
}

#[test]
fn kill_switch_bloqueia_promocao_de_producao() {
    let dir = diretorio("producao");
    preparar_empresa(&dir);
    let mut comando = comando(
        &dir,
        &[
            "--empresa",
            CNPJ,
            "empresa",
            "promover",
            "--id",
            "a1",
            "--confirmar",
            CNPJ,
        ],
    );
    comando.env("CARIMBAR_PRODUCAO", "0");
    let saida = rodar(&mut comando);
    assert_eq!(saida.status.code(), Some(5));
    assert!(String::from_utf8_lossy(&saida.stderr).contains("produção bloqueada"));
}

#[test]
fn rejeicao_fiscal_tem_codigo_tres() {
    let dir = diretorio("rejeicao");
    preparar_empresa(&dir);
    std::fs::write(dir.join("dps.xml"), dps_xml()).expect("dps");
    let corpo = json!({
        "idDps": ID_DPS,
        "erros": [{"codigo": "E1", "descricao": "DPS invalida"}]
    })
    .to_string();
    let replay = json!({
        "respostas": [
            {"url_contem": "/nfse", "status": 400, "corpo": corpo}
        ]
    });
    std::fs::write(dir.join("replay.json"), replay.to_string()).expect("replay");
    let saida = rodar(&mut comando(
        &dir,
        &[
            "--empresa",
            CNPJ,
            "emitir",
            "--dps",
            "dps.xml",
            "--replay",
            "replay.json",
        ],
    ));
    assert_eq!(saida.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&saida.stderr).contains("rejeitada"));
}

#[cfg(feature = "nfe-sha1")]
#[test]
fn fluxo_nfe_simulado_pela_cli() {
    let dir = diretorio("nfe");
    preparar_empresa(&dir);
    let chave = carimbar_types::ChaveAcesso::com_dv("3526110000000100013655001000000000112345678")
        .expect("chave")
        .as_str()
        .to_owned();
    let nfe = format!(
        r#"<NFe xmlns="http://www.portalfiscal.inf.br/nfe"><infNFe versao="4.00" Id="NFe{chave}"><ide><cUF>35</cUF><tpAmb>2</tpAmb></ide><emit><CNPJ>{CNPJ}</CNPJ></emit></infNFe></NFe>"#
    );
    std::fs::write(dir.join("nfe.xml"), &nfe).expect("nfe");
    let corpo = format!(
        r#"<retEnviNFe xmlns="http://www.portalfiscal.inf.br/nfe" versao="4.00"><cStat>104</cStat><xMotivo>Lote processado</xMotivo><protNFe versao="4.00"><infProt><chNFe>{chave}</chNFe><nProt>135</nProt><cStat>100</cStat><xMotivo>Autorizado</xMotivo></infProt></protNFe></retEnviNFe>"#
    );
    let replay =
        json!({"respostas": [{"url_contem": "nfeautorizacao4", "status": 200, "corpo": corpo}]});
    std::fs::write(dir.join("replay.json"), replay.to_string()).expect("replay");
    let saida = rodar(&mut comando(
        &dir,
        &[
            "--empresa",
            CNPJ,
            "--json",
            "emitir-nfe",
            "--nfe",
            "nfe.xml",
            "--saida",
            "autorizada.xml",
            "--replay",
            "replay.json",
        ],
    ));
    assert!(
        saida.status.success(),
        "emitir-nfe: stderr={} stdout={}",
        String::from_utf8_lossy(&saida.stderr),
        String::from_utf8_lossy(&saida.stdout)
    );
    let json: serde_json::Value = serde_json::from_slice(&saida.stdout).expect("JSON");
    assert_eq!(json["estado"], "autorizada");
    assert_eq!(json["chave"], chave);
    assert!(dir.join("autorizada.xml").exists());
}
