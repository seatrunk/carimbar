//! Testes de integração do transporte mTLS.
//!
//! Toda a PKI (duas CAs, servidor e clientes) é sintética e gerada em tempo de
//! teste pelo CLI `openssl`. Nenhum certificado, chave ou senha é versionado, e
//! nenhuma conexão sai da máquina local.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::Duration;

use carimbar::transport::{
    ClienteMtls, ErroEnvio, ErroTransporte, Identidade, Metodo, Opcoes, Requisicao, Resposta,
    envelope_soap12,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig, ServerConnection, StreamOwned};

const ACAO_STATUS: &str =
    "http://www.portalfiscal.inf.br/nfe/wsdl/NFeStatusServico4/nfeStatusServicoNF";

// ---------------------------------------------------------------------------
// PKI sintética
// ---------------------------------------------------------------------------

struct Pki {
    ca_confiavel: Vec<u8>,
    ca_desconhecida: Vec<u8>,
    servidor_certificado: Vec<u8>,
    servidor_chave: Vec<u8>,
    cliente_certificado: Vec<u8>,
    cliente_chave: Vec<u8>,
    intruso_certificado: Vec<u8>,
    intruso_chave: Vec<u8>,
}

fn pki() -> &'static Pki {
    static PKI: OnceLock<Pki> = OnceLock::new();
    PKI.get_or_init(gerar_pki)
}

fn caminho(caminho: &Path) -> &str {
    caminho.to_str().expect("caminho UTF-8")
}

fn openssl(args: &[&str]) {
    let saida = Command::new("openssl")
        .args(args)
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

fn criar_ca(dir: &Path, nome: &str, cn: &str) -> (PathBuf, PathBuf) {
    let chave = dir.join(format!("{nome}.key"));
    let certificado = dir.join(format!("{nome}.pem"));
    openssl(&[
        "genpkey",
        "-algorithm",
        "RSA",
        "-pkeyopt",
        "rsa_keygen_bits:2048",
        "-out",
        caminho(&chave),
    ]);
    openssl(&[
        "req",
        "-x509",
        "-new",
        "-key",
        caminho(&chave),
        "-out",
        caminho(&certificado),
        "-days",
        "2",
        "-sha256",
        "-subj",
        &format!("/CN={cn}"),
        "-addext",
        "basicConstraints=critical,CA:TRUE",
        "-addext",
        "keyUsage=critical,keyCertSign,cRLSign",
    ]);
    (chave, certificado)
}

fn criar_folha(
    dir: &Path,
    nome: &str,
    ca_certificado: &Path,
    ca_chave: &Path,
    cn: &str,
    extensoes: &str,
) -> (PathBuf, PathBuf) {
    let chave = dir.join(format!("{nome}.key"));
    let pedido = dir.join(format!("{nome}.csr"));
    let certificado = dir.join(format!("{nome}.pem"));
    let ext = dir.join(format!("{nome}.ext"));
    std::fs::write(&ext, extensoes).expect("escreve extensões");

    openssl(&[
        "genpkey",
        "-algorithm",
        "RSA",
        "-pkeyopt",
        "rsa_keygen_bits:2048",
        "-out",
        caminho(&chave),
    ]);
    openssl(&[
        "req",
        "-new",
        "-key",
        caminho(&chave),
        "-out",
        caminho(&pedido),
        "-subj",
        &format!("/CN={cn}"),
    ]);
    openssl(&[
        "x509",
        "-req",
        "-in",
        caminho(&pedido),
        "-CA",
        caminho(ca_certificado),
        "-CAkey",
        caminho(ca_chave),
        "-CAcreateserial",
        "-days",
        "2",
        "-sha256",
        "-extfile",
        caminho(&ext),
        "-out",
        caminho(&certificado),
    ]);
    (chave, certificado)
}

fn exportar_certificado(certificado: &Path) -> Vec<u8> {
    let der = certificado.with_extension("cert.der");
    openssl(&[
        "x509",
        "-in",
        caminho(certificado),
        "-outform",
        "der",
        "-out",
        caminho(&der),
    ]);
    std::fs::read(der).expect("lê certificado DER")
}

fn exportar_chave(chave: &Path) -> Vec<u8> {
    let der = chave.with_extension("key.der");
    openssl(&[
        "pkcs8",
        "-topk8",
        "-nocrypt",
        "-in",
        caminho(chave),
        "-outform",
        "der",
        "-out",
        caminho(&der),
    ]);
    std::fs::read(der).expect("lê chave DER")
}

fn gerar_pki() -> Pki {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("transporte-pki");
    std::fs::create_dir_all(&dir).expect("cria diretório da PKI");

    let (ca_chave, ca_certificado) = criar_ca(&dir, "ca-confiavel", "CA Sintetica Confiavel");
    let (ca2_chave, ca2_certificado) = criar_ca(&dir, "ca-desconhecida", "CA Sintetica Estranha");

    let (servidor_chave, servidor_certificado) = criar_folha(
        &dir,
        "servidor",
        &ca_certificado,
        &ca_chave,
        "localhost",
        concat!(
            "basicConstraints=critical,CA:FALSE\n",
            "keyUsage=critical,digitalSignature,keyEncipherment\n",
            "extendedKeyUsage=serverAuth\n",
            "subjectAltName=IP:127.0.0.1,DNS:localhost\n"
        ),
    );
    let extensoes_cliente = concat!(
        "basicConstraints=critical,CA:FALSE\n",
        "keyUsage=critical,digitalSignature,keyEncipherment\n",
        "extendedKeyUsage=clientAuth\n"
    );
    let (cliente_chave, cliente_certificado) = criar_folha(
        &dir,
        "cliente",
        &ca_certificado,
        &ca_chave,
        "EMPRESA SINTETICA CLIENTE",
        extensoes_cliente,
    );
    let (intruso_chave, intruso_certificado) = criar_folha(
        &dir,
        "intruso",
        &ca2_certificado,
        &ca2_chave,
        "EMPRESA SINTETICA INTRUSA",
        extensoes_cliente,
    );

    Pki {
        ca_confiavel: exportar_certificado(&ca_certificado),
        ca_desconhecida: exportar_certificado(&ca2_certificado),
        servidor_certificado: exportar_certificado(&servidor_certificado),
        servidor_chave: exportar_chave(&servidor_chave),
        cliente_certificado: exportar_certificado(&cliente_certificado),
        cliente_chave: exportar_chave(&cliente_chave),
        intruso_certificado: exportar_certificado(&intruso_certificado),
        intruso_chave: exportar_chave(&intruso_chave),
    }
}

// ---------------------------------------------------------------------------
// Servidor TLS de teste
// ---------------------------------------------------------------------------

enum Comportamento {
    Responder(Vec<u8>),
    Silencio,
}

struct Servidor {
    porta: u16,
    pedidos: Arc<Mutex<Vec<Vec<u8>>>>,
    parar: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Servidor {
    fn iniciar(exigir_cliente: bool, comportamentos: Vec<Comportamento>) -> Self {
        let config = Arc::new(configuracao_servidor(exigir_cliente));
        let ouvinte = TcpListener::bind(("127.0.0.1", 0)).expect("porta livre");
        ouvinte.set_nonblocking(true).expect("modo não bloqueante");
        let porta = ouvinte.local_addr().expect("endereço local").port();

        let pedidos = Arc::new(Mutex::new(Vec::new()));
        let parar = Arc::new(AtomicBool::new(false));
        let fila = Arc::new(Mutex::new(VecDeque::from(comportamentos)));

        let thread = {
            let pedidos = Arc::clone(&pedidos);
            let parar = Arc::clone(&parar);
            std::thread::spawn(move || {
                while !parar.load(Ordering::Relaxed) {
                    match ouvinte.accept() {
                        Ok((tcp, _)) => {
                            let config = Arc::clone(&config);
                            let pedidos = Arc::clone(&pedidos);
                            let fila = Arc::clone(&fila);
                            std::thread::spawn(move || atender(config, tcp, pedidos, fila));
                        }
                        Err(erro) if erro.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => break,
                    }
                }
            })
        };

        Self {
            porta,
            pedidos,
            parar,
            thread: Some(thread),
        }
    }

    fn url(&self, caminho: &str) -> String {
        format!("https://127.0.0.1:{}{caminho}", self.porta)
    }

    fn pedidos(&self) -> Vec<Vec<u8>> {
        self.pedidos.lock().expect("pedidos").clone()
    }

    fn total(&self) -> usize {
        self.pedidos.lock().expect("pedidos").len()
    }
}

impl Drop for Servidor {
    fn drop(&mut self) {
        self.parar.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn configuracao_servidor(exigir_cliente: bool) -> ServerConfig {
    let pki = pki();
    let cadeia = vec![CertificateDer::from(pki.servidor_certificado.clone())];
    let chave = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pki.servidor_chave.clone()));

    let construtor =
        ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .expect("protocolos TLS");

    let construtor = if exigir_cliente {
        let mut raizes = RootCertStore::empty();
        raizes
            .add(CertificateDer::from(pki.ca_confiavel.clone()))
            .expect("CA do cliente");
        let verificador = WebPkiClientVerifier::builder_with_provider(
            Arc::new(raizes),
            Arc::new(rustls::crypto::ring::default_provider()),
        )
        .build()
        .expect("verificador de cliente");
        construtor.with_client_cert_verifier(verificador)
    } else {
        construtor.with_no_client_auth()
    };

    construtor
        .with_single_cert(cadeia, chave)
        .expect("certificado do servidor")
}

fn atender(
    config: Arc<ServerConfig>,
    tcp: TcpStream,
    pedidos: Arc<Mutex<Vec<Vec<u8>>>>,
    fila: Arc<Mutex<VecDeque<Comportamento>>>,
) {
    let conexao = match ServerConnection::new(config) {
        Ok(conexao) => conexao,
        Err(_) => return,
    };
    let mut fluxo = StreamOwned::new(conexao, tcp);
    let pedido = match ler_pedido(&mut fluxo) {
        Ok(pedido) => pedido,
        Err(_) => return,
    };
    pedidos.lock().expect("pedidos").push(pedido);

    let comportamento = fila.lock().expect("fila").pop_front().unwrap_or_else(|| {
        Comportamento::Responder(resposta("200 OK", &[("Content-Type", "text/plain")], b"ok"))
    });

    match comportamento {
        Comportamento::Responder(bytes) => {
            let _ = fluxo.write_all(&bytes);
            let _ = fluxo.flush();
        }
        Comportamento::Silencio => std::thread::sleep(Duration::from_millis(1500)),
    }

    fluxo.conn.send_close_notify();
    let _ = fluxo.sock.shutdown(Shutdown::Both);
}

fn ler_pedido(fluxo: &mut StreamOwned<ServerConnection, TcpStream>) -> std::io::Result<Vec<u8>> {
    fluxo.sock.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut dados = Vec::new();
    let mut temp = [0_u8; 4096];
    loop {
        if let Some(fim) = posicao(&dados, b"\r\n\r\n") {
            let tamanho = tamanho_do_corpo(&dados[..fim]);
            if dados.len() >= fim + 4 + tamanho {
                return Ok(dados);
            }
        }
        let lidos = fluxo.read(&mut temp)?;
        if lidos == 0 {
            return Ok(dados);
        }
        dados.extend_from_slice(&temp[..lidos]);
    }
}

fn posicao(dados: &[u8], agulha: &[u8]) -> Option<usize> {
    dados
        .windows(agulha.len())
        .position(|janela| janela == agulha)
}

fn tamanho_do_corpo(cabecalho: &[u8]) -> usize {
    let texto = String::from_utf8_lossy(cabecalho);
    for linha in texto.split("\r\n") {
        if let Some((nome, valor)) = linha.split_once(':')
            && nome.trim().eq_ignore_ascii_case("content-length")
        {
            return valor.trim().parse().unwrap_or(0);
        }
    }
    0
}

fn resposta(status: &str, cabecalhos: &[(&str, &str)], corpo: &[u8]) -> Vec<u8> {
    let mut texto = format!("HTTP/1.1 {status}\r\n");
    for (nome, valor) in cabecalhos {
        texto.push_str(&format!("{nome}: {valor}\r\n"));
    }
    texto.push_str(&format!("Content-Length: {}\r\n", corpo.len()));
    texto.push_str("Connection: close\r\n\r\n");
    let mut bytes = texto.into_bytes();
    bytes.extend_from_slice(corpo);
    bytes
}

// ---------------------------------------------------------------------------
// Clientes de teste
// ---------------------------------------------------------------------------

fn opcoes() -> Opcoes {
    Opcoes {
        timeout: Duration::from_secs(5),
        limite_resposta: 1024 * 1024,
        tentativas: 1,
        espera_entre_tentativas: Duration::ZERO,
    }
}

fn identidade(certificado: &[u8], chave: &[u8]) -> Identidade {
    Identidade::de_der(vec![certificado.to_vec()], chave.to_vec()).expect("identidade")
}

fn cliente(confiavel: bool) -> ClienteMtls {
    cliente_com(if confiavel {
        pki().ca_confiavel.clone()
    } else {
        pki().ca_desconhecida.clone()
    })
}

fn cliente_com(raiz: Vec<u8>) -> ClienteMtls {
    let pki = pki();
    ClienteMtls::novo(
        identidade(&pki.cliente_certificado, &pki.cliente_chave),
        &[raiz],
        opcoes(),
    )
    .expect("cliente mTLS")
}

fn cliente_com_opcoes(opcoes: Opcoes) -> ClienteMtls {
    let pki = pki();
    ClienteMtls::novo(
        identidade(&pki.cliente_certificado, &pki.cliente_chave),
        std::slice::from_ref(&pki.ca_confiavel),
        opcoes,
    )
    .expect("cliente mTLS")
}

fn porta_livre() -> u16 {
    let ouvinte = TcpListener::bind(("127.0.0.1", 0)).expect("porta livre");
    ouvinte.local_addr().expect("endereço").port()
}

// ---------------------------------------------------------------------------
// Testes
// ---------------------------------------------------------------------------

#[test]
fn mtls_envia_soap_12_e_devolve_resposta() {
    let servidor = Servidor::iniciar(
        true,
        vec![Comportamento::Responder(resposta(
            "200 OK",
            &[("Content-Type", "application/soap+xml; charset=utf-8")],
            b"<soap12:Envelope><soap12:Body><retorno/></soap12:Body></soap12:Envelope>",
        ))],
    );
    let requisicao = Requisicao::post_soap12(
        servidor.url("/ws/nfeStatusServico4"),
        ACAO_STATUS,
        "<nfeDadosMsg/>",
    )
    .expect("requisição SOAP");

    let resposta = cliente(true).executar(&requisicao).expect("resposta");

    assert_eq!(resposta.status, 200);
    assert!(resposta.sucesso());
    assert_eq!(
        resposta.cabecalho("Content-Type"),
        Some("application/soap+xml; charset=utf-8")
    );
    assert!(resposta.texto().expect("utf-8").contains("<retorno/>"));
    assert_eq!(resposta.exigir_sucesso(), Ok(()));

    let pedidos = servidor.pedidos();
    assert_eq!(pedidos.len(), 1);
    let texto = String::from_utf8_lossy(&pedidos[0]);
    assert!(
        texto.starts_with("POST /ws/nfeStatusServico4 HTTP/1.1\r\n"),
        "{texto}"
    );
    assert!(texto.contains(&format!("Host: 127.0.0.1:{}", servidor.porta)));
    assert!(texto.contains(&format!(
        "Content-Type: application/soap+xml; charset=utf-8; action=\"{ACAO_STATUS}\""
    )));
    assert!(texto.contains("Accept: application/soap+xml"));
    assert!(texto.contains("Connection: close"));
    assert!(texto.contains("User-Agent: carimbar-transport/"));
    assert!(texto.contains(&format!(
        "Content-Length: {}",
        envelope_soap12("<nfeDadosMsg/>").len()
    )));
    assert!(texto.contains("<soap12:Envelope"));
    assert!(texto.contains("<soap12:Body><nfeDadosMsg/></soap12:Body>"));
}

#[test]
fn servidor_de_ca_desconhecida_e_recusado_antes_do_envio() {
    let servidor = Servidor::iniciar(true, vec![]);
    let erro = cliente(false)
        .executar(&Requisicao::get(servidor.url("/")))
        .expect_err("CA desconhecida");

    match erro {
        ErroEnvio::AntesDoEnvio(ErroTransporte::Tls(_)) => {}
        outro => panic!("esperava falha de TLS antes do envio, veio {outro:?}"),
    }
    assert_eq!(servidor.total(), 0);
}

#[test]
fn identidade_de_cliente_de_outra_ca_e_recusada() {
    let servidor = Servidor::iniciar(true, vec![]);
    let pki = pki();
    let intruso = identidade(&pki.intruso_certificado, &pki.intruso_chave);
    let cliente = ClienteMtls::novo(intruso, std::slice::from_ref(&pki.ca_confiavel), opcoes())
        .expect("cliente mTLS");

    let erro = cliente
        .executar(&Requisicao::get(servidor.url("/")))
        .expect_err("identidade intrusa");

    // Em TLS 1.3 a recusa do certificado de cliente pode chegar depois do
    // Finished do cliente; o erro continua sendo de TLS e nada foi processado.
    let transporte = match erro {
        ErroEnvio::AntesDoEnvio(erro) | ErroEnvio::ResultadoDesconhecido(erro) => erro,
        outro => panic!("esperava falha de TLS, veio {outro:?}"),
    };
    assert!(
        matches!(transporte, ErroTransporte::Tls(_)),
        "{transporte:?}"
    );
    assert_eq!(servidor.total(), 0);
}

#[test]
fn url_sem_https_e_recusada_sem_conexao() {
    let erro = cliente(true)
        .executar(&Requisicao::get("http://127.0.0.1/"))
        .expect_err("HTTP sem TLS");
    assert!(matches!(
        erro,
        ErroEnvio::AntesDoEnvio(ErroTransporte::Url(_))
    ));
    assert!(erro.seguro_para_reenviar());
}

#[test]
fn conexao_recusada_e_antes_do_envio() {
    let porta = porta_livre();
    let erro = cliente(true)
        .executar(&Requisicao::get(format!("https://127.0.0.1:{porta}/")))
        .expect_err("conexão recusada");
    assert!(matches!(
        erro,
        ErroEnvio::AntesDoEnvio(ErroTransporte::Conexao { .. })
    ));
    assert!(erro.seguro_para_reenviar());
}

#[test]
fn timeout_de_leitura_e_resultado_desconhecido() {
    let servidor = Servidor::iniciar(true, vec![Comportamento::Silencio]);
    let cliente = cliente_com_opcoes(Opcoes {
        timeout: Duration::from_millis(400),
        ..opcoes()
    });

    let erro = cliente
        .executar(&Requisicao::get(servidor.url("/consulta")))
        .expect_err("sem resposta");

    assert_eq!(
        erro,
        ErroEnvio::ResultadoDesconhecido(ErroTransporte::Timeout("leitura"))
    );
    assert!(!erro.seguro_para_reenviar());
}

#[test]
fn executar_nao_repete_envio_de_emissao() {
    let servidor = Servidor::iniciar(
        true,
        vec![Comportamento::Responder(resposta(
            "500 Internal Server Error",
            &[("Content-Type", "text/plain")],
            b"falha",
        ))],
    );

    let resposta = cliente(true)
        .executar(&Requisicao::get(servidor.url("/emitir")))
        .expect("resposta HTTP");

    assert_eq!(resposta.status, 500);
    assert_eq!(resposta.texto().expect("utf-8"), "falha");
    assert_eq!(
        resposta.exigir_sucesso(),
        Err(ErroTransporte::Status { status: 500 })
    );
    assert_eq!(servidor.total(), 1);
}

#[test]
fn executar_seguro_repete_erro_transitorio() {
    let servidor = Servidor::iniciar(
        true,
        vec![
            Comportamento::Responder(resposta("503 Service Unavailable", &[], b"ocupado")),
            Comportamento::Responder(resposta(
                "200 OK",
                &[("Content-Type", "application/json")],
                br#"{"ok":true}"#,
            )),
        ],
    );
    let cliente = cliente_com_opcoes(Opcoes {
        tentativas: 3,
        espera_entre_tentativas: Duration::from_millis(10),
        ..opcoes()
    });

    let resposta = cliente
        .executar_seguro(&Requisicao::get_json(servidor.url("/consulta")))
        .expect("resposta");

    assert_eq!(resposta.status, 200);
    assert_eq!(resposta.texto().expect("utf-8"), r#"{"ok":true}"#);
    assert_eq!(servidor.total(), 2);
    let pedido = String::from_utf8_lossy(&servidor.pedidos()[0]).to_string();
    assert!(pedido.starts_with("GET /consulta HTTP/1.1\r\n"));
    assert!(pedido.contains("Accept: application/json"));
    assert!(!pedido.contains("Content-Length:"));
    assert!(!pedido.contains("Content-Type:"));
}

#[test]
fn executar_seguro_respeita_o_limite_de_tentativas() {
    let servidor = Servidor::iniciar(
        true,
        vec![
            Comportamento::Responder(resposta("500 Internal Server Error", &[], b"1")),
            Comportamento::Responder(resposta("500 Internal Server Error", &[], b"2")),
            Comportamento::Responder(resposta("500 Internal Server Error", &[], b"3")),
        ],
    );
    let cliente = cliente_com_opcoes(Opcoes {
        tentativas: 2,
        espera_entre_tentativas: Duration::ZERO,
        ..opcoes()
    });

    let resposta = cliente
        .executar_seguro(&Requisicao::get(servidor.url("/consulta")))
        .expect("resposta");

    assert_eq!(resposta.status, 500);
    assert_eq!(resposta.texto().expect("utf-8"), "2");
    assert_eq!(servidor.total(), 2);
}

#[test]
fn resposta_chunked_e_montada_por_completo() {
    let servidor = Servidor::iniciar(
        true,
        vec![Comportamento::Responder(
            b"HTTP/1.1 200 OK\r\n\
              Transfer-Encoding: chunked\r\n\
              Content-Type: text/plain; charset=utf-8\r\n\
              \r\n\
              3\r\nola\r\n\
              5\r\nmundo\r\n\
              0\r\n\r\n"
                .to_vec(),
        )],
    );

    let resposta = cliente(true)
        .executar(&Requisicao::get(servidor.url("/consulta")))
        .expect("resposta chunked");

    assert_eq!(resposta.status, 200);
    assert_eq!(resposta.texto().expect("utf-8"), "olamundo");
}

#[test]
fn resposta_acima_do_limite_e_recusada() {
    let servidor = Servidor::iniciar(
        true,
        vec![Comportamento::Responder(resposta(
            "200 OK",
            &[("Content-Type", "text/plain")],
            &[b'x'; 128],
        ))],
    );
    let cliente = cliente_com_opcoes(Opcoes {
        limite_resposta: 32,
        ..opcoes()
    });

    let erro = cliente
        .executar(&Requisicao::get(servidor.url("/consulta")))
        .expect_err("resposta grande");

    assert_eq!(
        erro,
        ErroEnvio::ResultadoDesconhecido(ErroTransporte::RespostaGrande { limite: 32 })
    );
}

#[test]
fn head_nao_le_corpo_anunciado() {
    let servidor = Servidor::iniciar(
        true,
        vec![Comportamento::Responder(
            b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n".to_vec(),
        )],
    );

    let resposta = cliente(true)
        .executar(&Requisicao::head(servidor.url("/consulta")))
        .expect("resposta HEAD");

    assert_eq!(resposta.status, 200);
    assert!(resposta.corpo.is_empty());
}

#[test]
fn resposta_204_nao_tem_corpo() {
    let servidor = Servidor::iniciar(
        true,
        vec![Comportamento::Responder(
            b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".to_vec(),
        )],
    );

    let resposta = cliente(true)
        .executar(&Requisicao::get(servidor.url("/consulta")))
        .expect("resposta 204");

    assert_eq!(resposta.status, 204);
    assert!(resposta.sucesso());
    assert!(resposta.corpo.is_empty());
}

#[test]
fn metodo_head_serializa_sem_corpo() {
    let requisicao = Requisicao::head("https://exemplo.invalid/consulta");
    assert_eq!(requisicao.metodo, Metodo::Head);
    assert!(requisicao.corpo.is_empty());
    assert!(requisicao.tipo_conteudo.is_none());
}

#[test]
fn resposta_de_erro_preserva_cabecalhos() {
    let resposta = Resposta {
        status: 400,
        cabecalhos: vec![("content-type".to_owned(), "text/xml".to_owned())],
        corpo: b"<erro/>".to_vec(),
    };
    assert_eq!(resposta.cabecalho("Content-Type"), Some("text/xml"));
    assert!(!resposta.sucesso());
    assert_eq!(
        resposta.exigir_sucesso(),
        Err(ErroTransporte::Status { status: 400 })
    );
}
