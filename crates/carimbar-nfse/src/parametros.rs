//! Parâmetros municipais do ADN, com cache e validade explícitas.
//!
//! Os endpoints ficam em `carimbar-endpoints` e o transporte é o mesmo
//! [`Executor`] da emissão. A chave de cache inclui ambiente, serviço e todos
//! os parâmetros, para que municípios e ambientes distintos nunca se misturem.

use crate::api::Executor;
use crate::erro::ErroNfse;
use carimbar_endpoints::{ConsultaParametros, ServicoParametros, resolver_parametros};
use carimbar_transport::{ErroEnvio, Requisicao, Resposta};
use carimbar_types::TpAmb;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Entrada de cache com instante de expiração.
#[derive(Debug, Clone)]
struct EntradaCache {
    corpo: String,
    expira_em: Instant,
}

/// Cache em memória com TTL.
#[derive(Debug)]
pub struct CacheParametros {
    ttl: Duration,
    entradas: Mutex<HashMap<String, EntradaCache>>,
}

impl CacheParametros {
    /// Cria o cache com validade explícita.
    pub fn novo(ttl: Duration) -> Self {
        Self {
            ttl,
            entradas: Mutex::new(HashMap::new()),
        }
    }

    /// Cache padrão de 1 hora.
    pub fn padrao() -> Self {
        Self::novo(Duration::from_secs(3600))
    }

    /// Validade configurada.
    pub fn ttl(&self) -> Duration {
        self.ttl
    }

    /// Remove todas as entradas.
    pub fn limpar(&self) {
        self.entradas.lock().expect("cache").clear();
    }

    /// Remove uma chave específica.
    pub fn invalidar(&self, chave: &str) {
        self.entradas.lock().expect("cache").remove(chave);
    }

    /// Quantidade de entradas vivas.
    pub fn tamanho(&self) -> usize {
        let agora = Instant::now();
        let mut entradas = self.entradas.lock().expect("cache");
        entradas.retain(|_, entrada| entrada.expira_em > agora);
        entradas.len()
    }

    fn obter(&self, chave: &str) -> Option<String> {
        let agora = Instant::now();
        let mut entradas = self.entradas.lock().expect("cache");
        match entradas.get(chave) {
            Some(entrada) if entrada.expira_em > agora => Some(entrada.corpo.clone()),
            Some(_) => {
                entradas.remove(chave);
                None
            }
            None => None,
        }
    }

    fn guardar(&self, chave: String, corpo: String) {
        let expira_em = Instant::now() + self.ttl;
        self.entradas
            .lock()
            .expect("cache")
            .insert(chave, EntradaCache { corpo, expira_em });
    }
}

/// Parâmetros de convênio do município.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParametrosConvenio {
    /// Tipo de convênio.
    pub tipo_convenio: String,
    /// Aderente ao ambiente nacional.
    pub aderente_ambiente_nacional: String,
    /// Aderente ao emissor nacional.
    pub aderente_emissor_nacional: String,
    /// Situação de emissão padrão dos contribuintes RFB.
    pub situacao_emissao_padrao: String,
    /// Aderente ao MAN.
    pub aderente_man: String,
    /// Permite aproveitamento de créditos.
    pub permite_aproveitamento_creditos: Option<bool>,
}

/// Alíquota de ISSQN parametrizada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aliquota {
    /// Incidência.
    pub incidencia: Option<String>,
    /// Alíquota em percentual, como texto para não perder precisão.
    pub aliquota: Option<String>,
    /// Início da vigência.
    pub data_inicio: String,
    /// Fim da vigência.
    pub data_fim: Option<String>,
}

/// Consulta de alíquotas por código de serviço.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsultaAliquotas {
    /// Mensagem informativa, quando houver.
    pub mensagem: Option<String>,
    /// Mapa `cTribNac -> histórico de alíquotas`.
    pub aliquotas: BTreeMap<String, Vec<Aliquota>>,
}

/// Cliente da API de parâmetros municipais.
pub struct ClienteParametros<'a> {
    ambiente: TpAmb,
    executor: &'a dyn Executor,
    cache: Option<&'a CacheParametros>,
}

impl<'a> ClienteParametros<'a> {
    /// Cria o cliente com cache opcional.
    pub fn new(
        ambiente: TpAmb,
        executor: &'a dyn Executor,
        cache: Option<&'a CacheParametros>,
    ) -> Self {
        Self {
            ambiente,
            executor,
            cache,
        }
    }

    /// Consulta o convênio do município.
    pub fn consultar_convenio(
        &self,
        codigo_municipio: &str,
        forcar_atualizacao: bool,
    ) -> Result<ParametrosConvenio, ErroNfse> {
        let corpo = self.buscar(
            ServicoParametros::Convenio,
            &[("codigoMunicipio", codigo_municipio)],
            forcar_atualizacao,
        )?;
        let objeto = parse_objeto(&corpo)?;
        let bruto = objeto
            .get("parametrosConvenio")
            .and_then(Value::as_object)
            .ok_or_else(|| ErroNfse::ParametroAusente("parametrosConvenio".to_owned()))?;
        Ok(ParametrosConvenio {
            tipo_convenio: numero_ou_texto(bruto, "tipoConvenioDeserializationSetter")
                .unwrap_or_default(),
            aderente_ambiente_nacional: numero_ou_texto(bruto, "aderenteAmbienteNacional")
                .unwrap_or_default(),
            aderente_emissor_nacional: numero_ou_texto(bruto, "aderenteEmissorNacional")
                .unwrap_or_default(),
            situacao_emissao_padrao: numero_ou_texto(
                bruto,
                "situacaoEmissaoPadraoContribuintesRFB",
            )
            .unwrap_or_default(),
            aderente_man: numero_ou_texto(bruto, "aderenteMAN").unwrap_or_default(),
            permite_aproveitamento_creditos: bruto
                .get("permiteAproveitametoDeCreditos")
                .and_then(Value::as_bool),
        })
    }

    /// Consulta a alíquota de um serviço na competência.
    pub fn consultar_aliquota(
        &self,
        codigo_municipio: &str,
        codigo_servico: &str,
        competencia: &str,
        forcar_atualizacao: bool,
    ) -> Result<ConsultaAliquotas, ErroNfse> {
        let corpo = self.buscar(
            ServicoParametros::Aliquota,
            &[
                ("codigoMunicipio", codigo_municipio),
                ("codigoServico", codigo_servico),
                ("competencia", competencia),
            ],
            forcar_atualizacao,
        )?;
        let objeto = parse_objeto(&corpo)?;
        let mensagem = objeto
            .get("mensagem")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let Some(mapa) = objeto.get("aliquotas").and_then(Value::as_object) else {
            return Err(ErroNfse::ParametroAusente(
                mensagem.unwrap_or_else(|| "aliquotas".to_owned()),
            ));
        };
        let mut aliquotas = BTreeMap::new();
        for (codigo, lista) in mapa {
            let lista = lista
                .as_array()
                .map(|itens| itens.iter().filter_map(parse_aliquota).collect())
                .unwrap_or_default();
            aliquotas.insert(codigo.clone(), lista);
        }
        if aliquotas.is_empty() {
            return Err(ErroNfse::ParametroAusente(mensagem.unwrap_or_else(|| {
                format!("aliquota {codigo_servico}/{competencia}")
            })));
        }
        Ok(ConsultaAliquotas {
            mensagem,
            aliquotas,
        })
    }

    /// Consulta retenções do município e devolve o JSON bruto.
    pub fn consultar_retencoes(
        &self,
        codigo_municipio: &str,
        competencia: &str,
        forcar_atualizacao: bool,
    ) -> Result<String, ErroNfse> {
        self.buscar(
            ServicoParametros::Retencoes,
            &[
                ("codigoMunicipio", codigo_municipio),
                ("competencia", competencia),
            ],
            forcar_atualizacao,
        )
    }

    fn buscar(
        &self,
        servico: ServicoParametros,
        parametros: &[(&str, &str)],
        forcar_atualizacao: bool,
    ) -> Result<String, ErroNfse> {
        let chave = chave_cache(self.ambiente, servico, parametros);
        if !forcar_atualizacao && let Some(corpo) = self.cache.and_then(|cache| cache.obter(&chave))
        {
            return Ok(corpo);
        }
        let endpoint = resolver_parametros(&ConsultaParametros::new(servico, self.ambiente))?;
        let url = endpoint.preencher(parametros)?;
        let resposta = match self.executor.executar(&Requisicao::get_json(url)) {
            Ok(resposta) => resposta,
            Err(ErroEnvio::AntesDoEnvio(erro) | ErroEnvio::ResultadoDesconhecido(erro)) => {
                return Err(ErroNfse::ResultadoDesconhecido(erro));
            }
            Err(outro) => return Err(ErroNfse::Transporte(outro.to_string())),
        };
        let corpo = corpo_aceito(&resposta)?;
        if let Some(cache) = self.cache {
            cache.guardar(chave, corpo.clone());
        }
        Ok(corpo)
    }
}

fn chave_cache(ambiente: TpAmb, servico: ServicoParametros, parametros: &[(&str, &str)]) -> String {
    let mut chave = format!("{}:{}", ambiente.codigo(), servico.nome());
    for (nome, valor) in parametros {
        chave.push('|');
        chave.push_str(nome);
        chave.push('=');
        chave.push_str(valor);
    }
    chave
}

fn corpo_aceito(resposta: &Resposta) -> Result<String, ErroNfse> {
    // O ADN usa 400/404 com corpo útil para "sem parâmetro".
    if resposta.status == 200 || matches!(resposta.status, 400 | 404) {
        return String::from_utf8(resposta.corpo.clone())
            .map_err(|_| ErroNfse::Json("corpo do ADN não é UTF-8".to_owned()));
    }
    Err(ErroNfse::Http {
        status: resposta.status,
        corpo: String::from_utf8_lossy(&resposta.corpo)
            .chars()
            .take(512)
            .collect(),
    })
}

fn parse_objeto(corpo: &str) -> Result<Map<String, Value>, ErroNfse> {
    let valor: Value = serde_json::from_str(corpo)
        .map_err(|erro| ErroNfse::Json(format!("JSON de parâmetros: {erro}")))?;
    valor
        .as_object()
        .cloned()
        .ok_or_else(|| ErroNfse::Json("parâmetros do ADN não são objeto".to_owned()))
}

fn parse_aliquota(valor: &Value) -> Option<Aliquota> {
    let objeto = valor.as_object()?;
    Some(Aliquota {
        incidencia: objeto
            .get("Incidencia")
            .and_then(Value::as_str)
            .map(str::to_owned),
        aliquota: objeto
            .get("Aliq")
            .and_then(Value::as_number)
            .map(ToString::to_string),
        data_inicio: objeto.get("DtIni")?.as_str()?.to_owned(),
        data_fim: objeto
            .get("DtFim")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn numero_ou_texto(objeto: &Map<String, Value>, campo: &str) -> Option<String> {
    match objeto.get(campo)? {
        Value::String(texto) => Some(texto.clone()),
        Value::Number(numero) => Some(numero.to_string()),
        Value::Bool(valor) => Some(u8::from(*valor).to_string()),
        _ => None,
    }
}
