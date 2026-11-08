//! Testes do cofre com certificados sintéticos gerados pelo CLI `openssl`.
//!
//! Nenhum certificado, senha ou dado real é versionado. Os arquivos do cofre
//! ficam em `CARGO_TARGET_TMPDIR` e as conexões nunca saem da máquina.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use carimbar_types::Cnpj;
use carimbar_vault::{ChaveMestra, Cofre, CustoKdf, ErroCofre, EventoAuditoria, ProvedorKek};
use rusqlite::{Connection, params};
use zeroize::Zeroizing;

const SENHA: &str = "senha-sintetica-123";
const CNPJ_A: &str = "00000001000136";
const CNPJ_B: &str = "00000002000008";

// ---------------------------------------------------------------------------
// PKI sintética
// ---------------------------------------------------------------------------

fn caminho(caminho: &Path) -> &str {
    caminho.to_str().expect("caminho UTF-8")
}

fn openssl(args: &[&str], senhas: &[(&str, &str)]) {
    let saida = Command::new("openssl")
        .args(args)
        .envs(senhas.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("openssl disponível no PATH");
    assert!(
        saida.status.success(),
        "openssl {args:?} falhou: {}",
        String::from_utf8_lossy(&saida.stderr)
    );
}

fn diretorio(nome: &str) -> PathBuf {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let id = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("vault-{nome}-{id}"));
    // O mesmo caminho pode ter sobrado de uma execução anterior; começar limpo
    // mantém o teste repetível.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("cria diretório temporário");
    dir
}

fn banco(nome: &str) -> PathBuf {
    diretorio(nome).join("cofre.db")
}

/// PKCS#12 A1 sintético com o CNPJ no SAN ICP-Brasil.
fn pfx(nome: &str, cnpj: &str, senha: &str) -> Vec<u8> {
    let dir = diretorio(nome);
    let chave = dir.join("a1.key");
    let certificado = dir.join("a1.pem");
    let arquivo = dir.join("a1.p12");

    openssl(
        &[
            "genpkey",
            "-algorithm",
            "RSA",
            "-pkeyopt",
            "rsa_keygen_bits:2048",
            "-out",
            caminho(&chave),
        ],
        &[],
    );
    openssl(
        &[
            "req",
            "-x509",
            "-new",
            "-key",
            caminho(&chave),
            "-out",
            caminho(&certificado),
            "-days",
            "2",
            "-subj",
            &format!("/CN=EMPRESA SINTETICA {nome}"),
            "-addext",
            &format!("subjectAltName=otherName:2.16.76.1.3.3;UTF8:{cnpj}"),
        ],
        &[],
    );
    openssl(
        &[
            "pkcs12",
            "-export",
            "-in",
            caminho(&certificado),
            "-inkey",
            caminho(&chave),
            "-out",
            caminho(&arquivo),
            "-passout",
            "env:CARIMBAR_SENHA_TESTE",
            "-name",
            "a1",
        ],
        &[("CARIMBAR_SENHA_TESTE", senha)],
    );
    std::fs::read(arquivo).expect("lê o PKCS#12 sintético")
}

fn cnpj(texto: &str) -> Cnpj {
    Cnpj::parse(texto).expect("CNPJ sintético com DV válido")
}

fn chave(byte: u8) -> Box<dyn ProvedorKek> {
    Box::new(ChaveMestra::de_bytes([byte; 32]))
}

fn agora() -> SystemTime {
    SystemTime::now()
}

// ---------------------------------------------------------------------------
// Provedor com falha injetada
// ---------------------------------------------------------------------------

struct ProvedorInstavel {
    interno: ChaveMestra,
    falhar_depois: usize,
    chamadas: AtomicUsize,
}

impl ProvedorInstavel {
    fn novo(byte: u8, falhar_depois: usize) -> Self {
        Self {
            interno: ChaveMestra::de_bytes([byte; 32]),
            falhar_depois,
            chamadas: AtomicUsize::new(0),
        }
    }
}

impl ProvedorKek for ProvedorInstavel {
    fn selar(&self, texto: &[u8], aad: &[u8]) -> Result<Vec<u8>, ErroCofre> {
        let chamada = self.chamadas.fetch_add(1, Ordering::SeqCst);
        if chamada >= self.falhar_depois {
            return Err(ErroCofre::Cripto);
        }
        self.interno.selar(texto, aad)
    }

    fn abrir(&self, selado: &[u8], aad: &[u8]) -> Result<Zeroizing<Vec<u8>>, ErroCofre> {
        self.interno.abrir(selado, aad)
    }
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[test]
fn importa_lista_reabre_e_usa_certificado() {
    let caminho = banco("ciclo");
    let empresa = cnpj(CNPJ_A);
    let pfx = pfx("ciclo", CNPJ_A, SENHA);

    {
        let mut cofre = Cofre::criar(&caminho, chave(1)).expect("cria cofre");
        let meta = cofre
            .importar(&empresa, "a1-2026", &pfx, SENHA)
            .expect("importa");

        assert_eq!(meta.empresa, empresa);
        assert_eq!(meta.titular, empresa);
        assert!(meta.valido_ate > agora());
        assert!(meta.valido_de < agora());
        assert_eq!(meta.sha256.len(), 64);
        assert!(meta.sha256.bytes().all(|b| b.is_ascii_hexdigit()));

        let lista = cofre.listar(&empresa).expect("lista");
        assert_eq!(lista, vec![meta]);
        assert!(cofre.listar(&cnpj(CNPJ_B)).expect("lista").is_empty());
        cofre.verificar_auditoria().expect("auditoria inicial");
    }

    let cofre = Cofre::abrir(&caminho, chave(1)).expect("reabre");
    let certificado = cofre.certificado(&empresa, "a1-2026").expect("certificado");
    assert_eq!(certificado.cnpj(), &empresa);
    let assinatura = certificado
        .assinar_rsa_sha256(b"mensagem sintetica")
        .expect("assina");
    assert_eq!(assinatura.len(), 256);

    let eventos: Vec<EventoAuditoria> = cofre
        .auditoria()
        .expect("auditoria")
        .into_iter()
        .map(|registro| registro.evento)
        .collect();
    assert_eq!(
        eventos,
        vec![
            EventoAuditoria::CofreCriado,
            EventoAuditoria::EmpresaCriada,
            EventoAuditoria::CertificadoImportado,
        ]
    );
    cofre.verificar_auditoria().expect("cadeia íntegra");
}

#[test]
fn chave_mestra_errada_nao_abre() {
    let caminho = banco("chave-errada");
    let empresa = cnpj(CNPJ_A);
    let pfx = pfx("chave-errada", CNPJ_A, SENHA);

    {
        let mut cofre = Cofre::criar(&caminho, chave(1)).expect("cria");
        cofre
            .importar(&empresa, "a1", &pfx, SENHA)
            .expect("importa");
    }

    let erro = Cofre::abrir(&caminho, chave(2)).expect_err("KEK errada");
    assert!(matches!(erro, ErroCofre::ChaveIncorreta), "{erro}");
}

#[test]
fn senha_deriva_a_kek_e_senha_errada_nao_abre() {
    let caminho = banco("senha");
    let empresa = cnpj(CNPJ_A);
    let pfx = pfx("senha", CNPJ_A, SENHA);

    {
        let mut cofre =
            Cofre::criar_com_senha(&caminho, "senha-forte", CustoKdf::rapido()).expect("cria");
        cofre
            .importar(&empresa, "a1", &pfx, SENHA)
            .expect("importa");
    }

    let erro = Cofre::abrir_com_senha(&caminho, "senha-errada").expect_err("senha errada");
    assert!(matches!(erro, ErroCofre::ChaveIncorreta), "{erro}");

    let cofre = Cofre::abrir_com_senha(&caminho, "senha-forte").expect("reabre");
    assert_eq!(
        cofre.certificado(&empresa, "a1").expect("cert").cnpj(),
        &empresa
    );
}

#[test]
fn empresas_ficam_isoladas_mesmo_com_o_mesmo_id() {
    let caminho = banco("isolamento");
    let a = cnpj(CNPJ_A);
    let b = cnpj(CNPJ_B);
    let pfx_a = pfx("isolamento-a", CNPJ_A, SENHA);
    let pfx_b = pfx("isolamento-b", CNPJ_B, SENHA);

    let mut cofre = Cofre::criar(&caminho, chave(3)).expect("cria");
    cofre.importar(&a, "a1", &pfx_a, SENHA).expect("importa A");
    cofre.importar(&b, "a1", &pfx_b, SENHA).expect("importa B");

    assert_eq!(cofre.certificado(&a, "a1").expect("A").cnpj(), &a);
    assert_eq!(cofre.certificado(&b, "a1").expect("B").cnpj(), &b);
    assert_eq!(cofre.listar(&a).expect("lista A").len(), 1);
    assert_eq!(cofre.listar(&b).expect("lista B").len(), 1);

    cofre.remover(&a, "a1").expect("remove A");
    assert!(matches!(
        cofre.certificado(&a, "a1"),
        Err(ErroCofre::CertificadoNaoEncontrado)
    ));
    assert!(cofre.listar(&a).expect("lista A").is_empty());
    assert_eq!(cofre.certificado(&b, "a1").expect("B").cnpj(), &b);
    cofre.verificar_auditoria().expect("cadeia íntegra");
}

#[test]
fn certificado_de_outra_empresa_nao_e_importado() {
    let caminho = banco("titular");
    let b = cnpj(CNPJ_B);
    let pfx_a = pfx("titular", CNPJ_A, SENHA);

    let mut cofre = Cofre::criar(&caminho, chave(4)).expect("cria");
    let erro = cofre
        .importar(&b, "a1", &pfx_a, SENHA)
        .expect_err("titular diferente");
    assert!(matches!(erro, ErroCofre::Certificado(_)), "{erro}");
    assert!(cofre.listar(&b).expect("lista b").is_empty());

    // A validação acontece antes da transação: nenhuma empresa é criada.
    let empresas: i64 = {
        let conexao = Connection::open(&caminho).expect("abre sqlite");
        conexao
            .query_row("SELECT COUNT(*) FROM empresas", [], |row| row.get(0))
            .expect("conta empresas")
    };
    assert_eq!(empresas, 0);

    let eventos = cofre.auditoria().expect("auditoria");
    assert_eq!(eventos.len(), 1);
    assert_eq!(eventos[0].evento, EventoAuditoria::CofreCriado);
}

#[test]
fn senha_errada_do_pfx_nao_grava_nada() {
    let caminho = banco("pfx-senha");
    let empresa = cnpj(CNPJ_A);
    let pfx = pfx("pfx-senha", CNPJ_A, SENHA);

    let mut cofre = Cofre::criar(&caminho, chave(5)).expect("cria");
    let erro = cofre
        .importar(&empresa, "a1", &pfx, "outra-senha")
        .expect_err("senha do PFX errada");
    assert!(!erro.to_string().contains("outra-senha"));
    assert!(cofre.listar(&empresa).expect("lista").is_empty());
}

#[test]
fn id_fora_do_formato_e_recusado() {
    let caminho = banco("id");
    let empresa = cnpj(CNPJ_A);
    let pfx = pfx("id", CNPJ_A, SENHA);
    let mut cofre = Cofre::criar(&caminho, chave(6)).expect("cria");

    let longo = "a".repeat(65);
    for id in ["", "com espaço", "com/barra", "com:dois", longo.as_str()] {
        let erro = cofre
            .importar(&empresa, id, &pfx, SENHA)
            .expect_err("id inválido");
        assert!(matches!(erro, ErroCofre::Parametro(_)), "{erro}");
    }
    assert!(cofre.listar(&empresa).expect("lista").is_empty());
}

#[test]
fn criar_duas_vezes_nao_sobrescreve() {
    let caminho = banco("duplo");
    let _cofre = Cofre::criar(&caminho, chave(7)).expect("cria");
    drop(_cofre);
    let erro = Cofre::criar(&caminho, chave(7)).expect_err("já existe");
    assert!(matches!(erro, ErroCofre::CofreJaExiste), "{erro}");
}

#[test]
fn cofre_aberto_sem_arquivo_falha() {
    let caminho = diretorio("inexistente").join("nao-existe.db");
    assert!(Cofre::abrir(&caminho, chave(8)).is_err());
}

#[test]
fn remover_inexistente_e_erro() {
    let caminho = banco("remover");
    let empresa = cnpj(CNPJ_A);
    let mut cofre = Cofre::criar(&caminho, chave(9)).expect("cria");
    let erro = cofre.remover(&empresa, "a1").expect_err("não existe");
    assert!(
        matches!(erro, ErroCofre::CertificadoNaoEncontrado),
        "{erro}"
    );
}

#[test]
fn segredo_nao_e_gravado_em_claro_no_arquivo() {
    let caminho = banco("repouso");
    let empresa = cnpj(CNPJ_A);
    let senha = "senha-muito-secreta-987";
    let pfx = pfx("repouso", CNPJ_A, senha);

    {
        let mut cofre = Cofre::criar(&caminho, chave(10)).expect("cria");
        cofre
            .importar(&empresa, "a1", &pfx, senha)
            .expect("importa");
    }

    // O cofre usa WAL: força o checkpoint para que o conteúdo esteja no
    // arquivo principal antes de procurar segredos em claro.
    {
        let conexao = Connection::open(&caminho).expect("abre sqlite");
        conexao
            .query_row("PRAGMA wal_checkpoint(FULL)", [], |_| Ok(()))
            .expect("checkpoint do WAL");
    }

    let arquivo = std::fs::read(&caminho).expect("lê arquivo do cofre");
    assert!(
        !arquivo
            .windows(pfx.len())
            .any(|janela| janela == pfx.as_slice()),
        "o PFX apareceu em claro no arquivo"
    );
    assert!(
        !arquivo
            .windows(senha.len())
            .any(|janela| janela == senha.as_bytes()),
        "a senha apareceu em claro no arquivo"
    );
}

#[test]
fn adulteracao_do_registro_cifrado_e_detectada() {
    let caminho = banco("adulteracao");
    let empresa = cnpj(CNPJ_A);
    let pfx = pfx("adulteracao", CNPJ_A, SENHA);

    {
        let mut cofre = Cofre::criar(&caminho, chave(11)).expect("cria");
        cofre
            .importar(&empresa, "a1", &pfx, SENHA)
            .expect("importa");
    }

    {
        let conexao = Connection::open(&caminho).expect("abre sqlite");
        conexao
            .execute(
                "UPDATE certificados SET pfx_selado = ?1 WHERE empresa = ?2 AND id = 'a1'",
                params![vec![0_u8; 64], CNPJ_A],
            )
            .expect("adultera");
    }

    let cofre = Cofre::abrir(&caminho, chave(11)).expect("reabre");
    let erro = cofre
        .certificado(&empresa, "a1")
        .expect_err("registro adulterado");
    assert!(matches!(erro, ErroCofre::RegistroAdulterado), "{erro}");
}

#[test]
fn auditoria_detecta_alteracao_de_registro() {
    let caminho = banco("auditoria-alterada");
    let empresa = cnpj(CNPJ_A);
    let pfx = pfx("auditoria-alterada", CNPJ_A, SENHA);

    {
        let mut cofre = Cofre::criar(&caminho, chave(12)).expect("cria");
        cofre
            .importar(&empresa, "a1", &pfx, SENHA)
            .expect("importa");
        cofre.verificar_auditoria().expect("íntegra");
    }

    {
        let conexao = Connection::open(&caminho).expect("abre sqlite");
        conexao
            .execute(
                "UPDATE auditoria SET detalhe = detalhe || 'x' WHERE seq = 3",
                [],
            )
            .expect("adultera auditoria");
    }

    let cofre = Cofre::abrir(&caminho, chave(12)).expect("reabre");
    let erro = cofre.verificar_auditoria().expect_err("adulterada");
    assert!(matches!(erro, ErroCofre::Auditoria(3)), "{erro}");
}

#[test]
fn auditoria_detecta_remocao_intermediaria() {
    let caminho = banco("auditoria-removida");
    let empresa = cnpj(CNPJ_A);
    let pfx = pfx("auditoria-removida", CNPJ_A, SENHA);

    {
        let mut cofre = Cofre::criar(&caminho, chave(13)).expect("cria");
        cofre
            .importar(&empresa, "a1", &pfx, SENHA)
            .expect("importa");
    }

    {
        let conexao = Connection::open(&caminho).expect("abre sqlite");
        conexao
            .execute("DELETE FROM auditoria WHERE seq = 2", [])
            .expect("remove registro");
    }

    let cofre = Cofre::abrir(&caminho, chave(13)).expect("reabre");
    let erro = cofre.verificar_auditoria().expect_err("cadeia quebrada");
    assert!(matches!(erro, ErroCofre::Auditoria(3)), "{erro}");
    assert_eq!(cofre.auditoria().expect("lista").len(), 2);
}

#[test]
fn rotacao_de_dek_recifra_todos_os_certificados() {
    let caminho = banco("rotacao-dek");
    let empresa = cnpj(CNPJ_A);
    let pfx = pfx("rotacao-dek", CNPJ_A, SENHA);

    let mut cofre = Cofre::criar(&caminho, chave(14)).expect("cria");
    cofre.importar(&empresa, "a1-1", &pfx, SENHA).expect("1");
    cofre.importar(&empresa, "a1-2", &pfx, SENHA).expect("2");

    assert_eq!(cofre.rotacionar_dek(&empresa).expect("rotaciona"), 2);
    assert_eq!(
        cofre.certificado(&empresa, "a1-1").expect("1").cnpj(),
        &empresa
    );
    assert_eq!(
        cofre.certificado(&empresa, "a1-2").expect("2").cnpj(),
        &empresa
    );

    let versao: i64 = {
        let conexao = Connection::open(&caminho).expect("abre sqlite");
        conexao
            .query_row(
                "SELECT dek_versao FROM empresas WHERE cnpj = ?1",
                [CNPJ_A],
                |row| row.get(0),
            )
            .expect("versão")
    };
    assert_eq!(versao, 2);

    cofre.verificar_auditoria().expect("cadeia íntegra");
    let eventos = cofre.auditoria().expect("auditoria");
    assert_eq!(
        eventos.last().map(|registro| registro.evento),
        Some(EventoAuditoria::DekRotacionada)
    );
}

#[test]
fn rotacao_de_dek_exige_empresa_existente() {
    let caminho = banco("rotacao-sem-empresa");
    let mut cofre = Cofre::criar(&caminho, chave(15)).expect("cria");
    let erro = cofre
        .rotacionar_dek(&cnpj(CNPJ_A))
        .expect_err("empresa inexistente");
    assert!(matches!(erro, ErroCofre::EmpresaNaoEncontrada), "{erro}");
}

#[test]
fn rotacao_de_kek_troca_a_chave_sem_perder_certificado() {
    let caminho = banco("rotacao-kek");
    let empresa = cnpj(CNPJ_A);
    let pfx = pfx("rotacao-kek", CNPJ_A, SENHA);

    {
        let mut cofre = Cofre::criar(&caminho, chave(16)).expect("cria");
        cofre
            .importar(&empresa, "a1", &pfx, SENHA)
            .expect("importa");
        cofre.rotacionar_kek(chave(17)).expect("rotaciona KEK");
    }

    let erro = Cofre::abrir(&caminho, chave(16)).expect_err("KEK antiga");
    assert!(matches!(erro, ErroCofre::ChaveIncorreta), "{erro}");

    let cofre = Cofre::abrir(&caminho, chave(17)).expect("KEK nova");
    assert_eq!(
        cofre.certificado(&empresa, "a1").expect("cert").cnpj(),
        &empresa
    );
    cofre.verificar_auditoria().expect("cadeia íntegra");
}

#[test]
fn falha_de_escrita_nao_deixa_estado_parcial() {
    let caminho = banco("falha-escrita");
    let empresa = cnpj(CNPJ_A);
    let pfx = pfx("falha-escrita", CNPJ_A, SENHA);

    {
        let mut cofre = Cofre::criar(&caminho, chave(18)).expect("cria");
        cofre
            .importar(&empresa, "a1", &pfx, SENHA)
            .expect("importa");

        // O primeiro selo (verificador) passa; o segundo (DEK da empresa) falha.
        let instavel = ProvedorInstavel::novo(19, 1);
        let erro = cofre
            .rotacionar_kek(Box::new(instavel))
            .expect_err("falha injetada");
        assert!(matches!(erro, ErroCofre::Cripto), "{erro}");

        // A KEK original continua valendo e o certificado continua legível.
        assert_eq!(
            cofre.certificado(&empresa, "a1").expect("cert").cnpj(),
            &empresa
        );
        cofre.verificar_auditoria().expect("cadeia íntegra");
    }

    assert!(Cofre::abrir(&caminho, chave(19)).is_err());
    let cofre = Cofre::abrir(&caminho, chave(18)).expect("reabre com a KEK antiga");
    assert_eq!(
        cofre.certificado(&empresa, "a1").expect("cert").cnpj(),
        &empresa
    );
}

#[test]
fn erro_e_debug_nao_vazam_segredo() {
    let caminho = banco("vazamento");
    let empresa = cnpj(CNPJ_A);
    let senha = "segredo-que-nao-pode-vazar";
    let pfx = pfx("vazamento", CNPJ_A, senha);

    let mut cofre = Cofre::criar(&caminho, chave(20)).expect("cria");
    let erro = cofre
        .importar(&empresa, "a1", &pfx, "senha-errada-xyz")
        .expect_err("senha errada");

    let texto = format!("{erro} {erro:?}");
    assert!(!texto.contains("senha-errada-xyz"));
    assert!(!texto.contains(senha));

    let debug = format!("{cofre:?}");
    assert!(debug.contains("oculto"));
    assert!(!debug.contains("ChaveMestra"));

    let meta = cofre
        .importar(&empresa, "a1", &pfx, senha)
        .expect("importa");
    let meta_debug = format!("{meta:?}");
    assert!(!meta_debug.contains(senha));
    assert!(!meta_debug.contains("PRIVATE KEY"));
}

#[test]
fn metadados_de_certificado_nao_expoem_chave() {
    let caminho = banco("metadados");
    let empresa = cnpj(CNPJ_A);
    let pfx = pfx("metadados", CNPJ_A, SENHA);

    let mut cofre = Cofre::criar(&caminho, chave(21)).expect("cria");
    cofre
        .importar(&empresa, "a1", &pfx, SENHA)
        .expect("importa");
    let meta = &cofre.listar(&empresa).expect("lista")[0];

    assert_eq!(meta.titular, empresa);
    assert_eq!(meta.empresa, empresa);
    assert_eq!(meta.id, "a1");
    assert!(meta.valido_ate > agora());
    assert_eq!(meta.sha256.len(), 64);
    assert!(meta.importado_em.duration_since(UNIX_EPOCH).is_ok());
}
