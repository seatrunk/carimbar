//! Implementação das ferramentas MCP e do estado de preparação.
//!
//! Operações com efeito fiscal usam duas fases: `preparar_*` devolve um token
//! de uso único, vinculado à empresa, ambiente e conteúdo exato; `confirmar_*`
//! executa. Tokens expiram e não sobrevivem ao reinício do processo.
//!
//! O servidor não expõe importação de certificado, senha, chave ou promoção de
//! produção. Essas ações ficam na CLI. O kill switch da fachada continua valendo
//! em todos os caminhos fiscais.

use base64::Engine;
#[cfg(feature = "nfe-sha1")]
use carimbar::ResultadoNfe;
use carimbar::{Emissor, ErroEmissor, Journal, KillSwitch, KillSwitchAmbiente, ResultadoNfse};
use carimbar_nfse::{
    Aliquota, ChaveNfse, ConsultaAliquotas, DataHoraFiscal, EventoCancelamento, Executor,
    InscricaoFederal, MotivoCancelamento, ParametrosConvenio,
};
use carimbar_transport::{ClienteMtls, ErroEnvio, ErroTransporte, Opcoes, Requisicao, Resposta};
use carimbar_types::{ChaveAcesso, Cnpj, TpAmb};
use carimbar_vault::{ChaveMestra, Cofre};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{ServerCapabilities, ServerConfig};
use rmcp::{ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Configuração do servidor MCP.
pub struct Config {
    /// Caminho do cofre.
    pub cofre: PathBuf,
    /// Caminho do journal.
    pub journal: PathBuf,
    /// KEK local.
    pub kek: [u8; 32],
    /// Diretório de raízes DER.
    pub raizes: Option<PathBuf>,
    /// Respostas simuladas (somente build de depuração).
    pub replay: Option<PathBuf>,
    /// Validade dos tokens de preparação.
    pub ttl_preparacao: Duration,
    /// Kill switch compartilhado.
    pub kill_switch: Arc<dyn KillSwitch>,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("cofre", &self.cofre)
            .field("journal", &self.journal)
            .field("kek", &"<oculta>")
            .field("raizes", &self.raizes)
            .field("replay", &self.replay)
            .field("ttl_preparacao", &self.ttl_preparacao)
            .finish_non_exhaustive()
    }
}

impl Config {
    /// Lê a configuração das variáveis de ambiente.
    pub fn de_ambiente() -> Result<Self, String> {
        let cofre = std::env::var("CARIMBAR_COFRE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("carimbar-cofre.db"));
        let journal = std::env::var("CARIMBAR_JOURNAL")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("carimbar-journal.db"));
        let kek = ler_kek()?;
        let raizes = std::env::var("CARIMBAR_RAIZES").ok().map(PathBuf::from);
        let replay = std::env::var("CARIMBAR_REPLAY").ok().map(PathBuf::from);
        let ttl = std::env::var("CARIMBAR_MCP_TTL_SEGUNDOS")
            .ok()
            .and_then(|valor| valor.parse::<u64>().ok())
            .unwrap_or(300);
        Ok(Self {
            cofre,
            journal,
            kek,
            raizes,
            replay,
            ttl_preparacao: Duration::from_secs(ttl),
            kill_switch: Arc::new(KillSwitchAmbiente),
        })
    }
}

fn ler_kek() -> Result<[u8; 32], String> {
    let texto = std::env::var("CARIMBAR_KEK")
        .map_err(|_| "defina CARIMBAR_KEK com 64 caracteres hexadecimais".to_owned())?;
    let texto = texto.strip_prefix("0x").unwrap_or(&texto);
    if texto.len() != 64 {
        return Err("CARIMBAR_KEK deve ter 64 caracteres hexadecimais".to_owned());
    }
    let mut chave = [0_u8; 32];
    for (indice, par) in texto.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let par =
            std::str::from_utf8(par).map_err(|_| "CARIMBAR_KEK não é hexadecimal".to_owned())?;
        chave[indice] =
            u8::from_str_radix(par, 16).map_err(|_| "CARIMBAR_KEK não é hexadecimal".to_owned())?;
    }
    Ok(chave)
}

/// Payload exato vinculado a um token.
#[derive(Debug, Clone)]
enum Payload {
    Emissao(String),
    EmissaoNfe(String),
    Cancelamento(EventoCancelamento),
}

#[derive(Debug, Clone)]
struct Preparacao {
    empresa: Cnpj,
    ambiente: TpAmb,
    payload: Payload,
    expira_em: Instant,
}

/// Estado dos tokens de preparação.
#[derive(Debug)]
pub struct Preparacoes {
    ttl: Duration,
    mapa: Mutex<HashMap<String, Preparacao>>,
}

impl Preparacoes {
    fn novo(ttl: Duration) -> Self {
        Self {
            ttl,
            mapa: Mutex::new(HashMap::new()),
        }
    }

    fn inserir(&self, empresa: Cnpj, ambiente: TpAmb, payload: Payload) -> String {
        self.limpar_expiradas();
        let token = token();
        let expira_em = Instant::now() + self.ttl;
        self.mapa.lock().expect("preparações").insert(
            token.clone(),
            Preparacao {
                empresa,
                ambiente,
                payload,
                expira_em,
            },
        );
        token
    }

    fn conferir(&self, token: &str) -> Result<Preparacao, String> {
        let preparacao = self
            .mapa
            .lock()
            .expect("preparações")
            .get(token)
            .cloned()
            .ok_or_else(|| "token desconhecido, expirado ou já usado".to_owned())?;
        if preparacao.expira_em <= Instant::now() {
            return Err("token expirado".to_owned());
        }
        Ok(preparacao)
    }

    fn retirar(&self, token: &str) -> Result<Preparacao, String> {
        let preparacao = self
            .mapa
            .lock()
            .expect("preparações")
            .remove(token)
            .ok_or_else(|| "token desconhecido, expirado ou já usado".to_owned())?;
        if preparacao.expira_em <= Instant::now() {
            return Err("token expirado".to_owned());
        }
        Ok(preparacao)
    }

    fn limpar_expiradas(&self) {
        let agora = Instant::now();
        self.mapa
            .lock()
            .expect("preparações")
            .retain(|_, preparacao| preparacao.expira_em > agora);
    }
}

fn token() -> String {
    let mut bytes = [0_u8; 32];
    if getrandom::fill(&mut bytes).is_err() {
        let agora = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duracao| duracao.as_nanos())
            .unwrap_or_default();
        bytes[..16].copy_from_slice(&agora.to_le_bytes());
    }
    let mut saida = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(saida, "{byte:02x}");
    }
    saida
}

struct KillSwitchCompartilhado(Arc<dyn KillSwitch>);

impl KillSwitch for KillSwitchCompartilhado {
    fn producao_permitida(&self) -> bool {
        self.0.producao_permitida()
    }
}

/// Servidor MCP com as ferramentas da fachada.
#[derive(Clone)]
pub struct ServidorCarimbar {
    config: Arc<Config>,
    preparacoes: Arc<Preparacoes>,
    tool_router: ToolRouter<Self>,
}

impl ServidorCarimbar {
    /// Cria o servidor com a configuração informada.
    pub fn novo(config: Config) -> Self {
        let preparacoes = Arc::new(Preparacoes::novo(config.ttl_preparacao));
        Self {
            config: Arc::new(config),
            preparacoes,
            tool_router: Self::tool_router(),
        }
    }

    fn com_emissor<T>(
        &self,
        empresa: Cnpj,
        f: impl FnOnce(&mut Emissor<'_>) -> Result<T, ErroEmissor>,
    ) -> Result<T, String> {
        let cofre = abrir_cofre(&self.config.cofre, self.config.kek)?;
        let journal = Journal::abrir(&self.config.journal).map_err(|erro| erro.to_string())?;
        let executor = self.criar_executor(&cofre, &empresa)?;
        let mut emissor = Emissor::com_kill_switch(
            cofre,
            journal,
            empresa,
            executor.as_ref(),
            Box::new(KillSwitchCompartilhado(self.config.kill_switch.clone())),
        )
        .map_err(|erro| erro.to_string())?;
        f(&mut emissor).map_err(|erro| erro.to_string())
    }

    fn criar_executor(&self, cofre: &Cofre, empresa: &Cnpj) -> Result<Box<dyn Executor>, String> {
        if let Some(caminho) = &self.config.replay {
            let executor = ExecutorReplay::de_arquivo(caminho)?;
            return Ok(Box::new(executor));
        }
        let id = cofre
            .listar(empresa)
            .map_err(|erro| erro.to_string())?
            .first()
            .map(|meta| meta.id.clone())
            .ok_or_else(|| format!("empresa {empresa} não tem certificado importado"))?;
        let certificado = cofre
            .certificado(empresa, &id)
            .map_err(|erro| erro.to_string())?;
        let raizes = ler_raizes(self.config.raizes.as_deref())?;
        let cliente = ClienteMtls::com_a1(&certificado, &raizes, Opcoes::default())
            .map_err(|erro| erro.to_string())?;
        Ok(Box::new(cliente))
    }
}

fn abrir_cofre(caminho: &Path, kek: [u8; 32]) -> Result<Cofre, String> {
    if caminho.exists() {
        Cofre::abrir(caminho, Box::new(ChaveMestra::de_bytes(kek))).map_err(|erro| erro.to_string())
    } else {
        Cofre::criar(caminho, Box::new(ChaveMestra::de_bytes(kek))).map_err(|erro| erro.to_string())
    }
}

fn ler_raizes(diretorio: Option<&Path>) -> Result<Vec<Vec<u8>>, String> {
    let diretorio = diretorio
        .ok_or_else(|| "defina CARIMBAR_RAIZES com um diretório de raízes DER".to_owned())?;
    let mut raizes = Vec::new();
    for entrada in std::fs::read_dir(diretorio).map_err(|erro| erro.to_string())? {
        let entrada = entrada.map_err(|erro| erro.to_string())?;
        let caminho = entrada.path();
        let extensao = caminho
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(extensao.as_str(), "der" | "cer" | "crt") {
            raizes.push(std::fs::read(&caminho).map_err(|erro| erro.to_string())?);
        }
    }
    if raizes.is_empty() {
        return Err(format!("nenhuma raiz DER em {}", diretorio.display()));
    }
    Ok(raizes)
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PrepararEmissaoReq {
    #[schemars(description = "CNPJ da empresa")]
    empresa: String,
    #[schemars(description = "XML da DPS, assinado ou não")]
    dps_xml: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ConfirmarEmissaoReq {
    #[schemars(description = "Token devolvido por preparar_emissao")]
    token: String,
    #[schemars(description = "Confirmação humana explícita para produção")]
    confirmar_producao: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ConsultarNfseReq {
    empresa: String,
    chave: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PrepararCancelamentoReq {
    empresa: String,
    chave: String,
    /// 1 erro na emissão, 2 serviço não prestado, 9 outros.
    motivo: u8,
    descricao: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ConfirmarCancelamentoReq {
    token: String,
    confirmar_producao: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GerarDanfseReq {
    empresa: String,
    operacao: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SincronizarAdnReq {
    empresa: String,
    limite_paginas: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ConsultarConvenioReq {
    empresa: String,
    municipio: String,
    forcar: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ConsultarAliquotaReq {
    empresa: String,
    municipio: String,
    servico: String,
    competencia: String,
    forcar: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct EstadoOperacaoReq {
    empresa: String,
    operacao: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PrepararEmissaoNfeReq {
    empresa: String,
    nfe_xml: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ConfirmarEmissaoNfeReq {
    token: String,
    confirmar_producao: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ConsultarNfeReq {
    empresa: String,
    chave: String,
}

#[tool_router]
impl ServidorCarimbar {
    /// Prepara uma emissão de NFS-e sem enviar.
    #[tool(
        description = "Prepara uma DPS para emissão e devolve um token de uso único. Não envia à Sefin."
    )]
    fn preparar_emissao(
        &self,
        Parameters(req): Parameters<PrepararEmissaoReq>,
    ) -> Result<Json<Value>, String> {
        let empresa = parse_empresa(&req.empresa)?;
        let dps_xml = req.dps_xml;
        let (operacao, estado, ambiente) = self.com_emissor(empresa, |emissor| {
            let preparada = emissor.preparar_nfse_xml(&dps_xml)?;
            Ok((
                preparada.id(),
                preparada.estado(),
                emissor.ambiente().tp_amb(),
            ))
        })?;
        let token = self
            .preparacoes
            .inserir(empresa, ambiente, Payload::Emissao(dps_xml));
        Ok(Json(json!({
            "token": token,
            "operacao": operacao,
            "estado": estado.as_str(),
            "empresa": empresa.as_str(),
            "ambiente": ambiente_texto(ambiente),
            "expira_em_segundos": self.config.ttl_preparacao.as_secs(),
        })))
    }

    /// Confirma a emissão preparada.
    #[tool(
        description = "Confirma a emissão pelo token de preparação. O token é de uso único e expira."
    )]
    fn confirmar_emissao(
        &self,
        Parameters(req): Parameters<ConfirmarEmissaoReq>,
    ) -> Result<Json<Value>, String> {
        let preparacao = self.preparacoes.conferir(&req.token)?;
        let Payload::Emissao(dps_xml) = preparacao.payload.clone() else {
            return Err("token não é de emissão".to_owned());
        };
        exigir_confirmacao_producao(preparacao.ambiente, req.confirmar_producao)?;
        let preparacao = self.preparacoes.retirar(&req.token)?;
        let resultado = self.com_emissor(preparacao.empresa, |emissor| {
            let preparada = emissor.preparar_nfse_xml(&dps_xml)?;
            emissor.confirmar_nfse(preparada.id())
        })?;
        Ok(Json(resultado_nfse(&resultado)))
    }

    /// Consulta uma NFS-e pela chave.
    #[tool(description = "Consulta a NFS-e pela chave de acesso e devolve o XML.")]
    fn consultar_nfse(
        &self,
        Parameters(req): Parameters<ConsultarNfseReq>,
    ) -> Result<Json<Value>, String> {
        let empresa = parse_empresa(&req.empresa)?;
        let chave = req.chave;
        let xml = self.com_emissor(empresa, |emissor| {
            let chave =
                ChaveNfse::parse(&chave, emissor.ambiente().tp_amb()).map_err(ErroEmissor::Nfse)?;
            emissor.consultar_nfse(&chave)
        })?;
        Ok(Json(json!({ "chave": chave, "xml": xml })))
    }

    /// Prepara um cancelamento.
    #[tool(
        description = "Prepara o cancelamento de uma NFS-e e devolve um token de uso único. Não envia à Sefin."
    )]
    fn preparar_cancelamento(
        &self,
        Parameters(req): Parameters<PrepararCancelamentoReq>,
    ) -> Result<Json<Value>, String> {
        let empresa = parse_empresa(&req.empresa)?;
        let motivo = motivo_cancelamento(req.motivo)?;
        let descricao = req.descricao;
        let (operacao, ambiente, evento) = self.com_emissor(empresa, |emissor| {
            let ambiente = emissor.ambiente().tp_amb();
            let evento = EventoCancelamento {
                ambiente,
                chave: ChaveNfse::parse(&req.chave, ambiente).map_err(ErroEmissor::Nfse)?,
                autor: InscricaoFederal::parse(empresa.as_str()).map_err(ErroEmissor::Nfse)?,
                ver_aplic: versao_aplic(),
                dh_evento: DataHoraFiscal::parse(&agora_fiscal()).map_err(ErroEmissor::Nfse)?,
                motivo,
                descricao,
            };
            let preparada = emissor.preparar_cancelamento(&evento)?;
            Ok((preparada.id(), ambiente, evento))
        })?;
        let token = self
            .preparacoes
            .inserir(empresa, ambiente, Payload::Cancelamento(evento));
        Ok(Json(json!({
            "token": token,
            "operacao": operacao,
            "estado": "preparada",
            "empresa": empresa.as_str(),
            "ambiente": ambiente_texto(ambiente),
            "expira_em_segundos": self.config.ttl_preparacao.as_secs(),
        })))
    }

    /// Confirma o cancelamento preparado.
    #[tool(
        description = "Confirma o cancelamento pelo token de preparação. O token é de uso único e expira."
    )]
    fn confirmar_cancelamento(
        &self,
        Parameters(req): Parameters<ConfirmarCancelamentoReq>,
    ) -> Result<Json<Value>, String> {
        let preparacao = self.preparacoes.conferir(&req.token)?;
        let Payload::Cancelamento(evento) = preparacao.payload.clone() else {
            return Err("token não é de cancelamento".to_owned());
        };
        exigir_confirmacao_producao(preparacao.ambiente, req.confirmar_producao)?;
        let preparacao = self.preparacoes.retirar(&req.token)?;
        let resultado = self.com_emissor(preparacao.empresa, |emissor| {
            let preparada = emissor.preparar_cancelamento(&evento)?;
            emissor.confirmar_cancelamento(preparada.id())
        })?;
        Ok(Json(resultado_nfse(&resultado)))
    }

    /// Gera o DANFSe de uma operação autorizada.
    #[tool(
        description = "Gera o DANFSe em PDF de uma operação autorizada e devolve o PDF em base64."
    )]
    fn gerar_danfse(
        &self,
        Parameters(req): Parameters<GerarDanfseReq>,
    ) -> Result<Json<Value>, String> {
        let empresa = parse_empresa(&req.empresa)?;
        let pdf = self.com_emissor(empresa, |emissor| emissor.gerar_danfse(req.operacao))?;
        Ok(Json(json!({
            "operacao": req.operacao,
            "pdf_base64": base64::engine::general_purpose::STANDARD.encode(pdf),
        })))
    }

    /// Sincroniza documentos do ADN.
    #[tool(description = "Sincroniza documentos do ADN por NSU para a empresa.")]
    fn sincronizar_adn(
        &self,
        Parameters(req): Parameters<SincronizarAdnReq>,
    ) -> Result<Json<Value>, String> {
        let empresa = parse_empresa(&req.empresa)?;
        let limite = req.limite_paginas.unwrap_or(10);
        let resultado = self.com_emissor(empresa, |emissor| emissor.sincronizar_adn(limite))?;
        Ok(Json(json!({
            "inicio_nsu": resultado.inicio_nsu,
            "ultimo_nsu": resultado.ultimo_nsu,
            "documentos_novos": resultado.documentos_novos,
            "paginas": resultado.paginas,
        })))
    }

    /// Consulta o convênio de um município.
    #[tool(description = "Consulta o convênio municipal no ADN, com cache e atualização opcional.")]
    fn consultar_convenio(
        &self,
        Parameters(req): Parameters<ConsultarConvenioReq>,
    ) -> Result<Json<Value>, String> {
        let empresa = parse_empresa(&req.empresa)?;
        let forcar = req.forcar.unwrap_or(false);
        let convenio = self.com_emissor(empresa, |emissor| {
            emissor.consultar_convenio(&req.municipio, forcar)
        })?;
        Ok(Json(convenio_json(&convenio)))
    }

    /// Consulta alíquotas de um serviço.
    #[tool(description = "Consulta alíquotas municipais por serviço e competência.")]
    fn consultar_aliquota(
        &self,
        Parameters(req): Parameters<ConsultarAliquotaReq>,
    ) -> Result<Json<Value>, String> {
        let empresa = parse_empresa(&req.empresa)?;
        let forcar = req.forcar.unwrap_or(false);
        let consulta = self.com_emissor(empresa, |emissor| {
            emissor.consultar_aliquota(&req.municipio, &req.servico, &req.competencia, forcar)
        })?;
        Ok(Json(aliquotas_json(&consulta)))
    }

    /// Consulta o estado local de uma operação.
    #[tool(description = "Consulta o estado local de uma operação no journal.")]
    fn estado_operacao(
        &self,
        Parameters(req): Parameters<EstadoOperacaoReq>,
    ) -> Result<Json<Value>, String> {
        let empresa = parse_empresa(&req.empresa)?;
        let operacao =
            self.com_emissor(empresa, |emissor| emissor.journal().operacao(req.operacao))?;
        Ok(Json(json!({
            "operacao": operacao.id,
            "estado": operacao.estado.as_str(),
            "chave": operacao.chave_nfse,
            "tipo": operacao.tipo.as_str(),
            "erro": operacao.erro,
        })))
    }

    /// Prepara uma NF-e sem enviar.
    #[tool(
        description = "Prepara uma NF-e modelo 55 e devolve um token de uso único. Não envia à Sefaz."
    )]
    fn preparar_emissao_nfe(
        &self,
        Parameters(req): Parameters<PrepararEmissaoNfeReq>,
    ) -> Result<Json<Value>, String> {
        let empresa = parse_empresa(&req.empresa)?;
        let nfe_xml = req.nfe_xml;
        let (operacao, ambiente) = self.com_emissor(empresa, |emissor| {
            let preparada = emissor.preparar_nfe_xml(&nfe_xml)?;
            Ok((preparada.id(), emissor.ambiente().tp_amb()))
        })?;
        let token = self
            .preparacoes
            .inserir(empresa, ambiente, Payload::EmissaoNfe(nfe_xml));
        Ok(Json(json!({
            "token": token,
            "operacao": operacao,
            "estado": "preparada",
            "empresa": empresa.as_str(),
            "ambiente": ambiente_texto(ambiente),
            "expira_em_segundos": self.config.ttl_preparacao.as_secs(),
        })))
    }

    /// Confirma a NF-e preparada.
    #[tool(
        description = "Confirma a NF-e pelo token de preparação. O token é de uso único e expira."
    )]
    #[allow(clippy::needless_return)]
    fn confirmar_emissao_nfe(
        &self,
        Parameters(req): Parameters<ConfirmarEmissaoNfeReq>,
    ) -> Result<Json<Value>, String> {
        let preparacao = self.preparacoes.conferir(&req.token)?;
        let Payload::EmissaoNfe(nfe_xml) = preparacao.payload.clone() else {
            return Err("token não é de emissão de NF-e".to_owned());
        };
        exigir_confirmacao_producao(preparacao.ambiente, req.confirmar_producao)?;
        let preparacao = self.preparacoes.retirar(&req.token)?;
        #[cfg(feature = "nfe-sha1")]
        {
            let resultado = self.com_emissor(preparacao.empresa, |emissor| {
                let preparada = emissor.preparar_nfe_xml(&nfe_xml)?;
                emissor.confirmar_nfe(preparada.id())
            })?;
            return Ok(Json(resultado_nfe(&resultado)));
        }
        #[cfg(not(feature = "nfe-sha1"))]
        {
            let _ = (preparacao, nfe_xml);
            Err("NF-e requer a feature nfe-sha1".to_owned())
        }
    }

    /// Consulta uma NF-e pela chave.
    #[tool(description = "Consulta a situação de uma NF-e pela chave.")]
    #[allow(clippy::needless_return)]
    fn consultar_nfe(
        &self,
        Parameters(req): Parameters<ConsultarNfeReq>,
    ) -> Result<Json<Value>, String> {
        let empresa = parse_empresa(&req.empresa)?;
        let chave = ChaveAcesso::parse(&req.chave).map_err(|erro| erro.to_string())?;
        #[cfg(feature = "nfe-sha1")]
        {
            let consulta = self.com_emissor(empresa, |emissor| emissor.consultar_nfe(&chave))?;
            return Ok(Json(json!({
                "chave": chave.as_str(),
                "codigo": consulta.codigo,
                "motivo": consulta.motivo,
                "protocolo": consulta.protocolo.map(|p| p.numero),
            })));
        }
        #[cfg(not(feature = "nfe-sha1"))]
        {
            let _ = (empresa, chave);
            Err("NF-e requer a feature nfe-sha1".to_owned())
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ServidorCarimbar {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Ferramentas fiscais do carimbar. Operações com efeito fiscal usam preparar_*/confirmar_*; confirmações são de uso único, expiram e não sobrevivem ao reinício. Produção exige confirmar_producao=true.",
        )
    }
}

fn parse_empresa(empresa: &str) -> Result<Cnpj, String> {
    Cnpj::parse(empresa).map_err(|erro| erro.to_string())
}

fn ambiente_texto(ambiente: TpAmb) -> &'static str {
    match ambiente {
        TpAmb::Producao => "producao",
        TpAmb::Homologacao => "homologacao",
    }
}

fn exigir_confirmacao_producao(
    ambiente: TpAmb,
    confirmar_producao: Option<bool>,
) -> Result<(), String> {
    if ambiente == TpAmb::Producao && confirmar_producao != Some(true) {
        return Err("produção exige confirmar_producao=true".to_owned());
    }
    Ok(())
}

fn resultado_nfse(resultado: &ResultadoNfse) -> Value {
    match resultado {
        ResultadoNfse::Autorizada {
            operacao,
            chave,
            xml_nfse,
        } => json!({
            "estado": "autorizada",
            "operacao": operacao,
            "chave": chave.as_str(),
            "xml_nfse": xml_nfse,
        }),
        ResultadoNfse::Rejeitada {
            operacao,
            erros,
            detalhe,
        } => json!({
            "estado": "rejeitada",
            "operacao": operacao,
            "erros": erros.iter().map(|erro| json!({
                "codigo": erro.codigo,
                "descricao": erro.descricao,
                "complemento": erro.complemento,
            })).collect::<Vec<_>>(),
            "detalhe": detalhe,
        }),
        ResultadoNfse::Desconhecido { operacao, detalhe } => json!({
            "estado": "desconhecida",
            "operacao": operacao,
            "detalhe": detalhe,
        }),
        ResultadoNfse::NaoEncontrada { operacao, detalhe } => json!({
            "estado": "nao_encontrada",
            "operacao": operacao,
            "detalhe": detalhe,
        }),
        ResultadoNfse::Cancelada {
            operacao,
            xml_evento,
        } => json!({
            "estado": "cancelada",
            "operacao": operacao,
            "xml_evento": xml_evento,
        }),
    }
}

#[cfg(feature = "nfe-sha1")]
fn resultado_nfe(resultado: &ResultadoNfe) -> Value {
    match resultado {
        ResultadoNfe::Autorizada {
            operacao,
            chave,
            xml,
        } => json!({
            "estado": "autorizada",
            "operacao": operacao,
            "chave": chave.as_str(),
            "xml": xml,
        }),
        ResultadoNfe::Recibo { operacao, recibo } => json!({
            "estado": "recibo",
            "operacao": operacao,
            "recibo": recibo,
        }),
        ResultadoNfe::Rejeitada {
            operacao,
            codigo,
            motivo,
        } => json!({
            "estado": "rejeitada",
            "operacao": operacao,
            "codigo": codigo,
            "motivo": motivo,
        }),
        ResultadoNfe::Desconhecida { operacao, detalhe } => json!({
            "estado": "desconhecida",
            "operacao": operacao,
            "detalhe": detalhe,
        }),
        ResultadoNfe::NaoEncontrada { operacao, detalhe } => json!({
            "estado": "nao_encontrada",
            "operacao": operacao,
            "detalhe": detalhe,
        }),
        ResultadoNfe::Cancelada {
            operacao,
            protocolo,
        } => json!({
            "estado": "cancelada",
            "operacao": operacao,
            "protocolo": protocolo,
        }),
    }
}

fn convenio_json(convenio: &ParametrosConvenio) -> Value {
    json!({
        "tipo_convenio": convenio.tipo_convenio,
        "aderente_ambiente_nacional": convenio.aderente_ambiente_nacional,
        "aderente_emissor_nacional": convenio.aderente_emissor_nacional,
        "situacao_emissao_padrao": convenio.situacao_emissao_padrao,
        "aderente_man": convenio.aderente_man,
        "permite_aproveitamento_creditos": convenio.permite_aproveitamento_creditos,
    })
}

fn aliquotas_json(consulta: &ConsultaAliquotas) -> Value {
    let mut mapa = serde_json::Map::new();
    for (codigo, lista) in &consulta.aliquotas {
        let itens = lista.iter().map(aliquota_json).collect::<Vec<_>>();
        mapa.insert(codigo.clone(), Value::Array(itens));
    }
    json!({"mensagem": consulta.mensagem, "aliquotas": mapa})
}

fn aliquota_json(aliquota: &Aliquota) -> Value {
    json!({
        "incidencia": aliquota.incidencia,
        "aliquota": aliquota.aliquota,
        "data_inicio": aliquota.data_inicio,
        "data_fim": aliquota.data_fim,
    })
}

fn motivo_cancelamento(codigo: u8) -> Result<MotivoCancelamento, String> {
    match codigo {
        1 => Ok(MotivoCancelamento::ErroEmissao),
        2 => Ok(MotivoCancelamento::ServicoNaoPrestado),
        9 => Ok(MotivoCancelamento::Outros),
        outro => Err(format!("motivo {outro} inválido; use 1, 2 ou 9")),
    }
}

fn versao_aplic() -> String {
    std::env::var("CARIMBAR_VER_APLIC")
        .unwrap_or_else(|_| format!("carimbar-mcp-{}", env!("CARGO_PKG_VERSION")))
}

fn agora_fiscal() -> String {
    let formato =
        time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]+00:00");
    time::OffsetDateTime::now_utc()
        .format(&formato)
        .unwrap_or_else(|_| "2026-01-01T00:00:00+00:00".to_owned())
}

/// Executor de replay para testes de protocolo.
#[derive(Debug)]
struct ExecutorReplay {
    respostas: Mutex<Vec<(Option<String>, u16, String)>>,
}

impl ExecutorReplay {
    fn de_arquivo(caminho: &Path) -> Result<Self, String> {
        if !cfg!(debug_assertions) {
            return Err("replay disponível apenas em build de depuração".to_owned());
        }
        let texto = std::fs::read_to_string(caminho).map_err(|erro| erro.to_string())?;
        let valor: Value = serde_json::from_str(&texto).map_err(|erro| erro.to_string())?;
        let lista = valor
            .get("respostas")
            .and_then(Value::as_array)
            .ok_or_else(|| "replay: respostas ausente".to_owned())?;
        let mut respostas = Vec::new();
        for item in lista {
            let objeto = item
                .as_object()
                .ok_or_else(|| "replay: resposta inválida".to_owned())?;
            let status = objeto
                .get("status")
                .and_then(Value::as_u64)
                .and_then(|valor| u16::try_from(valor).ok())
                .ok_or_else(|| "replay: status inválido".to_owned())?;
            let corpo = objeto
                .get("corpo")
                .and_then(Value::as_str)
                .ok_or_else(|| "replay: corpo ausente".to_owned())?;
            respostas.push((
                objeto
                    .get("url_contem")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                status,
                corpo.to_owned(),
            ));
        }
        Ok(Self {
            respostas: Mutex::new(respostas),
        })
    }
}

impl Executor for ExecutorReplay {
    fn executar(&self, requisicao: &Requisicao) -> Result<Resposta, ErroEnvio> {
        let mut respostas = self.respostas.lock().expect("replay");
        let indice = respostas.iter().position(|(filtro, _, _)| {
            filtro
                .as_deref()
                .is_none_or(|trecho| requisicao.url.contains(trecho))
        });
        let Some(indice) = indice else {
            return Err(ErroEnvio::AntesDoEnvio(ErroTransporte::Configuracao(
                format!("replay sem resposta para {}", requisicao.url),
            )));
        };
        let (_, status, corpo) = respostas.remove(indice);
        Ok(Resposta {
            status,
            cabecalhos: vec![("content-type".to_owned(), "application/json".to_owned())],
            corpo: corpo.into_bytes(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carimbar::KillSwitchFixo;
    use carimbar_nfse::comprimir_b64;
    use std::process::{Command, Stdio};

    pub(super) const CNPJ: &str = "00000001000136";
    const SENHA: &str = "senha-sintetica-123";
    const KEK: [u8; 32] = [9; 32];
    const ID_DPS: &str = "DPS355030810000000100013600001000000000000001";

    fn cnpj() -> Cnpj {
        Cnpj::parse(CNPJ).expect("CNPJ")
    }

    fn chave() -> String {
        format!("{}{}{}", "355030", CNPJ, "0".repeat(30))
    }

    fn diretorio(nome: &str) -> PathBuf {
        let unico = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duracao| duracao.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("carimbar-mcp-{nome}-{unico}"));
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
            .expect("openssl disponível");
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
            chave.to_str().expect("utf-8"),
            "-out",
            cert.to_str().expect("utf-8"),
            "-subj",
            "/CN=EMPRESA SINTETICA MCP",
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
        std::fs::read(pfx).expect("pfx")
    }

    fn dps_xml(ambiente: u8) -> String {
        format!(
            r#"<DPS xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.01"><infDPS Id="{ID_DPS}"><tpAmb>{ambiente}</tpAmb><prest><CNPJ>{CNPJ}</CNPJ></prest></infDPS></DPS>"#
        )
    }

    fn nfse_xml() -> String {
        format!(
            r#"<NFSe xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.01"><infNFSe Id="NFS{}"><xLocEmi>Sao Paulo</xLocEmi><xLocPrestacao>Sao Paulo</xLocPrestacao><nNFSe>123</nNFSe><cStat>100</cStat><dhProc>2026-11-09T10:00:00-03:00</dhProc><nDFSe>1</nDFSe><emit><CNPJ>{CNPJ}</CNPJ><xNome>Empresa Sintetica</xNome><enderNac><xLgr>Rua A</xLgr><nro>1</nro><xBairro>Centro</xBairro><cMun>3550308</cMun><UF>SP</UF><CEP>01001000</CEP></enderNac></emit><valores><vBC>10.00</vBC><pAliqAplic>2.00</pAliqAplic><vISSQN>0.20</vISSQN><vLiq>9.80</vLiq></valores><DPS versao="1.01"><infDPS Id="{ID_DPS}"><tpAmb>2</tpAmb><serv><cServ><cTribNac>010101</cTribNac><xDescServ>Servico</xDescServ></cServ></serv><valores><vServPrest><vServ>10.00</vServ></vServPrest><trib><tribMun><tribISSQN>1</tribISSQN></tribMun></trib></valores></infDPS></DPS></infNFSe></NFSe>"#,
            chave()
        )
    }

    pub(super) fn cenario(
        nome: &str,
        ambiente: TpAmb,
        kill_switch: Arc<dyn KillSwitch>,
    ) -> (Config, PathBuf) {
        let dir = diretorio(nome);
        let cofre_path = dir.join("cofre.db");
        let journal_path = dir.join("journal.db");
        let empresa = cnpj();
        let mut cofre =
            Cofre::criar(&cofre_path, Box::new(ChaveMestra::de_bytes(KEK))).expect("cofre");
        cofre
            .importar(&empresa, "a1", &pfx(&dir), SENHA)
            .expect("importa");
        if ambiente == TpAmb::Producao {
            let journal = Journal::abrir(&journal_path).expect("journal");
            journal.garantir_empresa(&empresa).expect("empresa");
            journal.promover_empresa(&empresa, "a1").expect("promove");
        }
        let corpo = json!({
            "idDps": ID_DPS,
            "chaveAcesso": chave(),
            "nfseXmlGZipB64": comprimir_b64(&nfse_xml()).expect("gzip"),
            "dataHoraProcessamento": "2026-11-09T10:00:00-03:00",
            "alertas": []
        })
        .to_string();
        let replay = json!({"respostas":[{"url_contem":"/nfse","status":201,"corpo":corpo}]});
        let replay_path = dir.join("replay.json");
        std::fs::write(&replay_path, replay.to_string()).expect("replay");
        let config = Config {
            cofre: cofre_path,
            journal: journal_path,
            kek: KEK,
            raizes: None,
            replay: Some(replay_path),
            ttl_preparacao: Duration::from_secs(300),
            kill_switch,
        };
        (config, dir)
    }

    #[test]
    fn token_expira_e_e_de_uso_unico() {
        let preparacoes = Preparacoes::novo(Duration::from_secs(60));
        let token = preparacoes.inserir(
            cnpj(),
            TpAmb::Homologacao,
            Payload::Emissao("<DPS/>".to_owned()),
        );
        assert!(preparacoes.retirar(&token).is_ok());
        assert!(preparacoes.retirar(&token).is_err());

        let expiradas = Preparacoes::novo(Duration::ZERO);
        let token = expiradas.inserir(
            cnpj(),
            TpAmb::Homologacao,
            Payload::Emissao("<DPS/>".to_owned()),
        );
        assert!(expiradas.retirar(&token).is_err());
    }

    #[test]
    fn lista_ferramentas_sem_administracao() {
        let (config, _dir) = cenario("lista", TpAmb::Homologacao, Arc::new(KillSwitchFixo(true)));
        let servidor = ServidorCarimbar::novo(config);
        let nomes = servidor
            .tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect::<Vec<_>>();
        for esperado in [
            "preparar_emissao",
            "confirmar_emissao",
            "consultar_nfse",
            "preparar_cancelamento",
            "confirmar_cancelamento",
            "gerar_danfse",
            "sincronizar_adn",
            "consultar_convenio",
            "consultar_aliquota",
            "estado_operacao",
        ] {
            assert!(
                nomes.iter().any(|nome| nome == esperado),
                "{esperado}: {nomes:?}"
            );
        }
        assert!(!nomes.iter().any(|nome| nome.contains("importar")));
        assert!(!nomes.iter().any(|nome| nome.contains("promover")));
    }

    #[test]
    fn prepara_confirma_e_gera_danfse_com_replay() {
        let (config, _dir) = cenario("fluxo", TpAmb::Homologacao, Arc::new(KillSwitchFixo(true)));
        let servidor = ServidorCarimbar::novo(config);
        let preparada = servidor
            .preparar_emissao(Parameters(PrepararEmissaoReq {
                empresa: CNPJ.to_owned(),
                dps_xml: dps_xml(2),
            }))
            .expect("prepara");
        let token = preparada.0["token"].as_str().expect("token").to_owned();
        let resultado = servidor
            .confirmar_emissao(Parameters(ConfirmarEmissaoReq {
                token: token.clone(),
                confirmar_producao: None,
            }))
            .expect("confirma");
        assert_eq!(resultado.0["estado"], "autorizada");
        let operacao = resultado.0["operacao"].as_i64().expect("operação");
        assert!(
            servidor
                .confirmar_emissao(Parameters(ConfirmarEmissaoReq {
                    token,
                    confirmar_producao: None,
                }))
                .is_err()
        );
        let pdf = servidor
            .gerar_danfse(Parameters(GerarDanfseReq {
                empresa: CNPJ.to_owned(),
                operacao,
            }))
            .expect("DANFSe");
        assert!(
            pdf.0["pdf_base64"]
                .as_str()
                .expect("base64")
                .starts_with("JVBERi")
        );
        let estado = servidor
            .estado_operacao(Parameters(EstadoOperacaoReq {
                empresa: CNPJ.to_owned(),
                operacao,
            }))
            .expect("estado");
        assert_eq!(estado.0["estado"], "autorizada");
    }

    #[test]
    fn producao_exige_confirmacao_humana() {
        let (config, _dir) = cenario("producao", TpAmb::Producao, Arc::new(KillSwitchFixo(true)));
        let servidor = ServidorCarimbar::novo(config);
        let preparada = servidor
            .preparar_emissao(Parameters(PrepararEmissaoReq {
                empresa: CNPJ.to_owned(),
                dps_xml: dps_xml(1),
            }))
            .expect("prepara");
        let token = preparada.0["token"].as_str().expect("token").to_owned();
        let erro = match servidor.confirmar_emissao(Parameters(ConfirmarEmissaoReq {
            token: token.clone(),
            confirmar_producao: None,
        })) {
            Ok(_) => panic!("sem confirmação humana deveria falhar"),
            Err(erro) => erro,
        };
        assert!(erro.contains("confirmar_producao"));
        let resultado = servidor
            .confirmar_emissao(Parameters(ConfirmarEmissaoReq {
                token,
                confirmar_producao: Some(true),
            }))
            .expect("com confirmação humana");
        assert_eq!(resultado.0["estado"], "autorizada");
    }

    #[test]
    fn kill_switch_bloqueia_confirmacao() {
        let (config, _dir) = cenario("kill", TpAmb::Producao, Arc::new(KillSwitchFixo(false)));
        let servidor = ServidorCarimbar::novo(config);
        let preparada = servidor
            .preparar_emissao(Parameters(PrepararEmissaoReq {
                empresa: CNPJ.to_owned(),
                dps_xml: dps_xml(1),
            }))
            .expect("prepara");
        let token = preparada.0["token"].as_str().expect("token").to_owned();
        let erro = match servidor.confirmar_emissao(Parameters(ConfirmarEmissaoReq {
            token,
            confirmar_producao: Some(true),
        })) {
            Ok(_) => panic!("kill switch deveria bloquear"),
            Err(erro) => erro,
        };
        assert!(erro.contains("produção bloqueada"));
    }
}

#[cfg(all(test, feature = "nfe-sha1"))]
mod tests_nfe {
    use super::tests::*;
    use super::*;

    #[test]
    fn prepara_confirma_nfe_com_replay() {
        let (config, dir) = cenario(
            "nfe",
            TpAmb::Homologacao,
            Arc::new(carimbar::KillSwitchFixo(true)),
        );
        let chave =
            carimbar_types::ChaveAcesso::com_dv("3526110000000100013655001000000000112345678")
                .expect("chave")
                .as_str()
                .to_owned();
        let nfe = format!(
            r#"<NFe xmlns="http://www.portalfiscal.inf.br/nfe"><infNFe versao="4.00" Id="NFe{chave}"><ide><cUF>35</cUF><tpAmb>2</tpAmb></ide><emit><CNPJ>{CNPJ}</CNPJ></emit></infNFe></NFe>"#
        );
        let corpo = format!(
            r#"<retEnviNFe xmlns="http://www.portalfiscal.inf.br/nfe" versao="4.00"><cStat>104</cStat><xMotivo>Lote processado</xMotivo><protNFe versao="4.00"><infProt><chNFe>{chave}</chNFe><nProt>135</nProt><cStat>100</cStat><xMotivo>Autorizado</xMotivo></infProt></protNFe></retEnviNFe>"#
        );
        let replay = json!({"respostas": [{"url_contem": "nfeautorizacao4", "status": 200, "corpo": corpo}]});
        std::fs::write(dir.join("replay.json"), replay.to_string()).expect("replay");
        let servidor = ServidorCarimbar::novo(config);
        let preparada = servidor
            .preparar_emissao_nfe(Parameters(PrepararEmissaoNfeReq {
                empresa: CNPJ.to_owned(),
                nfe_xml: nfe,
            }))
            .expect("prepara NF-e");
        let token = preparada.0["token"].as_str().expect("token").to_owned();
        let resultado = servidor
            .confirmar_emissao_nfe(Parameters(ConfirmarEmissaoNfeReq {
                token,
                confirmar_producao: None,
            }))
            .expect("confirma NF-e");
        assert_eq!(resultado.0["estado"], "autorizada");
        assert_eq!(resultado.0["chave"], chave);
    }
}
