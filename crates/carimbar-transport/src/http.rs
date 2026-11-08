use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use carimbar_cert::CertificadoA1;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use zeroize::Zeroizing;

use crate::erro::{ErroEnvio, ErroTransporte};
use crate::requisicao::{Metodo, Requisicao};

/// Limite padrão de resposta: 8 MiB.
pub const LIMITE_PADRAO: usize = 8 * 1024 * 1024;

/// Opções do cliente mTLS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opcoes {
    /// Tempo limite de conexão, escrita e leitura.
    pub timeout: Duration,
    /// Tamanho máximo da resposta, em bytes.
    pub limite_resposta: usize,
    /// Total de tentativas de `executar_seguro` (mínimo 1).
    pub tentativas: u32,
    /// Espera entre tentativas.
    pub espera_entre_tentativas: Duration,
}

impl Default for Opcoes {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            limite_resposta: LIMITE_PADRAO,
            tentativas: 3,
            espera_entre_tentativas: Duration::from_millis(200),
        }
    }
}

/// Identidade de cliente (cadeia folha→raiz e chave PKCS#8) para mTLS.
///
/// A chave é zerada ao sair de escopo e nunca aparece em `Debug`.
pub struct Identidade {
    cadeia: Vec<Vec<u8>>,
    chave: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for Identidade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identidade")
            .field("certificados", &self.cadeia.len())
            .field("chave", &"<oculta>")
            .finish()
    }
}

impl Identidade {
    /// Monta a identidade a partir de certificados DER (folha primeiro) e chave PKCS#8.
    pub fn de_der(cadeia: Vec<Vec<u8>>, chave_pkcs8: Vec<u8>) -> Result<Self, ErroTransporte> {
        if cadeia.is_empty() || cadeia.iter().any(Vec::is_empty) {
            return Err(ErroTransporte::Configuracao(
                "cadeia de certificados vazia".to_owned(),
            ));
        }
        if chave_pkcs8.is_empty() {
            return Err(ErroTransporte::Configuracao(
                "chave privada vazia".to_owned(),
            ));
        }
        Ok(Self {
            cadeia,
            chave: Zeroizing::new(chave_pkcs8),
        })
    }

    /// Copia a identidade de um certificado A1 já aberto.
    pub fn do_a1(certificado: &CertificadoA1) -> Result<Self, ErroTransporte> {
        let identidade = certificado.identidade_tls();
        Self::de_der(
            identidade.cadeia_der().to_vec(),
            identidade.chave_pkcs8_der().to_vec(),
        )
    }
}

/// Resposta HTTP recebida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resposta {
    /// Código de status.
    pub status: u16,
    /// Cabeçalhos com nome em minúsculas, na ordem recebida.
    pub cabecalhos: Vec<(String, String)>,
    /// Corpo completo dentro do limite configurado.
    pub corpo: Vec<u8>,
}

impl Resposta {
    /// `true` para 2xx.
    pub fn sucesso(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Primeiro valor do cabeçalho, sem diferenciar maiúsculas de minúsculas.
    pub fn cabecalho(&self, nome: &str) -> Option<&str> {
        self.cabecalhos
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(nome))
            .map(|(_, v)| v.as_str())
    }

    /// Corpo como UTF-8.
    pub fn texto(&self) -> Result<&str, ErroTransporte> {
        std::str::from_utf8(&self.corpo)
            .map_err(|_| ErroTransporte::Protocolo("corpo não é UTF-8".to_owned()))
    }

    /// Converte status fora de 2xx em erro, preservando o corpo no chamador.
    pub fn exigir_sucesso(&self) -> Result<(), ErroTransporte> {
        if self.sucesso() {
            Ok(())
        } else {
            Err(ErroTransporte::Status {
                status: self.status,
            })
        }
    }
}

/// Cliente HTTP/1.1 sobre TLS mútuo, com raízes de confiança explícitas.
///
/// Não há raízes embutidas: a cadeia ICP-Brasil (ou a CA de teste) é fornecida
/// pelo chamador em DER. Toda conexão usa `Connection: close` e um único par
/// requisição/resposta.
pub struct ClienteMtls {
    config: Arc<ClientConfig>,
    opcoes: Opcoes,
}

impl std::fmt::Debug for ClienteMtls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClienteMtls")
            .field("opcoes", &self.opcoes)
            .finish_non_exhaustive()
    }
}

impl ClienteMtls {
    /// Cria o cliente com identidade, raízes DER e opções.
    pub fn novo(
        identidade: Identidade,
        raizes: &[Vec<u8>],
        opcoes: Opcoes,
    ) -> Result<Self, ErroTransporte> {
        if raizes.is_empty() {
            return Err(ErroTransporte::Configuracao(
                "nenhuma raiz de confiança informada".to_owned(),
            ));
        }
        if opcoes.limite_resposta == 0 {
            return Err(ErroTransporte::Configuracao(
                "limite de resposta deve ser maior que zero".to_owned(),
            ));
        }
        if opcoes.tentativas == 0 {
            return Err(ErroTransporte::Configuracao(
                "tentativas deve ser pelo menos 1".to_owned(),
            ));
        }

        let mut store = RootCertStore::empty();
        for der in raizes {
            store
                .add(CertificateDer::from(der.clone()))
                .map_err(|e| ErroTransporte::Configuracao(format!("raiz inválida: {e}")))?;
        }

        let cadeia: Vec<CertificateDer<'static>> = identidade
            .cadeia
            .iter()
            .map(|der| CertificateDer::from(der.clone()))
            .collect();
        let chave = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identidade.chave.to_vec()));

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|e| ErroTransporte::Configuracao(format!("protocolos TLS: {e}")))?
            .with_root_certificates(store)
            .with_client_auth_cert(cadeia, chave)
            .map_err(|e| ErroTransporte::Configuracao(format!("identidade TLS: {e}")))?;

        Ok(Self {
            config: Arc::new(config),
            opcoes,
        })
    }

    /// Atalho para um certificado A1 aberto.
    pub fn com_a1(
        certificado: &CertificadoA1,
        raizes: &[Vec<u8>],
        opcoes: Opcoes,
    ) -> Result<Self, ErroTransporte> {
        Self::novo(Identidade::do_a1(certificado)?, raizes, opcoes)
    }

    /// Opções em uso.
    pub fn opcoes(&self) -> &Opcoes {
        &self.opcoes
    }

    /// Executa uma única vez.
    ///
    /// Use em envio de emissão: uma falha depois do início da escrita retorna
    /// [`ErroEnvio::ResultadoDesconhecido`] e **não** deve ser reenviada sem
    /// consultar o estado no servidor.
    pub fn executar(&self, requisicao: &Requisicao) -> Result<Resposta, ErroEnvio> {
        match self.uma_vez(requisicao) {
            Ok(resposta) => Ok(resposta),
            Err(Falha::Antes(erro)) => Err(ErroEnvio::AntesDoEnvio(erro)),
            Err(Falha::Depois(erro)) => Err(ErroEnvio::ResultadoDesconhecido(erro)),
        }
    }

    /// Executa com retry, para operações comprovadamente seguras.
    ///
    /// Repete erros transitórios (DNS, conexão, timeout) e respostas 429/5xx.
    /// O chamador declara que a operação é segura; envio de emissão deve usar
    /// [`Self::executar`].
    pub fn executar_seguro(&self, requisicao: &Requisicao) -> Result<Resposta, ErroTransporte> {
        let mut tentativa = 0;
        loop {
            tentativa += 1;
            let ultima = tentativa >= self.opcoes.tentativas;
            match self.executar(requisicao) {
                Ok(resposta) if !ultima && (resposta.status == 429 || resposta.status >= 500) => {
                    self.esperar();
                }
                Ok(resposta) => return Ok(resposta),
                Err(ErroEnvio::AntesDoEnvio(erro)) if !ultima && erro.transitorio() => {
                    self.esperar();
                }
                Err(ErroEnvio::AntesDoEnvio(erro)) => return Err(erro),
                // Em operação declarada segura, repetir é permitido mesmo que a
                // requisição possa ter chegado ao servidor.
                Err(ErroEnvio::ResultadoDesconhecido(_erro)) if !ultima => {
                    self.esperar();
                }
                Err(ErroEnvio::ResultadoDesconhecido(erro)) => return Err(erro),
            }
        }
    }

    fn esperar(&self) {
        if !self.opcoes.espera_entre_tentativas.is_zero() {
            std::thread::sleep(self.opcoes.espera_entre_tentativas);
        }
    }

    fn uma_vez(&self, requisicao: &Requisicao) -> Result<Resposta, Falha> {
        let url = Url::parse(&requisicao.url).map_err(Falha::antes)?;
        let mut stream = self.conectar(&url).map_err(Falha::antes)?;

        // Completa o handshake antes de escrever, para classificar recusa de
        // certificado como falha anterior ao envio.
        while stream.conn.is_handshaking() {
            stream
                .conn
                .complete_io(&mut stream.sock)
                .map_err(|e| Falha::antes(erro_tls(&e)))?;
        }

        let bytes = montar_requisicao(requisicao, &url)?;
        stream
            .write_all(&bytes)
            .map_err(|e| Falha::depois(erro_io(e, "escrita")))?;
        stream
            .flush()
            .map_err(|e| Falha::depois(erro_io(e, "escrita")))?;

        ler_resposta(&mut stream, self.opcoes.limite_resposta, requisicao.metodo)
            .map_err(Falha::depois)
    }

    fn conectar(
        &self,
        url: &Url,
    ) -> Result<StreamOwned<ClientConnection, TcpStream>, ErroTransporte> {
        let enderecos = (url.host.as_str(), url.porta)
            .to_socket_addrs()
            .map_err(|_| ErroTransporte::Dns {
                host: url.host.clone(),
            })?;

        let mut ultima_falha = None;
        let mut tcp = None;
        for endereco in enderecos {
            match TcpStream::connect_timeout(&endereco, self.opcoes.timeout) {
                Ok(stream) => {
                    tcp = Some(stream);
                    break;
                }
                Err(erro) => ultima_falha = Some(erro),
            }
        }
        let tcp = tcp.ok_or_else(|| match ultima_falha {
            Some(erro) if erro.kind() == std::io::ErrorKind::TimedOut => {
                ErroTransporte::Timeout("conexão")
            }
            _ => ErroTransporte::Conexao {
                host: url.host.clone(),
                porta: url.porta,
            },
        })?;
        tcp.set_read_timeout(Some(self.opcoes.timeout))
            .map_err(|e| erro_io(e, "leitura"))?;
        tcp.set_write_timeout(Some(self.opcoes.timeout))
            .map_err(|e| erro_io(e, "escrita"))?;
        tcp.set_nodelay(true).ok();

        let nome = ServerName::try_from(url.host.clone())
            .map_err(|e| ErroTransporte::Tls(format!("nome do servidor inválido: {e}")))?;
        let conn = ClientConnection::new(Arc::clone(&self.config), nome)
            .map_err(|e| ErroTransporte::Tls(e.to_string()))?;
        Ok(StreamOwned::new(conn, tcp))
    }
}

enum Falha {
    Antes(ErroTransporte),
    Depois(ErroTransporte),
}

impl Falha {
    fn antes(erro: ErroTransporte) -> Self {
        Self::Antes(erro)
    }

    fn depois(erro: ErroTransporte) -> Self {
        Self::Depois(erro)
    }
}

fn erro_io(erro: std::io::Error, fase: &'static str) -> ErroTransporte {
    match erro.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
            ErroTransporte::Timeout(fase)
        }
        // Erros de protocolo TLS chegam embrulhados em `InvalidData`; separá-los
        // permite reconhecer recusa de certificado também após o handshake.
        std::io::ErrorKind::InvalidData
            if erro
                .get_ref()
                .is_some_and(|interno| interno.downcast_ref::<rustls::Error>().is_some()) =>
        {
            ErroTransporte::Tls(erro.to_string())
        }
        _ => ErroTransporte::Io {
            fase,
            detalhe: erro.to_string(),
        },
    }
}

fn erro_tls(erro: &std::io::Error) -> ErroTransporte {
    match erro.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
            ErroTransporte::Timeout("TLS")
        }
        _ => ErroTransporte::Tls(erro.to_string()),
    }
}

struct Url {
    /// Host para resolução DNS e SNI, sem colchetes.
    host: String,
    /// Valor do cabeçalho `Host`, com colchetes quando o host é IPv6.
    host_cabecalho: String,
    porta: u16,
    caminho: String,
}

impl Url {
    fn parse(url: &str) -> Result<Self, ErroTransporte> {
        if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(ErroTransporte::Url(url.to_owned()));
        }
        // Fragmento não é enviado ao servidor.
        let url = url.split('#').next().unwrap_or_default();
        let resto = url
            .strip_prefix("https://")
            .ok_or_else(|| ErroTransporte::Url(url.to_owned()))?;
        if resto.is_empty() {
            return Err(ErroTransporte::Url(url.to_owned()));
        }

        let fim_autoridade = resto.find(['/', '?']).unwrap_or(resto.len());
        let autoridade = &resto[..fim_autoridade];
        if autoridade.is_empty() || autoridade.contains('@') {
            return Err(ErroTransporte::Url(url.to_owned()));
        }

        let (host, porta) = if let Some(sem_colchete) = autoridade.strip_prefix('[') {
            let (host, depois) = sem_colchete
                .split_once(']')
                .ok_or_else(|| ErroTransporte::Url(url.to_owned()))?;
            let porta = match depois {
                "" => 443,
                resto if resto.starts_with(':') => parse_porta(&resto[1..])?,
                _ => return Err(ErroTransporte::Url(url.to_owned())),
            };
            (host.to_owned(), porta)
        } else if let Some((host, porta)) = autoridade.rsplit_once(':') {
            (host.to_owned(), parse_porta(porta)?)
        } else {
            (autoridade.to_owned(), 443)
        };
        if host.is_empty() {
            return Err(ErroTransporte::Url(url.to_owned()));
        }
        let host_cabecalho = if autoridade.starts_with('[') {
            format!("[{host}]")
        } else {
            host.clone()
        };

        let mut caminho = resto[fim_autoridade..].to_owned();
        if caminho.is_empty() {
            caminho.push('/');
        } else if !caminho.starts_with('/') {
            caminho.insert(0, '/');
        }
        Ok(Self {
            host,
            host_cabecalho,
            porta,
            caminho,
        })
    }
}

fn parse_porta(texto: &str) -> Result<u16, ErroTransporte> {
    texto
        .parse()
        .ok()
        .filter(|porta| *porta > 0)
        .ok_or_else(|| ErroTransporte::Url(format!("porta inválida: {texto}")))
}

fn montar_requisicao(requisicao: &Requisicao, url: &Url) -> Result<Vec<u8>, Falha> {
    validar_cabecalho(&requisicao.url, "URL")?;
    if let Some(tipo) = &requisicao.tipo_conteudo {
        validar_cabecalho(tipo, "Content-Type")?;
    }
    if let Some(aceitar) = &requisicao.aceitar {
        validar_cabecalho(aceitar, "Accept")?;
    }

    let mut cabecalho = String::new();
    write!(
        cabecalho,
        "{} {} HTTP/1.1\r\nHost: {}",
        requisicao.metodo.as_str(),
        url.caminho,
        url.host_cabecalho
    )
    .map_err(|_| Falha::antes(ErroTransporte::Configuracao("cabeçalho".to_owned())))?;
    if url.porta != 443 {
        write!(cabecalho, ":{}", url.porta)
            .map_err(|_| Falha::antes(ErroTransporte::Configuracao("cabeçalho".to_owned())))?;
    }
    cabecalho.push_str("\r\n");
    if let Some(tipo) = &requisicao.tipo_conteudo {
        write!(cabecalho, "Content-Type: {tipo}\r\n")
            .map_err(|_| Falha::antes(ErroTransporte::Configuracao("cabeçalho".to_owned())))?;
    }
    if let Some(aceitar) = &requisicao.aceitar {
        write!(cabecalho, "Accept: {aceitar}\r\n")
            .map_err(|_| Falha::antes(ErroTransporte::Configuracao("cabeçalho".to_owned())))?;
    }
    let tem_corpo = requisicao.metodo == Metodo::Post;
    if tem_corpo {
        write!(cabecalho, "Content-Length: {}\r\n", requisicao.corpo.len())
            .map_err(|_| Falha::antes(ErroTransporte::Configuracao("cabeçalho".to_owned())))?;
    }
    cabecalho.push_str("Connection: close\r\n");
    cabecalho.push_str(concat!(
        "User-Agent: carimbar-transport/",
        env!("CARGO_PKG_VERSION"),
        "\r\n\r\n"
    ));

    let mut bytes = cabecalho.into_bytes();
    if tem_corpo {
        bytes.extend_from_slice(&requisicao.corpo);
    }
    Ok(bytes)
}

fn validar_cabecalho(valor: &str, campo: &str) -> Result<(), Falha> {
    if valor.contains(['\r', '\n']) {
        Err(Falha::antes(ErroTransporte::Configuracao(format!(
            "{campo} contém quebra de linha"
        ))))
    } else {
        Ok(())
    }
}

fn ler_resposta(
    stream: &mut StreamOwned<ClientConnection, TcpStream>,
    limite: usize,
    metodo: Metodo,
) -> Result<Resposta, ErroTransporte> {
    let mut leitor = Leitor::novo(stream, limite);
    let status_linha = leitor.linha(8192)?;
    let mut partes = status_linha.split_whitespace();
    let versao = partes
        .next()
        .ok_or_else(|| ErroTransporte::Protocolo("status line vazia".to_owned()))?;
    if !versao.starts_with("HTTP/1.") {
        return Err(ErroTransporte::Protocolo(format!(
            "versão inesperada: {versao}"
        )));
    }
    let status: u16 = partes
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ErroTransporte::Protocolo("status HTTP ausente".to_owned()))?;

    let mut cabecalhos = Vec::new();
    loop {
        let linha = leitor.linha(8192)?;
        if linha.is_empty() {
            break;
        }
        if cabecalhos.len() >= 100 {
            return Err(ErroTransporte::Protocolo("cabeçalhos demais".to_owned()));
        }
        let (nome, valor) = linha
            .split_once(':')
            .ok_or_else(|| ErroTransporte::Protocolo(format!("cabeçalho inválido: {linha}")))?;
        cabecalhos.push((nome.trim().to_ascii_lowercase(), valor.trim().to_owned()));
    }

    let chunked = cabecalhos.iter().any(|(nome, valor)| {
        nome == "transfer-encoding" && valor.to_ascii_lowercase().contains("chunked")
    });
    let corpo = if metodo == Metodo::Head || status == 204 || status == 304 {
        Vec::new()
    } else if chunked {
        leitor.chunked()?
    } else if let Some((_, tamanho)) = cabecalhos.iter().find(|(n, _)| n == "content-length") {
        let tamanho: usize = tamanho
            .parse()
            .map_err(|_| ErroTransporte::Protocolo("Content-Length inválido".to_owned()))?;
        if tamanho > limite {
            return Err(ErroTransporte::RespostaGrande { limite });
        }
        leitor.exato(tamanho)?
    } else {
        leitor.ate_o_fim()?
    };

    Ok(Resposta {
        status,
        cabecalhos,
        corpo,
    })
}

struct Leitor<'a> {
    stream: &'a mut StreamOwned<ClientConnection, TcpStream>,
    buf: Vec<u8>,
    limite: usize,
}

impl<'a> Leitor<'a> {
    fn novo(stream: &'a mut StreamOwned<ClientConnection, TcpStream>, limite: usize) -> Self {
        Self {
            stream,
            buf: Vec::new(),
            limite,
        }
    }

    fn encher(&mut self) -> Result<usize, ErroTransporte> {
        if self.buf.len() >= self.limite {
            return Err(ErroTransporte::RespostaGrande {
                limite: self.limite,
            });
        }
        let mut temp = [0_u8; 8192];
        let max = temp.len().min(self.limite - self.buf.len());
        let lidos = self
            .stream
            .read(&mut temp[..max])
            .map_err(|e| erro_io(e, "leitura"))?;
        self.buf.extend_from_slice(&temp[..lidos]);
        Ok(lidos)
    }

    fn garantir(&mut self, minimo: usize) -> Result<(), ErroTransporte> {
        while self.buf.len() < minimo {
            if self.encher()? == 0 {
                return Err(ErroTransporte::Protocolo(
                    "conexão encerrada antes do corpo completo".to_owned(),
                ));
            }
        }
        Ok(())
    }

    fn linha(&mut self, maximo: usize) -> Result<String, ErroTransporte> {
        loop {
            if let Some(pos) = self.buf.windows(2).position(|par| par == b"\r\n") {
                let linha = self.buf[..pos].to_vec();
                self.buf.drain(..pos + 2);
                return String::from_utf8(linha)
                    .map_err(|_| ErroTransporte::Protocolo("linha não é UTF-8".to_owned()));
            }
            if self.buf.len() > maximo {
                return Err(ErroTransporte::Protocolo("linha longa demais".to_owned()));
            }
            if self.encher()? == 0 {
                return Err(ErroTransporte::Protocolo(
                    "conexão encerrada no cabeçalho".to_owned(),
                ));
            }
        }
    }

    fn exato(&mut self, tamanho: usize) -> Result<Vec<u8>, ErroTransporte> {
        if tamanho > self.limite {
            return Err(ErroTransporte::RespostaGrande {
                limite: self.limite,
            });
        }
        self.garantir(tamanho)?;
        Ok(self.buf.drain(..tamanho).collect())
    }

    fn ate_o_fim(&mut self) -> Result<Vec<u8>, ErroTransporte> {
        loop {
            if self.encher()? == 0 {
                break;
            }
        }
        Ok(std::mem::take(&mut self.buf))
    }

    fn chunked(&mut self) -> Result<Vec<u8>, ErroTransporte> {
        let mut corpo = Vec::new();
        loop {
            let linha = self.linha(1024)?;
            let tamanho_texto = linha.split(';').next().unwrap_or("").trim();
            let tamanho = usize::from_str_radix(tamanho_texto, 16)
                .map_err(|_| ErroTransporte::Protocolo("chunk inválido".to_owned()))?;
            if tamanho == 0 {
                loop {
                    if self.linha(8192)?.is_empty() {
                        break;
                    }
                }
                return Ok(corpo);
            }
            if corpo.len().saturating_add(tamanho) > self.limite {
                return Err(ErroTransporte::RespostaGrande {
                    limite: self.limite,
                });
            }
            corpo.extend(self.exato(tamanho)?);
            let crlf = self.exato(2)?;
            if crlf != b"\r\n" {
                return Err(ErroTransporte::Protocolo("chunk sem CRLF final".to_owned()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_exige_https_e_separa_porta() {
        let url = Url::parse("https://exemplo.invalid:8443/nfse?x=1").expect("URL");
        assert_eq!(url.host, "exemplo.invalid");
        assert_eq!(url.porta, 8443);
        assert_eq!(url.caminho, "/nfse?x=1");
        assert!(Url::parse("http://exemplo.invalid").is_err());
        assert!(Url::parse("https://exemplo.invalid:0").is_err());
        assert!(Url::parse("https://usuario@exemplo.invalid").is_err());
        assert!(Url::parse("https://exemplo.invalid/a b").is_err());
    }

    #[test]
    fn url_sem_caminho_usa_barra() {
        let url = Url::parse("https://exemplo.invalid").expect("URL");
        assert_eq!(url.caminho, "/");
        assert_eq!(url.porta, 443);
    }

    #[test]
    fn url_ipv6_usa_colchetes_no_host() {
        let url = Url::parse("https://[::1]:8443/nfse").expect("URL");
        assert_eq!(url.host, "::1");
        assert_eq!(url.host_cabecalho, "[::1]");
        assert_eq!(url.porta, 8443);
        assert_eq!(url.caminho, "/nfse");
    }

    #[test]
    fn url_descarta_fragmento() {
        let url = Url::parse("https://exemplo.invalid/nfse#secao").expect("URL");
        assert_eq!(url.caminho, "/nfse");
    }

    #[test]
    fn identidade_nao_mostra_chave() {
        let identidade = Identidade::de_der(vec![vec![1, 2, 3]], vec![4, 5, 6]).expect("id");
        let debug = format!("{identidade:?}");
        assert!(debug.contains("oculta"));
        assert!(!debug.contains("4, 5, 6"));
    }

    #[test]
    fn opcoes_padrao_sao_utilitarias() {
        let opcoes = Opcoes::default();
        assert!(opcoes.tentativas >= 1);
        assert!(opcoes.timeout > Duration::ZERO);
        assert_eq!(opcoes.limite_resposta, LIMITE_PADRAO);
    }

    fn identidade_teste() -> Identidade {
        Identidade::de_der(vec![vec![1, 2, 3]], vec![4, 5, 6]).expect("id")
    }

    #[test]
    fn configuracao_invalida_e_recusada() {
        assert!(matches!(
            ClienteMtls::novo(identidade_teste(), &[], Opcoes::default()),
            Err(ErroTransporte::Configuracao(_))
        ));
        assert!(matches!(
            ClienteMtls::novo(
                identidade_teste(),
                &[vec![0]],
                Opcoes {
                    limite_resposta: 0,
                    ..Opcoes::default()
                }
            ),
            Err(ErroTransporte::Configuracao(_))
        ));
        assert!(matches!(
            ClienteMtls::novo(
                identidade_teste(),
                &[vec![0]],
                Opcoes {
                    tentativas: 0,
                    ..Opcoes::default()
                }
            ),
            Err(ErroTransporte::Configuracao(_))
        ));
    }

    #[test]
    fn identidade_vazia_e_recusada() {
        assert!(Identidade::de_der(Vec::new(), vec![1]).is_err());
        assert!(Identidade::de_der(vec![Vec::new()], vec![1]).is_err());
        assert!(Identidade::de_der(vec![vec![1]], Vec::new()).is_err());
    }
}
