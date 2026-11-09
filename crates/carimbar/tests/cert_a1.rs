//! Testes do certificado A1 com material sintético gerado em tempo de teste pelo
//! CLI `openssl`. Nenhum certificado ou chave é versionado.

use carimbar::cert::{CertificadoA1, ErroCertificado};
use carimbar::types::Cnpj;
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SENHA: &str = "senha-sintetica-123";
const CNPJ_TITULAR: &str = "00000001000136";
const CNPJ_OUTRO: &str = "00000002000008";
const CNPJ_ALFANUMERICO: &str = "00000001000A40";

enum Chave {
    Rsa,
    Ec,
}

struct Opcoes<'a> {
    nome: &'a str,
    cnpj_no_san: Option<&'a str>,
    dias: u32,
    chave: Chave,
}

fn diretorio(nome: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("a1-{nome}"));
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

/// Gera um par chave/certificado sintético em `dir`, sem exportar.
fn gerar_par(dir: &Path, opcoes: &Opcoes<'_>) -> (PathBuf, PathBuf) {
    let chave = dir.join("chave.pem");
    let cert = dir.join("cert.pem");
    let dias = opcoes.dias.to_string();
    let mut args: Vec<String> = vec!["req", "-x509", "-nodes", "-sha256", "-days", &dias]
        .into_iter()
        .map(String::from)
        .collect();
    match opcoes.chave {
        Chave::Rsa => args.extend(["-newkey", "rsa:2048"].map(String::from)),
        Chave::Ec => args.extend(
            ["-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1"].map(String::from),
        ),
    }
    args.extend(
        [
            "-keyout",
            chave.to_str().expect("utf-8"),
            "-out",
            cert.to_str().expect("utf-8"),
            "-subj",
            &format!("/CN=EMPRESA SINTETICA {}", opcoes.nome),
        ]
        .map(String::from),
    );
    if let Some(cnpj) = opcoes.cnpj_no_san {
        args.push("-addext".to_owned());
        args.push(format!(
            "subjectAltName=otherName:2.16.76.1.3.3;UTF8:{cnpj}"
        ));
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    openssl(&refs, &[]);
    (chave, cert)
}

fn exportar(dir: &Path, chave: &Path, cert: &Path) -> Vec<u8> {
    let pfx = dir.join("a1.p12");
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
            "-name",
            "a1",
        ],
        &[("CARIMBAR_SENHA_TESTE", SENHA)],
    );
    std::fs::read(pfx).expect("lê o PKCS#12 gerado")
}

/// PKCS#12 A1 sintético com CNPJ no SAN (chave RSA 2048).
fn a1(nome: &str, cnpj_no_san: Option<&str>, dias: u32) -> Vec<u8> {
    let dir = diretorio(nome);
    let (chave, cert) = gerar_par(
        &dir,
        &Opcoes {
            nome,
            cnpj_no_san,
            dias,
            chave: Chave::Rsa,
        },
    );
    exportar(&dir, &chave, &cert)
}

fn cnpj(texto: &str) -> Cnpj {
    Cnpj::parse(texto).expect("CNPJ sintético com DV válido")
}

fn agora() -> SystemTime {
    SystemTime::now()
}

#[test]
fn abre_a1_valido_e_vincula_titular_ao_cnpj() {
    let pfx = a1("valido", Some(CNPJ_TITULAR), 3650);
    let certificado = CertificadoA1::abrir(&pfx, SENHA, &cnpj(CNPJ_TITULAR)).expect("abre");
    assert_eq!(certificado.cnpj().as_str(), CNPJ_TITULAR);
    assert!(certificado.valido_ate() > agora());
    assert!(!certificado.certificado_der().is_empty());
}

#[test]
fn aceita_cnpj_alfanumerico_no_titular() {
    let pfx = a1("alfanumerico", Some(CNPJ_ALFANUMERICO), 3650);
    let certificado = CertificadoA1::abrir(&pfx, SENHA, &cnpj(CNPJ_ALFANUMERICO)).expect("abre");
    assert_eq!(certificado.cnpj().as_str(), CNPJ_ALFANUMERICO);
}

#[test]
fn senha_incorreta_e_recusada_sem_revelar_a_senha() {
    let pfx = a1("senha", Some(CNPJ_TITULAR), 3650);
    let erro =
        CertificadoA1::abrir(&pfx, "outra-senha", &cnpj(CNPJ_TITULAR)).expect_err("senha errada");
    assert_eq!(erro, ErroCertificado::SenhaIncorreta);
    assert!(!erro.to_string().contains("outra-senha"));
}

#[test]
fn arquivo_que_nao_e_pkcs12_e_recusado() {
    let erro = CertificadoA1::abrir(b"nao sou um pkcs12", SENHA, &cnpj(CNPJ_TITULAR))
        .expect_err("arquivo inválido");
    assert!(
        matches!(
            erro,
            ErroCertificado::ArquivoInvalido | ErroCertificado::SenhaIncorreta
        ),
        "{erro}"
    );
}

#[test]
fn pkcs12_truncado_e_recusado() {
    let pfx = a1("truncado", Some(CNPJ_TITULAR), 3650);
    let metade = &pfx[..pfx.len() / 2];
    assert!(CertificadoA1::abrir(metade, SENHA, &cnpj(CNPJ_TITULAR)).is_err());
}

#[test]
fn arquivo_acima_do_limite_e_recusado_sem_ler() {
    let grande = vec![0_u8; 1024 * 1024 + 1];
    let erro = CertificadoA1::abrir(&grande, SENHA, &cnpj(CNPJ_TITULAR)).expect_err("grande");
    assert!(
        matches!(erro, ErroCertificado::ArquivoGrande { .. }),
        "{erro}"
    );
}

#[test]
fn certificado_expirado_e_recusado() {
    let pfx = a1("expirado", Some(CNPJ_TITULAR), 1);
    let daqui_cinco_anos = agora() + Duration::from_secs(5 * 365 * 24 * 3600);
    let erro = CertificadoA1::abrir_em(&pfx, SENHA, &cnpj(CNPJ_TITULAR), daqui_cinco_anos)
        .expect_err("expirado");
    assert!(matches!(erro, ErroCertificado::Expirado(_)), "{erro}");
}

#[test]
fn certificado_ainda_nao_valido_e_recusado() {
    let pfx = a1("futuro", Some(CNPJ_TITULAR), 3650);
    let erro = CertificadoA1::abrir_em(
        &pfx,
        SENHA,
        &cnpj(CNPJ_TITULAR),
        UNIX_EPOCH + Duration::from_secs(1),
    )
    .expect_err("ainda não válido");
    assert!(matches!(erro, ErroCertificado::AindaNaoValido(_)), "{erro}");
}

#[test]
fn titular_de_outro_cnpj_e_recusado() {
    let pfx = a1("titular", Some(CNPJ_TITULAR), 3650);
    let erro = CertificadoA1::abrir(&pfx, SENHA, &cnpj(CNPJ_OUTRO)).expect_err("titular");
    assert_eq!(
        erro,
        ErroCertificado::TitularIncompativel {
            esperado: CNPJ_OUTRO.to_owned(),
            encontrado: CNPJ_TITULAR.to_owned(),
        }
    );
}

#[test]
fn certificado_sem_cnpj_do_titular_e_recusado() {
    let pfx = a1("sem-cnpj", None, 3650);
    let erro = CertificadoA1::abrir(&pfx, SENHA, &cnpj(CNPJ_TITULAR)).expect_err("sem CNPJ");
    assert_eq!(erro, ErroCertificado::SemCnpjTitular);
}

#[test]
fn chave_de_um_par_com_certificado_de_outro_e_recusada() {
    let dir_a = diretorio("par-a");
    let dir_b = diretorio("par-b");
    let opcoes = |nome| Opcoes {
        nome,
        cnpj_no_san: Some(CNPJ_TITULAR),
        dias: 3650,
        chave: Chave::Rsa,
    };
    let (_, cert_a) = gerar_par(&dir_a, &opcoes("A"));
    let (chave_b, _) = gerar_par(&dir_b, &opcoes("B"));
    let pfx = exportar_com_chave_trocada(&dir_a, &cert_a, &chave_b);
    let erro = CertificadoA1::abrir(&pfx, SENHA, &cnpj(CNPJ_TITULAR)).expect_err("par trocado");
    assert_eq!(erro, ErroCertificado::ChaveNaoCorresponde);
}

/// O OpenSSL recusa empacotar chave e certificado que não combinam; o writer do
/// `p12-keystore` não faz essa checagem e permite gerar o caso de teste.
fn exportar_com_chave_trocada(dir: &Path, cert: &Path, chave: &Path) -> Vec<u8> {
    let cert_der = dir.join("cert.der");
    let chave_der = dir.join("chave.der");
    openssl(
        &[
            "x509",
            "-in",
            cert.to_str().expect("utf-8"),
            "-outform",
            "DER",
            "-out",
            cert_der.to_str().expect("utf-8"),
        ],
        &[],
    );
    openssl(
        &[
            "pkcs8",
            "-topk8",
            "-nocrypt",
            "-in",
            chave.to_str().expect("utf-8"),
            "-outform",
            "DER",
            "-out",
            chave_der.to_str().expect("utf-8"),
        ],
        &[],
    );
    let certificado =
        p12_keystore::Certificate::from_der(&std::fs::read(cert_der).expect("lê cert"))
            .expect("certificado DER");
    let chave_privada =
        p12_keystore::PrivateKey::from_der(&std::fs::read(chave_der).expect("lê chave"))
            .expect("chave PKCS#8");
    let cadeia = p12_keystore::PrivateKeyChain::new(
        p12_keystore::LocalKeyId(vec![1, 2, 3, 4]),
        chave_privada,
        vec![certificado],
    );
    let mut armazem = p12_keystore::KeyStore::new();
    armazem.add_entry("a1", p12_keystore::KeyStoreEntry::PrivateKeyChain(cadeia));
    armazem.writer(SENHA).write().expect("escreve PKCS#12")
}

#[test]
fn cadeia_assinada_por_ca_inclui_a_ca_e_mantem_a_folha_primeiro() {
    let dir = diretorio("cadeia-ca");
    let ca_chave = dir.join("ca-chave.pem");
    let ca = dir.join("ca.pem");
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
            ca_chave.to_str().expect("utf-8"),
            "-out",
            ca.to_str().expect("utf-8"),
            "-subj",
            "/CN=AC SINTETICA TESTE",
            "-addext",
            "basicConstraints=critical,CA:TRUE",
            "-addext",
            "keyUsage=critical,keyCertSign,cRLSign",
        ],
        &[],
    );

    let chave = dir.join("folha-chave.pem");
    let csr = dir.join("folha.csr");
    let folha = dir.join("folha.pem");
    let extensoes = dir.join("folha.ext");
    std::fs::write(
        &extensoes,
        format!(
            "subjectAltName=otherName:2.16.76.1.3.3;UTF8:{CNPJ_TITULAR}\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,nonRepudiation\n"
        ),
    )
    .expect("grava extensões");
    openssl(
        &[
            "req",
            "-new",
            "-nodes",
            "-newkey",
            "rsa:2048",
            "-keyout",
            chave.to_str().expect("utf-8"),
            "-out",
            csr.to_str().expect("utf-8"),
            "-subj",
            "/CN=EMPRESA SINTETICA CADEIA",
        ],
        &[],
    );
    openssl(
        &[
            "x509",
            "-req",
            "-sha256",
            "-days",
            "3650",
            "-in",
            csr.to_str().expect("utf-8"),
            "-CA",
            ca.to_str().expect("utf-8"),
            "-CAkey",
            ca_chave.to_str().expect("utf-8"),
            "-CAcreateserial",
            "-extfile",
            extensoes.to_str().expect("utf-8"),
            "-out",
            folha.to_str().expect("utf-8"),
        ],
        &[],
    );

    let pfx = dir.join("cadeia.p12");
    openssl(
        &[
            "pkcs12",
            "-export",
            "-in",
            folha.to_str().expect("utf-8"),
            "-inkey",
            chave.to_str().expect("utf-8"),
            "-certfile",
            ca.to_str().expect("utf-8"),
            "-out",
            pfx.to_str().expect("utf-8"),
            "-passout",
            "env:CARIMBAR_SENHA_TESTE",
        ],
        &[("CARIMBAR_SENHA_TESTE", SENHA)],
    );
    let pfx = std::fs::read(pfx).expect("lê PKCS#12");

    let certificado = CertificadoA1::abrir(&pfx, SENHA, &cnpj(CNPJ_TITULAR)).expect("abre");
    assert_eq!(certificado.cadeia_der().len(), 2);
    assert_eq!(
        certificado.certificado_der(),
        der_do_pem(&dir, &folha).as_slice()
    );
    assert_eq!(certificado.cadeia_der()[1], der_do_pem(&dir, &ca));
}

fn der_do_pem(dir: &Path, pem: &Path) -> Vec<u8> {
    let saida = dir.join(format!(
        "{}.der",
        pem.file_stem().and_then(|s| s.to_str()).unwrap_or("cert")
    ));
    openssl(
        &[
            "x509",
            "-in",
            pem.to_str().expect("utf-8"),
            "-outform",
            "DER",
            "-out",
            saida.to_str().expect("utf-8"),
        ],
        &[],
    );
    std::fs::read(saida).expect("lê DER")
}

#[test]
fn chave_ec_e_recusada_com_o_algoritmo_informado() {
    let dir = diretorio("ec");
    let (chave, cert) = gerar_par(
        &dir,
        &Opcoes {
            nome: "EC",
            cnpj_no_san: Some(CNPJ_TITULAR),
            dias: 3650,
            chave: Chave::Ec,
        },
    );
    let pfx = exportar(&dir, &chave, &cert);
    let erro = CertificadoA1::abrir(&pfx, SENHA, &cnpj(CNPJ_TITULAR)).expect_err("EC");
    assert!(
        matches!(erro, ErroCertificado::ChaveNaoSuportada(_)),
        "{erro}"
    );
}

#[test]
fn assinatura_rsa_sha256_verifica_com_a_chave_publica_do_certificado() {
    let pfx = a1("assina", Some(CNPJ_TITULAR), 3650);
    let certificado = CertificadoA1::abrir(&pfx, SENHA, &cnpj(CNPJ_TITULAR)).expect("abre");
    let mensagem = b"<SignedInfo>sintetico</SignedInfo>";
    let assinatura = certificado.assinar_rsa_sha256(mensagem).expect("assina");

    let chave_publica = chave_publica_do_certificado(certificado.certificado_der());
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &chave_publica)
        .verify(mensagem, &assinatura)
        .expect("assinatura válida");
    assert!(
        UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &chave_publica)
            .verify(b"outra mensagem", &assinatura)
            .is_err()
    );
}

#[test]
fn identidade_tls_expoe_cadeia_e_chave_sem_copia_no_debug() {
    let pfx = a1("identidade", Some(CNPJ_TITULAR), 3650);
    let certificado = CertificadoA1::abrir(&pfx, SENHA, &cnpj(CNPJ_TITULAR)).expect("abre");
    let identidade = certificado.identidade_tls();
    assert_eq!(identidade.cadeia_der()[0], certificado.certificado_der());
    let chave = identidade.chave_pkcs8_der();
    assert!(!chave.is_empty());

    let debug = format!("{certificado:?} {identidade:?}");
    assert!(debug.contains("<oculta>"));
    assert!(!debug.contains(&hex(&chave[..16])));
    assert!(!debug.contains(SENHA));
}

#[test]
fn erros_nao_contem_senha_nem_chave() {
    let pfx = a1("erros", Some(CNPJ_TITULAR), 3650);
    let erro = CertificadoA1::abrir(&pfx, SENHA, &cnpj(CNPJ_OUTRO)).expect_err("titular");
    assert!(!erro.to_string().contains(SENHA));
    assert!(!format!("{erro:?}").contains(SENHA));
}

fn chave_publica_do_certificado(der: &[u8]) -> Vec<u8> {
    use x509_parser::prelude::FromDer;
    let (_, cert) = x509_parser::certificate::X509Certificate::from_der(der).expect("certificado");
    cert.public_key().subject_public_key.data.to_vec()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
