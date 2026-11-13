//! Construtor de DPS a partir de um pedido simples.
//!
//! O chamador informa o prestador uma vez (município, regime e dados públicos)
//! e, por nota, apenas serviço, descrição, valor e tomador. O módulo deriva o
//! restante da DPS: `Id`, local da prestação, tributos e `totTrib` conforme o
//! regime.
//!
//! O formato de entrada é um objeto JSON. A CLI lê `--pedido pedido.json` (com
//! `--prestador prestador.json` opcional), o MCP aceita o mesmo objeto e o
//! binding Python converte um `dict` para JSON. O parser é deliberadamente
//! tolerante a campos extras, para permitir evolução sem quebrar chamadores.
//!
//! Quando `competencia` não é informada, vale a data UTC do `dhEmi`; quem está
//! em fuso local com o dia já virado deve informar a competência explicitamente.
//!
//! Exemplo mínimo:
//!
//! ```json
//! {
//!   "prestador": {
//!     "municipio": "3550308",
//!     "simples": 3,
//!     "apuracao": 1,
//!     "percentual_simples_nacional": "6.00"
//!   },
//!   "id": "venda-123",
//!   "servico": "010101",
//!   "descricao": "Consultoria prestada",
//!   "valor_centavos": 10000,
//!   "tomador": {"cnpj": "00000002000008", "nome": "Cliente"}
//! }
//! ```

use crate::nfse::dps::{
    Dps, LocalPrestacao, OpSimplesNacional, Pessoa, Prestador, RegimeApuracaoSimples,
    RegimeEspecialTributacao, RegimeTributario, Servico, TipoEmitente, TipoRetencaoIssqn,
    TotalTributos, TribIssqn, TributoMunicipal, Valores,
};
use crate::nfse::erro::ErroNfse;
use crate::nfse::identificadores::{CodigoMunicipio, InscricaoFederal, NumeroDps, SerieDps};
use crate::nfse::tempo::{DataFiscal, DataHoraFiscal};
use crate::types::{Cnpj, Dinheiro, TpAmb};
use rust_decimal::Decimal;
use serde_json::{Map, Value};
use std::str::FromStr;

/// Configuração reutilizável do prestador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrestadorConfig {
    /// Município emissor (`cLocEmi`).
    pub municipio: CodigoMunicipio,
    /// Série padrão da DPS, de 1 a 49999 (E0010).
    pub serie: u32,
    /// Inscrição municipal, quando o município a exige.
    pub inscricao_municipal: Option<String>,
    /// Nome ou razão social.
    pub nome: Option<String>,
    /// E-mail de contato.
    pub email: Option<String>,
    /// Regime tributário.
    pub regime: RegimeTributario,
    /// Percentual aproximado do Simples Nacional, usado quando o pedido não
    /// informa `total_tributos` para ME/EPP.
    pub percentual_simples_nacional: Option<Decimal>,
    /// Versão do aplicativo emissor; padrão `carimbar-<versão>`.
    pub ver_aplic: Option<String>,
}

impl PrestadorConfig {
    /// Lê a configuração do prestador de um objeto JSON.
    pub fn de_json(valor: &Value) -> Result<Self, ErroNfse> {
        let objeto = objeto(valor, "prestador")?;
        let municipio = CodigoMunicipio::new(texto_obrigatorio(objeto, "municipio", "prestador")?)?;
        let serie = numero_u32_opcional(objeto, "serie")?.unwrap_or(1);
        SerieDps::new_api(serie)?;
        let simples = parse_simples(
            objeto
                .get("simples")
                .ok_or_else(|| ErroNfse::campo("prestador/simples", "obrigatório: 1, 2 ou 3"))?,
        )?;
        let apuracao = match objeto.get("apuracao") {
            Some(valor) => Some(parse_apuracao(valor)?),
            None => None,
        };
        let regime_especial = match objeto.get("regime_especial") {
            Some(valor) => parse_regime_especial(valor)?,
            None => RegimeEspecialTributacao::Nenhum,
        };
        let percentual_simples_nacional = objeto
            .get("percentual_simples_nacional")
            .map(|valor| decimal(valor, "prestador/percentual_simples_nacional"))
            .transpose()?;
        Ok(Self {
            municipio,
            serie,
            inscricao_municipal: texto_opcional(objeto, "inscricao_municipal", "prestador")?,
            nome: texto_opcional(objeto, "nome", "prestador")?,
            email: texto_opcional(objeto, "email", "prestador")?,
            regime: RegimeTributario {
                op_simples_nacional: simples,
                regime_apuracao: apuracao,
                regime_especial,
            },
            percentual_simples_nacional,
            ver_aplic: texto_opcional(objeto, "ver_aplic", "prestador")?,
        })
    }

    /// Monta o `Prestador` da DPS para o CNPJ da empresa.
    pub fn prestador(&self, empresa: &Cnpj) -> Result<Prestador, ErroNfse> {
        Ok(Prestador {
            inscricao: InscricaoFederal::Cnpj(*empresa),
            inscricao_municipal: self.inscricao_municipal.clone(),
            nome: self.nome.clone(),
            email: self.email.clone(),
            regime: self.regime.clone(),
        })
    }

    /// Série validada para a API.
    pub fn serie(&self) -> Result<SerieDps, ErroNfse> {
        SerieDps::new_api(self.serie)
    }
}

/// Pedido de emissão de uma NFS-e.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PedidoEmissao {
    /// Chave de idempotência escolhida pelo cliente (por exemplo, o id da
    /// venda). Repetir o pedido com a mesma chave devolve a mesma operação.
    pub idempotencia: Option<String>,
    /// Configuração do prestador.
    pub prestador: PrestadorConfig,
    /// Série; quando ausente, usa a série do prestador.
    pub serie: Option<u32>,
    /// Competência; quando ausente, usa a data do `dhEmi`.
    pub competencia: Option<DataFiscal>,
    /// Código de tributação nacional (6 dígitos).
    pub codigo_servico: String,
    /// Descrição do serviço.
    pub descricao: String,
    /// Valor do serviço.
    pub valor: Dinheiro,
    /// Tomador, opcional.
    pub tomador: Option<Pessoa>,
    /// Local da prestação; padrão é o município do prestador.
    pub local_prestacao: Option<LocalPrestacao>,
    /// Código de tributação municipal (3 dígitos).
    pub codigo_tributacao_municipal: Option<String>,
    /// Código NBS (9 dígitos).
    pub codigo_nbs: Option<String>,
    /// Código interno do contribuinte.
    pub codigo_interno: Option<String>,
    /// Informações complementares.
    pub informacoes_complementares: Option<String>,
    /// Alíquota do ISSQN, quando o município não a parametriza.
    pub aliquota: Option<Decimal>,
    /// Tributação do ISSQN.
    pub trib_issqn: TribIssqn,
    /// Retenção do ISSQN.
    pub tipo_retencao: TipoRetencaoIssqn,
    /// Total aproximado de tributos; quando ausente, é derivado do regime.
    pub total_tributos: Option<TotalTributos>,
}

impl PedidoEmissao {
    /// Defasagem padrão de `dhEmi` em relação ao relógio local, em segundos.
    ///
    /// O Emissor Nacional usa 60 segundos; a folga evita que a data de emissão
    /// fique à frente do relógio do servidor do governo.
    pub const DEFASAGEM_DH_EMI_SEGUNDOS: u64 = 60;

    /// Lê um pedido em que o prestador vem no próprio JSON.
    pub fn de_json(valor: &Value) -> Result<Self, ErroNfse> {
        Self::de_json_com_prestador(valor, None)
    }

    /// Lê um pedido, usando `prestador` como fallback quando o JSON não traz o
    /// bloco `prestador`.
    ///
    /// Aceita o formato plano (campos na raiz) e o aninhado em `"pedido"`.
    pub fn de_json_com_prestador(
        valor: &Value,
        prestador: Option<&Value>,
    ) -> Result<Self, ErroNfse> {
        let raiz = objeto(valor, "pedido")?;
        let pedido = match raiz.get("pedido") {
            Some(aninhado) => objeto(aninhado, "pedido")?,
            None => raiz,
        };
        let prestador_json = pedido
            .get("prestador")
            .filter(|valor| !valor.is_null())
            .or_else(|| raiz.get("prestador").filter(|valor| !valor.is_null()))
            .or_else(|| prestador.filter(|valor| !valor.is_null()))
            .ok_or_else(|| {
                ErroNfse::campo(
                    "prestador",
                    "informe o bloco prestador no pedido ou use --prestador",
                )
            })?;
        let prestador = PrestadorConfig::de_json(prestador_json)?;
        let idempotencia = texto_opcional(pedido, "idempotencia", "pedido")?
            .or(texto_opcional(pedido, "id", "pedido")?)
            .or(texto_opcional(raiz, "idempotencia", "pedido")?)
            .or(texto_opcional(raiz, "id", "pedido")?);
        let serie = numero_u32_opcional(pedido, "serie")?;
        if let Some(serie) = serie {
            SerieDps::new_api(serie)?;
        }
        let competencia = match pedido.get("competencia") {
            Some(valor) => Some(DataFiscal::parse(&texto(valor, "pedido/competencia")?)?),
            None => None,
        };
        let codigo_servico = texto_obrigatorio(pedido, "servico", "pedido")?;
        let descricao = texto_obrigatorio(pedido, "descricao", "pedido")?;
        let valor = valor_dinheiro_do_pedido(pedido)?;
        let tomador = match pedido.get("tomador") {
            Some(Value::Null) | None => None,
            Some(valor) => Some(pessoa(valor, "pedido/tomador")?),
        };
        let local_prestacao = match pedido.get("local_prestacao") {
            Some(Value::Null) | None => None,
            Some(valor) => Some(local_prestacao(valor)?),
        };
        let aliquota = pedido
            .get("aliquota")
            .map(|valor| decimal(valor, "pedido/aliquota"))
            .transpose()?;
        let trib_issqn = match pedido.get("trib_issqn") {
            Some(valor) => parse_trib_issqn(valor)?,
            None => TribIssqn::Tributavel,
        };
        let tipo_retencao = match pedido.get("retencao_issqn") {
            Some(valor) => parse_retencao(valor)?,
            None => TipoRetencaoIssqn::NaoRetido,
        };
        let total_tributos = pedido
            .get("total_tributos")
            .map(total_tributos)
            .transpose()?;
        Ok(Self {
            idempotencia,
            prestador,
            serie,
            competencia,
            codigo_servico,
            descricao,
            valor,
            tomador,
            local_prestacao,
            codigo_tributacao_municipal: texto_opcional(
                pedido,
                "codigo_tributacao_municipal",
                "pedido",
            )?,
            codigo_nbs: texto_opcional(pedido, "codigo_nbs", "pedido")?,
            codigo_interno: texto_opcional(pedido, "codigo_interno", "pedido")?,
            informacoes_complementares: texto_opcional(
                pedido,
                "informacoes_complementares",
                "pedido",
            )?,
            aliquota,
            trib_issqn,
            tipo_retencao,
            total_tributos,
        })
    }

    /// Série efetiva: a do pedido ou a do prestador.
    pub fn serie(&self) -> Result<SerieDps, ErroNfse> {
        match self.serie {
            Some(serie) => SerieDps::new_api(serie),
            None => self.prestador.serie(),
        }
    }

    /// Monta a DPS para o número, ambiente e instante informados.
    pub fn montar(
        &self,
        empresa: &Cnpj,
        ambiente: TpAmb,
        numero: NumeroDps,
        dh_emi: DataHoraFiscal,
    ) -> Result<Dps, ErroNfse> {
        let prestador = self.prestador.prestador(empresa)?;
        let total_tributos = match self.total_tributos {
            Some(total) => total,
            None => match prestador.regime.op_simples_nacional {
                OpSimplesNacional::Mei => TotalTributos::IndicadorZero,
                OpSimplesNacional::MeEpp => {
                    let percentual =
                        self.prestador.percentual_simples_nacional.ok_or_else(|| {
                            ErroNfse::campo(
                                "total_tributos",
                                "ME/EPP exige percentual_simples_nacional no prestador ou total_tributos no pedido",
                            )
                        })?;
                    TotalTributos::PercentualSimplesNacional(percentual)
                }
                OpSimplesNacional::NaoOptante => {
                    return Err(ErroNfse::campo(
                        "total_tributos",
                        "fora do Simples exige total_tributos com vTotTrib ou pTotTrib (E0713)",
                    ));
                }
            },
        };
        let local_prestacao = self
            .local_prestacao
            .clone()
            .unwrap_or_else(|| LocalPrestacao::Municipio(self.prestador.municipio.clone()));
        Ok(Dps {
            ambiente,
            dh_emi,
            ver_aplic: self
                .prestador
                .ver_aplic
                .clone()
                .unwrap_or_else(|| format!("carimbar-{}", env!("CARGO_PKG_VERSION"))),
            serie: self.serie()?,
            numero,
            d_compet: self.competencia.unwrap_or_else(|| dh_emi.data()),
            tp_emit: TipoEmitente::Prestador,
            c_motivo_emis_ti: None,
            ch_nfse_rej: None,
            c_loc_emi: self.prestador.municipio.clone(),
            substituicao: None,
            prestador,
            tomador: self.tomador.clone(),
            intermediario: None,
            servico: Servico {
                local_prestacao,
                codigo_tributacao_nacional: self.codigo_servico.clone(),
                codigo_tributacao_municipal: self.codigo_tributacao_municipal.clone(),
                codigo_nbs: self.codigo_nbs.clone(),
                codigo_interno: self.codigo_interno.clone(),
                descricao: self.descricao.clone(),
                informacoes_complementares: self.informacoes_complementares.clone(),
            },
            valores: Valores {
                valor_recebido: None,
                valor_servico: self.valor,
                desconto_incondicional: None,
                desconto_condicional: None,
                tributo_municipal: TributoMunicipal {
                    trib_issqn: self.trib_issqn,
                    tipo_retencao: self.tipo_retencao,
                    aliquota: self.aliquota,
                },
                total_tributos,
            },
            ibs_cbs: None,
        })
    }
}

fn objeto<'a>(valor: &'a Value, campo: &'static str) -> Result<&'a Map<String, Value>, ErroNfse> {
    valor
        .as_object()
        .ok_or_else(|| ErroNfse::campo(campo, "esperado um objeto JSON"))
}

fn texto(valor: &Value, campo: &'static str) -> Result<String, ErroNfse> {
    match valor {
        Value::String(texto) => Ok(texto.trim().to_owned()),
        Value::Number(numero) => Ok(numero.to_string()),
        _ => Err(ErroNfse::campo(campo, "esperado texto")),
    }
}

fn texto_obrigatorio(
    objeto: &Map<String, Value>,
    nome: &str,
    contexto: &'static str,
) -> Result<String, ErroNfse> {
    let valor = objeto
        .get(nome)
        .ok_or_else(|| ErroNfse::campo(contexto, format!("campo obrigatório ausente: {nome}")))?;
    let texto = texto(valor, contexto)?;
    if texto.is_empty() {
        return Err(ErroNfse::campo(
            contexto,
            format!("campo obrigatório vazio: {nome}"),
        ));
    }
    Ok(texto)
}

fn texto_opcional(
    objeto: &Map<String, Value>,
    nome: &str,
    contexto: &'static str,
) -> Result<Option<String>, ErroNfse> {
    match objeto.get(nome) {
        None | Some(Value::Null) => Ok(None),
        Some(valor) => {
            let texto = texto(valor, contexto)?;
            Ok((!texto.is_empty()).then_some(texto))
        }
    }
}

fn numero_u32_opcional(objeto: &Map<String, Value>, nome: &str) -> Result<Option<u32>, ErroNfse> {
    match objeto.get(nome) {
        None | Some(Value::Null) => Ok(None),
        Some(valor) => {
            let numero = valor
                .as_u64()
                .or_else(|| valor.as_str().and_then(|texto| texto.parse().ok()))
                .ok_or_else(|| ErroNfse::campo("pedido", format!("{nome} deve ser inteiro")))?;
            u32::try_from(numero)
                .map(Some)
                .map_err(|_| ErroNfse::campo("pedido", format!("{nome} fora do intervalo")))
        }
    }
}

fn decimal(valor: &Value, campo: &'static str) -> Result<Decimal, ErroNfse> {
    let texto = match valor {
        Value::String(texto) => texto.trim().to_owned(),
        Value::Number(numero) => numero.to_string(),
        _ => return Err(ErroNfse::campo(campo, "esperado número decimal ou texto")),
    };
    Decimal::from_str(&texto)
        .map_err(|_| ErroNfse::campo(campo, format!("decimal inválido: {texto:?}")))
}

fn dinheiro(valor: &Value, campo: &'static str) -> Result<Dinheiro, ErroNfse> {
    let texto = match valor {
        Value::String(texto) => texto.trim().to_owned(),
        Value::Number(numero) => numero.to_string(),
        _ => return Err(ErroNfse::campo(campo, "esperado valor monetário")),
    };
    texto
        .parse::<Dinheiro>()
        .map_err(|erro| ErroNfse::campo(campo, erro.to_string()))
}

fn valor_dinheiro_do_pedido(pedido: &Map<String, Value>) -> Result<Dinheiro, ErroNfse> {
    if let Some(valor) = pedido.get("valor_centavos") {
        let centavos = valor
            .as_u64()
            .or_else(|| valor.as_str().and_then(|texto| texto.parse().ok()))
            .ok_or_else(|| ErroNfse::campo("pedido/valor_centavos", "esperado inteiro"))?;
        return Ok(Dinheiro::from_centavos(centavos));
    }
    let valor = pedido
        .get("valor")
        .ok_or_else(|| ErroNfse::campo("pedido", "informe valor_centavos ou valor"))?;
    dinheiro(valor, "pedido/valor")
}

fn pessoa(valor: &Value, campo: &'static str) -> Result<Pessoa, ErroNfse> {
    let objeto = objeto(valor, campo)?;
    let inscricao = objeto
        .get("inscricao")
        .or_else(|| {
            objeto
                .get("cnpj")
                .filter(|valor| !matches!(valor, Value::Null))
        })
        .or_else(|| {
            objeto
                .get("cpf")
                .filter(|valor| !matches!(valor, Value::Null))
        })
        .ok_or_else(|| ErroNfse::campo(campo, "informe cnpj, cpf ou inscricao"))?;
    let inscricao = InscricaoFederal::parse(&texto(inscricao, campo)?)?;
    Ok(Pessoa {
        inscricao,
        inscricao_municipal: texto_opcional(objeto, "inscricao_municipal", campo)?,
        nome: texto_opcional(objeto, "nome", campo)?,
        email: texto_opcional(objeto, "email", campo)?,
    })
}

fn local_prestacao(valor: &Value) -> Result<LocalPrestacao, ErroNfse> {
    let objeto = objeto(valor, "pedido/local_prestacao")?;
    if let Some(municipio) = objeto.get("municipio") {
        return Ok(LocalPrestacao::Municipio(CodigoMunicipio::new(texto(
            municipio,
            "pedido/local_prestacao/municipio",
        )?)?));
    }
    if let Some(pais) = objeto.get("pais") {
        let pais = texto(pais, "pedido/local_prestacao/pais")?;
        if pais.len() != 2 || !pais.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(ErroNfse::campo(
                "pedido/local_prestacao/pais",
                "esperado código ISO de 2 letras maiúsculas",
            ));
        }
        return Ok(LocalPrestacao::Pais(pais));
    }
    Err(ErroNfse::campo(
        "pedido/local_prestacao",
        "informe municipio ou pais",
    ))
}

fn parse_simples(valor: &Value) -> Result<OpSimplesNacional, ErroNfse> {
    match valor {
        Value::Number(numero) => match numero.as_u64() {
            Some(1) => Ok(OpSimplesNacional::NaoOptante),
            Some(2) => Ok(OpSimplesNacional::Mei),
            Some(3) => Ok(OpSimplesNacional::MeEpp),
            _ => Err(ErroNfse::campo(
                "prestador/simples",
                "esperado 1 (não optante), 2 (MEI) ou 3 (ME/EPP)",
            )),
        },
        Value::String(texto) => match texto.trim().to_ascii_lowercase().as_str() {
            "1" | "nao_optante" | "não optante" => Ok(OpSimplesNacional::NaoOptante),
            "2" | "mei" => Ok(OpSimplesNacional::Mei),
            "3" | "me_epp" | "me/epp" | "me epp" => Ok(OpSimplesNacional::MeEpp),
            _ => Err(ErroNfse::campo(
                "prestador/simples",
                "esperado 1 (não optante), 2 (MEI) ou 3 (ME/EPP)",
            )),
        },
        _ => Err(ErroNfse::campo("prestador/simples", "esperado 1, 2 ou 3")),
    }
}

fn parse_apuracao(valor: &Value) -> Result<RegimeApuracaoSimples, ErroNfse> {
    match codigo_u8(valor, "prestador/apuracao")? {
        1 => Ok(RegimeApuracaoSimples::SimplesNacional),
        2 => Ok(RegimeApuracaoSimples::FederalSimplesIssFora),
        3 => Ok(RegimeApuracaoSimples::ForaSimplesNacional),
        outro => Err(ErroNfse::campo(
            "prestador/apuracao",
            format!("esperado 1, 2 ou 3; recebido {outro}"),
        )),
    }
}

fn parse_regime_especial(valor: &Value) -> Result<RegimeEspecialTributacao, ErroNfse> {
    match codigo_u8(valor, "prestador/regime_especial")? {
        0 => Ok(RegimeEspecialTributacao::Nenhum),
        1 => Ok(RegimeEspecialTributacao::AtoCooperado),
        2 => Ok(RegimeEspecialTributacao::Estimativa),
        3 => Ok(RegimeEspecialTributacao::MicroempresaMunicipal),
        4 => Ok(RegimeEspecialTributacao::Notario),
        5 => Ok(RegimeEspecialTributacao::ProfissionalAutonomo),
        6 => Ok(RegimeEspecialTributacao::SociedadeProfissionais),
        9 => Ok(RegimeEspecialTributacao::Outros),
        outro => Err(ErroNfse::campo(
            "prestador/regime_especial",
            format!("código inválido: {outro}"),
        )),
    }
}

fn parse_trib_issqn(valor: &Value) -> Result<TribIssqn, ErroNfse> {
    match codigo_u8(valor, "pedido/trib_issqn")? {
        1 => Ok(TribIssqn::Tributavel),
        2 => Ok(TribIssqn::Imunidade),
        3 => Ok(TribIssqn::Exportacao),
        4 => Ok(TribIssqn::NaoIncidencia),
        outro => Err(ErroNfse::campo(
            "pedido/trib_issqn",
            format!("código inválido: {outro}"),
        )),
    }
}

fn parse_retencao(valor: &Value) -> Result<TipoRetencaoIssqn, ErroNfse> {
    match codigo_u8(valor, "pedido/retencao_issqn")? {
        1 => Ok(TipoRetencaoIssqn::NaoRetido),
        2 => Ok(TipoRetencaoIssqn::Tomador),
        3 => Ok(TipoRetencaoIssqn::Intermediario),
        outro => Err(ErroNfse::campo(
            "pedido/retencao_issqn",
            format!("código inválido: {outro}"),
        )),
    }
}

fn codigo_u8(valor: &Value, campo: &'static str) -> Result<u8, ErroNfse> {
    valor
        .as_u64()
        .or_else(|| valor.as_str().and_then(|texto| texto.parse().ok()))
        .and_then(|numero| u8::try_from(numero).ok())
        .ok_or_else(|| ErroNfse::campo(campo, "esperado código inteiro"))
}

fn total_tributos(valor: &Value) -> Result<TotalTributos, ErroNfse> {
    if matches!(valor, Value::String(_) | Value::Number(_)) {
        return Ok(TotalTributos::PercentualSimplesNacional(decimal(
            valor,
            "pedido/total_tributos",
        )?));
    }
    let mapa = objeto(valor, "pedido/total_tributos")?;
    if let Some(percentual) = mapa.get("percentual_simples_nacional") {
        return Ok(TotalTributos::PercentualSimplesNacional(decimal(
            percentual,
            "pedido/total_tributos/percentual_simples_nacional",
        )?));
    }
    if mapa
        .get("indicador_zero")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(TotalTributos::IndicadorZero);
    }
    if let Some(valor) = mapa.get("valores") {
        let valores = objeto(valor, "pedido/total_tributos/valores")?;
        return Ok(TotalTributos::Valores {
            federal: dinheiro(
                valores.get("federal").ok_or_else(|| {
                    ErroNfse::campo("pedido/total_tributos/valores", "federal ausente")
                })?,
                "pedido/total_tributos/valores/federal",
            )?,
            estadual: dinheiro(
                valores.get("estadual").ok_or_else(|| {
                    ErroNfse::campo("pedido/total_tributos/valores", "estadual ausente")
                })?,
                "pedido/total_tributos/valores/estadual",
            )?,
            municipal: dinheiro(
                valores.get("municipal").ok_or_else(|| {
                    ErroNfse::campo("pedido/total_tributos/valores", "municipal ausente")
                })?,
                "pedido/total_tributos/valores/municipal",
            )?,
        });
    }
    if let Some(valor) = mapa.get("percentuais") {
        let percentuais = objeto(valor, "pedido/total_tributos/percentuais")?;
        return Ok(TotalTributos::Percentuais {
            federal: decimal(
                percentuais.get("federal").ok_or_else(|| {
                    ErroNfse::campo("pedido/total_tributos/percentuais", "federal ausente")
                })?,
                "pedido/total_tributos/percentuais/federal",
            )?,
            estadual: decimal(
                percentuais.get("estadual").ok_or_else(|| {
                    ErroNfse::campo("pedido/total_tributos/percentuais", "estadual ausente")
                })?,
                "pedido/total_tributos/percentuais/estadual",
            )?,
            municipal: decimal(
                percentuais.get("municipal").ok_or_else(|| {
                    ErroNfse::campo("pedido/total_tributos/percentuais", "municipal ausente")
                })?,
                "pedido/total_tributos/percentuais/municipal",
            )?,
        });
    }
    Err(ErroNfse::campo(
        "pedido/total_tributos",
        "informe percentual_simples_nacional, indicador_zero, valores ou percentuais",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn empresa() -> Cnpj {
        Cnpj::parse("00000001000136").expect("CNPJ")
    }

    fn pedido(valor: Value) -> PedidoEmissao {
        PedidoEmissao::de_json(&valor).expect("pedido")
    }

    fn dh() -> DataHoraFiscal {
        DataHoraFiscal::parse("2026-11-13T12:00:00+00:00").expect("dhEmi")
    }

    #[test]
    fn monta_dps_minima_para_mei() {
        let pedido = pedido(json!({
            "prestador": {"municipio": "3550308", "simples": 2},
            "id": "venda-1",
            "servico": "010101",
            "descricao": "Servico sintetico",
            "valor_centavos": 10000,
            "tomador": {"cnpj": "00000002000008", "nome": "Cliente"}
        }));
        let dps = pedido
            .montar(
                &empresa(),
                TpAmb::Homologacao,
                NumeroDps::new(1).expect("número"),
                dh(),
            )
            .expect("DPS");
        assert_eq!(dps.serie.get(), 1);
        assert_eq!(dps.numero.get(), 1);
        assert_eq!(dps.valores.valor_servico.para_xml(), "100.00");
        assert_eq!(dps.valores.total_tributos, TotalTributos::IndicadorZero);
        assert_eq!(
            dps.d_compet.to_string(),
            dh().data().to_string(),
            "competência padrão é a data do dhEmi"
        );
        assert!(
            dps.para_xml()
                .expect("XML")
                .contains("<indTotTrib>0</indTotTrib>")
        );
    }

    #[test]
    fn me_epp_usa_percentual_do_prestador() {
        let pedido = pedido(json!({
            "prestador": {
                "municipio": "3550308",
                "simples": 3,
                "apuracao": 1,
                "percentual_simples_nacional": "6.00"
            },
            "servico": "010101",
            "descricao": "Servico",
            "valor": "10.00"
        }));
        let dps = pedido
            .montar(
                &empresa(),
                TpAmb::Homologacao,
                NumeroDps::new(1).expect("número"),
                dh(),
            )
            .expect("DPS");
        assert_eq!(
            dps.valores.total_tributos,
            TotalTributos::PercentualSimplesNacional(Decimal::new(600, 2))
        );
    }

    #[test]
    fn nao_optante_exige_total_detalhado() {
        let pedido = pedido(json!({
            "prestador": {"municipio": "3550308", "simples": 1},
            "servico": "010101",
            "descricao": "Servico",
            "valor": "10.00"
        }));
        let erro = pedido
            .montar(
                &empresa(),
                TpAmb::Homologacao,
                NumeroDps::new(1).expect("número"),
                dh(),
            )
            .expect_err("E0713");
        assert!(erro.to_string().contains("E0713"));
    }

    #[test]
    fn aceita_total_detalhado_e_tomador_com_cpf() {
        let pedido = pedido(json!({
            "prestador": {"municipio": "3550308", "simples": 1},
            "servico": "010101",
            "descricao": "Servico",
            "valor_centavos": 500,
            "tomador": {"cpf": "52998224725", "email": "cliente@exemplo.com"},
            "total_tributos": {"valores": {"federal": "0.10", "estadual": "0.00", "municipal": "0.20"}}
        }));
        let dps = pedido
            .montar(
                &empresa(),
                TpAmb::Homologacao,
                NumeroDps::new(1).expect("número"),
                dh(),
            )
            .expect("DPS");
        assert!(matches!(
            dps.valores.total_tributos,
            TotalTributos::Valores { .. }
        ));
        assert_eq!(
            dps.tomador.expect("tomador").inscricao.as_str(),
            "52998224725"
        );
    }

    #[test]
    fn formato_aninhado_e_idempotencia_sao_aceitos() {
        let pedido = pedido(json!({
            "prestador": {"municipio": "3550308", "simples": 2},
            "pedido": {
                "id": "venda-9",
                "servico": "010101",
                "descricao": "Servico",
                "valor_centavos": 100
            }
        }));
        assert_eq!(pedido.idempotencia.as_deref(), Some("venda-9"));
        assert_eq!(pedido.serie().expect("série").get(), 1);
    }

    #[test]
    fn prestador_externo_e_usado_quando_ausente_no_pedido() {
        let pedido = PedidoEmissao::de_json_com_prestador(
            &json!({
                "servico": "010101",
                "descricao": "Servico",
                "valor_centavos": 100
            }),
            Some(&json!({"municipio": "3550308", "simples": 2})),
        )
        .expect("pedido");
        assert_eq!(pedido.prestador.municipio.as_str(), "3550308");
    }

    #[test]
    fn serie_fora_da_faixa_da_api_e_recusada() {
        let erro = PedidoEmissao::de_json(&json!({
            "prestador": {"municipio": "3550308", "simples": 2, "serie": 50000},
            "servico": "010101",
            "descricao": "Servico",
            "valor_centavos": 100
        }))
        .expect_err("E0010");
        assert!(erro.to_string().contains("1 a 49999"));
    }
}
