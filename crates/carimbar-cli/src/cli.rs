//! Implementação dos comandos da CLI.

use crate::replay::ReplayExecutor;
use carimbar::{Emissor, ErroEmissor, Journal, ResultadoNfse};
use carimbar_nfse::{
    ChaveNfse, DataHoraFiscal, ErroNfse, EventoCancelamento, Executor, InscricaoFederal,
    MotivoCancelamento,
};
use carimbar_transport::{ClienteMtls, ErroEnvio, ErroTransporte, Opcoes, Requisicao, Resposta};
use carimbar_types::{Cnpj, ConfirmacaoProducao};
use carimbar_vault::{ChaveMestra, Cofre};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const CODIGO_OPERACIONAL: u8 = 1;
const CODIGO_USO: u8 = 2;
const CODIGO_FISCAL: u8 = 3;
const CODIGO_TRANSPORTE: u8 = 4;
const CODIGO_PRODUCAO: u8 = 5;
const CODIGO_CONFLITO: u8 = 6;

/// Erro com código de saída estável.
#[derive(Debug)]
pub struct ErroCli {
    mensagem: String,
    codigo: u8,
}

impl ErroCli {
    fn operacional(mensagem: impl Into<String>) -> Self {
        Self {
            mensagem: mensagem.into(),
            codigo: CODIGO_OPERACIONAL,
        }
    }
    fn uso(mensagem: impl Into<String>) -> Self {
        Self {
            mensagem: mensagem.into(),
            codigo: CODIGO_USO,
        }
    }
    fn fiscal(mensagem: impl Into<String>) -> Self {
        Self {
            mensagem: mensagem.into(),
            codigo: CODIGO_FISCAL,
        }
    }
    fn transporte(mensagem: impl Into<String>) -> Self {
        Self {
            mensagem: mensagem.into(),
            codigo: CODIGO_TRANSPORTE,
        }
    }
    fn producao(mensagem: impl Into<String>) -> Self {
        Self {
            mensagem: mensagem.into(),
            codigo: CODIGO_PRODUCAO,
        }
    }
    fn conflito(mensagem: impl Into<String>) -> Self {
        Self {
            mensagem: mensagem.into(),
            codigo: CODIGO_CONFLITO,
        }
    }
    /// Código de saída.
    pub fn codigo(&self) -> u8 {
        self.codigo
    }
}

impl fmt::Display for ErroCli {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.mensagem)
    }
}

impl From<ErroEmissor> for ErroCli {
    fn from(erro: ErroEmissor) -> Self {
        match &erro {
            ErroEmissor::ProducaoBloqueada => Self::producao(erro.to_string()),
            ErroEmissor::ConflitoIdempotencia { .. } => Self::conflito(erro.to_string()),
            ErroEmissor::Nfse(ErroNfse::AntesDoEnvio(_))
            | ErroEmissor::Nfse(ErroNfse::ResultadoDesconhecido(_))
            | ErroEmissor::Nfse(ErroNfse::Transporte(_)) => Self::transporte(erro.to_string()),
            _ => Self::operacional(erro.to_string()),
        }
    }
}

impl From<ErroNfse> for ErroCli {
    fn from(erro: ErroNfse) -> Self {
        match &erro {
            ErroNfse::AntesDoEnvio(_)
            | ErroNfse::ResultadoDesconhecido(_)
            | ErroNfse::Transporte(_) => Self::transporte(erro.to_string()),
            _ => Self::operacional(erro.to_string()),
        }
    }
}

impl From<carimbar_vault::ErroCofre> for ErroCli {
    fn from(erro: carimbar_vault::ErroCofre) -> Self {
        Self::operacional(erro.to_string())
    }
}

impl From<carimbar_transport::ErroTransporte> for ErroCli {
    fn from(erro: carimbar_transport::ErroTransporte) -> Self {
        Self::operacional(erro.to_string())
    }
}

#[derive(Parser, Debug)]
#[command(
    name = "carimbar",
    version,
    about = "Emissão de NFS-e/NF-e e administração do cofre"
)]
struct Cli {
    /// Caminho do cofre SQLite.
    #[arg(
        long,
        global = true,
        env = "CARIMBAR_COFRE",
        default_value = "carimbar-cofre.db"
    )]
    cofre: PathBuf,
    /// Caminho do journal SQLite.
    #[arg(
        long,
        global = true,
        env = "CARIMBAR_JOURNAL",
        default_value = "carimbar-journal.db"
    )]
    journal: PathBuf,
    /// CNPJ da empresa.
    #[arg(long, global = true)]
    empresa: Option<Cnpj>,
    /// Saída em JSON.
    #[arg(long, global = true)]
    json: bool,
    /// Diretório com raízes de confiança em DER.
    #[arg(long, global = true, env = "CARIMBAR_RAIZES")]
    raizes: Option<PathBuf>,
    /// Identificador do certificado no cofre.
    #[arg(long, global = true, env = "CARIMBAR_CERTIFICADO")]
    certificado: Option<String>,
    /// Respostas simuladas (somente build de depuração).
    #[arg(long, global = true, hide = true)]
    replay: Option<PathBuf>,
    #[command(subcommand)]
    comando: Comando,
}

#[derive(Debug, Clone)]
struct Config {
    cofre: PathBuf,
    journal: PathBuf,
    json: bool,
    raizes: Option<PathBuf>,
    certificado: Option<String>,
    replay: Option<PathBuf>,
}

impl Config {
    fn de_cli(cli: &Cli) -> Self {
        Self {
            cofre: cli.cofre.clone(),
            journal: cli.journal.clone(),
            json: cli.json,
            raizes: cli.raizes.clone(),
            certificado: cli.certificado.clone(),
            replay: cli.replay.clone(),
        }
    }
}

#[derive(Subcommand, Debug)]
enum Comando {
    /// Administração de empresas e certificados.
    Empresa {
        #[command(subcommand)]
        acao: EmpresaAcao,
    },
    /// Emite uma DPS em XML.
    Emitir {
        /// Arquivo XML da DPS.
        #[arg(long)]
        dps: PathBuf,
        /// Onde gravar o XML autorizado.
        #[arg(long)]
        xml_saida: Option<PathBuf>,
        /// Onde gravar o DANFSe.
        #[arg(long)]
        pdf_saida: Option<PathBuf>,
    },
    /// Consulta uma NFS-e pela chave.
    Consultar {
        /// Chave de acesso.
        #[arg(long)]
        chave: String,
        /// Onde gravar o XML.
        #[arg(long)]
        saida: Option<PathBuf>,
    },
    /// Cancela uma NFS-e.
    Cancelar {
        /// Chave de acesso.
        #[arg(long)]
        chave: String,
        /// 1 erro na emissão, 2 serviço não prestado, 9 outros.
        #[arg(long, default_value_t = 1)]
        motivo: u8,
        /// Descrição do motivo (15 a 255 caracteres).
        #[arg(long)]
        descricao: String,
    },
    /// Gera o DANFSe de uma operação autorizada.
    Danfse {
        /// Identificador local da operação.
        #[arg(long)]
        operacao: i64,
        /// Arquivo PDF de saída.
        #[arg(long)]
        saida: PathBuf,
    },
    /// Distribuição ADN.
    Adn {
        #[command(subcommand)]
        acao: AdnAcao,
    },
    /// Parâmetros municipais.
    Parametros {
        #[command(subcommand)]
        acao: ParametrosAcao,
    },
}

#[derive(Subcommand, Debug)]
enum EmpresaAcao {
    /// Importa um certificado A1. A senha vem de CARIMBAR_SENHA_PFX.
    Importar {
        /// Identificador do certificado.
        #[arg(long)]
        id: String,
        /// Arquivo PKCS#12.
        #[arg(long)]
        pfx: PathBuf,
    },
    /// Lista os certificados da empresa.
    Listar,
    /// Promove a empresa para produção.
    Promover {
        /// Identificador do certificado.
        #[arg(long)]
        id: String,
        /// CNPJ confirmado para produção (precisa ser o mesmo da empresa).
        #[arg(long)]
        confirmar: Cnpj,
    },
    /// Devolve a empresa para homologação.
    Rebaixar,
}

#[derive(Subcommand, Debug)]
enum AdnAcao {
    /// Sincroniza documentos por NSU.
    Sincronizar {
        /// Máximo de páginas nesta rodada.
        #[arg(long, default_value_t = 10)]
        limite_paginas: u32,
    },
    /// Lista documentos já persistidos.
    Documentos,
}

#[derive(Subcommand, Debug)]
enum ParametrosAcao {
    /// Convênio do município.
    Convenio {
        /// Código IBGE do município.
        #[arg(long)]
        municipio: String,
        /// Ignora o cache.
        #[arg(long)]
        forcar: bool,
    },
    /// Alíquotas de um serviço.
    Aliquota {
        /// Código IBGE do município.
        #[arg(long)]
        municipio: String,
        /// Código de tributação nacional.
        #[arg(long)]
        servico: String,
        /// Competência AAAA-MM-DD.
        #[arg(long)]
        competencia: String,
        /// Ignora o cache.
        #[arg(long)]
        forcar: bool,
    },
    /// Retenções do município (JSON bruto).
    Retencoes {
        /// Código IBGE do município.
        #[arg(long)]
        municipio: String,
        /// Competência AAAA-MM-DD.
        #[arg(long)]
        competencia: String,
        /// Ignora o cache.
        #[arg(long)]
        forcar: bool,
    },
}

struct ExecutorIndisponivel;

impl Executor for ExecutorIndisponivel {
    fn executar(&self, _requisicao: &Requisicao) -> Result<Resposta, ErroEnvio> {
        Err(ErroEnvio::AntesDoEnvio(ErroTransporte::Configuracao(
            "transporte não configurado para este comando".to_owned(),
        )))
    }
}

/// Executa a CLI.
pub fn executar() -> Result<(), ErroCli> {
    let cli = Cli::parse();
    let config = Config::de_cli(&cli);
    let empresa = cli
        .empresa
        .ok_or_else(|| ErroCli::uso("--empresa é obrigatório"))?;
    let cofre = abrir_cofre(&config.cofre)?;

    match cli.comando {
        Comando::Empresa { acao } => executar_empresa(&config, cofre, empresa, acao),
        comando => executar_fiscal(&config, cofre, empresa, comando),
    }
}

fn executar_empresa(
    cli: &Config,
    mut cofre: Cofre,
    empresa: Cnpj,
    acao: EmpresaAcao,
) -> Result<(), ErroCli> {
    match acao {
        EmpresaAcao::Importar { id, pfx } => {
            let bytes = fs::read(&pfx).map_err(|erro| {
                ErroCli::uso(format!("não foi possível ler {}: {erro}", pfx.display()))
            })?;
            let senha = std::env::var("CARIMBAR_SENHA_PFX").map_err(|_| {
                ErroCli::uso("defina CARIMBAR_SENHA_PFX; a senha não é aceita em argumento")
            })?;
            let meta = cofre.importar(&empresa, &id, &bytes, &senha)?;
            imprimir(
                cli.json,
                &json!({
                    "id": meta.id,
                    "empresa": meta.empresa.as_str(),
                    "titular": meta.titular.as_str(),
                    "sha256": meta.sha256,
                    "valido_ate": formatar_tempo(meta.valido_ate),
                }),
                &format!(
                    "certificado {} importado para {} (sha256 {})",
                    id, empresa, meta.sha256
                ),
            );
            Ok(())
        }
        EmpresaAcao::Listar => {
            let lista = cofre.listar(&empresa)?;
            let valores = lista
                .iter()
                .map(|meta| {
                    json!({
                        "id": meta.id,
                        "titular": meta.titular.as_str(),
                        "sha256": meta.sha256,
                        "valido_de": formatar_tempo(meta.valido_de),
                        "valido_ate": formatar_tempo(meta.valido_ate),
                    })
                })
                .collect::<Vec<_>>();
            imprimir(
                cli.json,
                &Value::Array(valores),
                &format!("{} certificado(s) para {empresa}", lista.len()),
            );
            Ok(())
        }
        EmpresaAcao::Promover { id, confirmar } => {
            let journal = Journal::abrir(&cli.journal)?;
            let executor = ExecutorIndisponivel;
            let mut emissor = Emissor::novo(cofre, journal, empresa, &executor)?;
            emissor.promover_producao(ConfirmacaoProducao::new(confirmar), &id)?;
            imprimir(
                cli.json,
                &json!({"empresa": empresa.as_str(), "ambiente": "producao", "certificado": id}),
                &format!("{empresa} promovida para produção com o certificado {id}"),
            );
            Ok(())
        }
        EmpresaAcao::Rebaixar => {
            let journal = Journal::abrir(&cli.journal)?;
            let executor = ExecutorIndisponivel;
            let mut emissor = Emissor::novo(cofre, journal, empresa, &executor)?;
            emissor.rebaixar_homologacao()?;
            imprimir(
                cli.json,
                &json!({"empresa": empresa.as_str(), "ambiente": "homologacao"}),
                &format!("{empresa} voltou para homologação"),
            );
            Ok(())
        }
    }
}

fn executar_fiscal(
    cli: &Config,
    cofre: Cofre,
    empresa: Cnpj,
    comando: Comando,
) -> Result<(), ErroCli> {
    let executor = criar_executor(cli, &cofre, &empresa)?;
    let journal = Journal::abrir(&cli.journal)?;
    let mut emissor = Emissor::novo(cofre, journal, empresa, executor.as_ref())?;
    match comando {
        Comando::Emitir {
            dps,
            xml_saida,
            pdf_saida,
        } => {
            let xml = fs::read_to_string(&dps).map_err(|erro| {
                ErroCli::uso(format!("não foi possível ler {}: {erro}", dps.display()))
            })?;
            let preparada = emissor.preparar_nfse_xml(&xml)?;
            let operacao = preparada.id();
            match emissor.confirmar_nfse(operacao)? {
                ResultadoNfse::Autorizada {
                    chave, xml_nfse, ..
                } => {
                    if let Some(caminho) = &xml_saida {
                        fs::write(caminho, &xml_nfse).map_err(|erro| {
                            ErroCli::operacional(format!(
                                "não foi possível gravar {}: {erro}",
                                caminho.display()
                            ))
                        })?;
                    }
                    if let Some(caminho) = &pdf_saida {
                        let pdf = emissor.gerar_danfse(operacao)?;
                        fs::write(caminho, pdf).map_err(|erro| {
                            ErroCli::operacional(format!(
                                "não foi possível gravar {}: {erro}",
                                caminho.display()
                            ))
                        })?;
                    }
                    imprimir(
                        cli.json,
                        &json!({
                            "operacao": operacao,
                            "estado": "autorizada",
                            "chave": chave.as_str(),
                            "xml_saida": xml_saida.map(|p| p.display().to_string()),
                            "pdf_saida": pdf_saida.map(|p| p.display().to_string()),
                        }),
                        &format!("NFS-e autorizada: {chave} (operação {operacao})"),
                    );
                    Ok(())
                }
                ResultadoNfse::Rejeitada { detalhe, .. } => {
                    Err(ErroCli::fiscal(format!("NFS-e rejeitada: {detalhe}")))
                }
                ResultadoNfse::Desconhecido { detalhe, .. } => Err(ErroCli::transporte(format!(
                    "resultado desconhecido na operação {operacao}: {detalhe}"
                ))),
                ResultadoNfse::NaoEncontrada { detalhe, .. } => Err(ErroCli::transporte(format!(
                    "operação {operacao} não encontrada na reconciliação: {detalhe}"
                ))),
                ResultadoNfse::Cancelada { .. } => Err(ErroCli::operacional(
                    "operação de emissão retornou cancelamento",
                )),
            }
        }
        Comando::Consultar { chave, saida } => {
            let chave = ChaveNfse::parse(&chave, emissor.ambiente().tp_amb())?;
            let xml = emissor.consultar_nfse(&chave)?;
            if let Some(caminho) = &saida {
                fs::write(caminho, &xml).map_err(|erro| {
                    ErroCli::operacional(format!(
                        "não foi possível gravar {}: {erro}",
                        caminho.display()
                    ))
                })?;
            }
            imprimir(
                cli.json,
                &json!({"chave": chave.as_str(), "xml": xml}),
                &xml,
            );
            Ok(())
        }
        Comando::Cancelar {
            chave,
            motivo,
            descricao,
        } => {
            let ambiente = emissor.ambiente().tp_amb();
            let evento = EventoCancelamento {
                ambiente,
                chave: ChaveNfse::parse(&chave, ambiente)?,
                autor: InscricaoFederal::parse(empresa.as_str())
                    .map_err(|erro| ErroCli::operacional(erro.to_string()))?,
                ver_aplic: versao_aplic(),
                dh_evento: DataHoraFiscal::parse(&agora_fiscal()?)
                    .map_err(|erro| ErroCli::operacional(erro.to_string()))?,
                motivo: motivo_cancelamento(motivo)?,
                descricao,
            };
            match emissor.executar_cancelamento(&evento)? {
                ResultadoNfse::Cancelada { operacao, .. } => {
                    imprimir(
                        cli.json,
                        &json!({"operacao": operacao, "estado": "cancelada", "chave": chave}),
                        &format!("cancelamento registrado para {chave} (operação {operacao})"),
                    );
                    Ok(())
                }
                ResultadoNfse::Rejeitada { detalhe, .. } => Err(ErroCli::fiscal(format!(
                    "cancelamento rejeitado: {detalhe}"
                ))),
                ResultadoNfse::Desconhecido { detalhe, .. } => Err(ErroCli::transporte(format!(
                    "resultado desconhecido no cancelamento: {detalhe}"
                ))),
                outro => Err(ErroCli::operacional(format!(
                    "resultado inesperado do cancelamento: {outro:?}"
                ))),
            }
        }
        Comando::Danfse { operacao, saida } => {
            let pdf = emissor.gerar_danfse(operacao)?;
            fs::write(&saida, pdf).map_err(|erro| {
                ErroCli::operacional(format!(
                    "não foi possível gravar {}: {erro}",
                    saida.display()
                ))
            })?;
            imprimir(
                cli.json,
                &json!({"operacao": operacao, "pdf": saida.display().to_string()}),
                &format!("DANFSe gravado em {}", saida.display()),
            );
            Ok(())
        }
        Comando::Adn { acao } => match acao {
            AdnAcao::Sincronizar { limite_paginas } => {
                let resultado = emissor.sincronizar_adn(limite_paginas)?;
                imprimir(
                    cli.json,
                    &json!({
                        "inicio_nsu": resultado.inicio_nsu,
                        "ultimo_nsu": resultado.ultimo_nsu,
                        "documentos_novos": resultado.documentos_novos,
                        "paginas": resultado.paginas,
                    }),
                    &format!(
                        "ADN: {} documento(s) novo(s), NSU {} -> {}",
                        resultado.documentos_novos, resultado.inicio_nsu, resultado.ultimo_nsu
                    ),
                );
                Ok(())
            }
            AdnAcao::Documentos => {
                let documentos = emissor.documentos_adn()?;
                let valores = documentos
                    .iter()
                    .map(|documento| {
                        json!({
                            "nsu": documento.nsu,
                            "chave": documento.chave_acesso,
                            "tipo": documento.tipo_documento,
                            "tipo_evento": documento.tipo_evento,
                            "data_hora_geracao": documento.data_hora_geracao,
                        })
                    })
                    .collect::<Vec<_>>();
                imprimir(
                    cli.json,
                    &Value::Array(valores),
                    &format!("{} documento(s) do ADN", documentos.len()),
                );
                Ok(())
            }
        },
        Comando::Parametros { acao } => match acao {
            ParametrosAcao::Convenio { municipio, forcar } => {
                let convenio = emissor.consultar_convenio(&municipio, forcar)?;
                imprimir(
                    cli.json,
                    &json!({
                        "tipo_convenio": convenio.tipo_convenio,
                        "aderente_ambiente_nacional": convenio.aderente_ambiente_nacional,
                        "aderente_emissor_nacional": convenio.aderente_emissor_nacional,
                        "situacao_emissao_padrao": convenio.situacao_emissao_padrao,
                        "aderente_man": convenio.aderente_man,
                        "permite_aproveitamento_creditos": convenio.permite_aproveitamento_creditos,
                    }),
                    &format!("convênio do município {municipio} consultado"),
                );
                Ok(())
            }
            ParametrosAcao::Aliquota {
                municipio,
                servico,
                competencia,
                forcar,
            } => {
                let consulta =
                    emissor.consultar_aliquota(&municipio, &servico, &competencia, forcar)?;
                let mut mapa = serde_json::Map::new();
                for (codigo, lista) in &consulta.aliquotas {
                    let itens = lista
                        .iter()
                        .map(|aliquota| {
                            json!({
                                "incidencia": aliquota.incidencia,
                                "aliquota": aliquota.aliquota,
                                "data_inicio": aliquota.data_inicio,
                                "data_fim": aliquota.data_fim,
                            })
                        })
                        .collect();
                    mapa.insert(codigo.clone(), Value::Array(itens));
                }
                imprimir(
                    cli.json,
                    &json!({"mensagem": consulta.mensagem, "aliquotas": mapa}),
                    &format!("alíquotas de {servico} em {municipio}/{competencia}"),
                );
                Ok(())
            }
            ParametrosAcao::Retencoes {
                municipio,
                competencia,
                forcar,
            } => {
                let corpo = {
                    let cliente = carimbar_nfse::ClienteParametros::new(
                        emissor.ambiente().tp_amb(),
                        executor.as_ref(),
                        None,
                    );
                    cliente.consultar_retencoes(&municipio, &competencia, forcar)?
                };
                println!("{corpo}");
                Ok(())
            }
        },
        Comando::Empresa { .. } => Err(ErroCli::operacional("comando administrativo inesperado")),
    }
}

fn criar_executor(
    cli: &Config,
    cofre: &Cofre,
    empresa: &Cnpj,
) -> Result<Box<dyn Executor>, ErroCli> {
    if let Some(caminho) = &cli.replay {
        let executor = ReplayExecutor::de_arquivo(caminho).map_err(ErroCli::operacional)?;
        return Ok(Box::new(executor));
    }
    let id = match &cli.certificado {
        Some(id) => id.clone(),
        None => cofre
            .listar(empresa)?
            .first()
            .map(|meta| meta.id.clone())
            .ok_or_else(|| {
                ErroCli::uso(format!("empresa {empresa} não tem certificado importado"))
            })?,
    };
    let certificado = cofre.certificado(empresa, &id)?;
    let raizes = ler_raizes(cli.raizes.as_deref())?;
    let cliente = ClienteMtls::com_a1(&certificado, &raizes, Opcoes::default())?;
    Ok(Box::new(cliente))
}

fn abrir_cofre(caminho: &Path) -> Result<Cofre, ErroCli> {
    let kek = ler_kek()?;
    if caminho.exists() {
        Ok(Cofre::abrir(caminho, Box::new(ChaveMestra::de_bytes(kek)))?)
    } else {
        Ok(Cofre::criar(caminho, Box::new(ChaveMestra::de_bytes(kek)))?)
    }
}

fn ler_kek() -> Result<[u8; 32], ErroCli> {
    let texto = std::env::var("CARIMBAR_KEK")
        .map_err(|_| ErroCli::uso("defina CARIMBAR_KEK com 64 caracteres hexadecimais"))?;
    let texto = texto.strip_prefix("0x").unwrap_or(&texto);
    if texto.len() != 64 {
        return Err(ErroCli::uso(
            "CARIMBAR_KEK deve ter 64 caracteres hexadecimais",
        ));
    }
    let mut chave = [0_u8; 32];
    for (indice, par) in texto.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let par =
            std::str::from_utf8(par).map_err(|_| ErroCli::uso("CARIMBAR_KEK não é hexadecimal"))?;
        chave[indice] = u8::from_str_radix(par, 16)
            .map_err(|_| ErroCli::uso("CARIMBAR_KEK não é hexadecimal"))?;
    }
    Ok(chave)
}

fn ler_raizes(diretorio: Option<&Path>) -> Result<Vec<Vec<u8>>, ErroCli> {
    let diretorio = diretorio.ok_or_else(|| {
        ErroCli::uso("defina CARIMBAR_RAIZES ou --raizes com um diretório de raízes DER")
    })?;
    let mut raizes = Vec::new();
    let entradas = fs::read_dir(diretorio).map_err(|erro| {
        ErroCli::uso(format!(
            "não foi possível ler {}: {erro}",
            diretorio.display()
        ))
    })?;
    for entrada in entradas {
        let entrada = entrada.map_err(|erro| ErroCli::operacional(erro.to_string()))?;
        let caminho = entrada.path();
        let extensao = caminho
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(extensao.as_str(), "der" | "cer" | "crt") {
            raizes.push(fs::read(&caminho).map_err(|erro| {
                ErroCli::operacional(format!(
                    "não foi possível ler {}: {erro}",
                    caminho.display()
                ))
            })?);
        }
    }
    if raizes.is_empty() {
        return Err(ErroCli::uso(format!(
            "nenhuma raiz DER em {}",
            diretorio.display()
        )));
    }
    Ok(raizes)
}

fn motivo_cancelamento(codigo: u8) -> Result<MotivoCancelamento, ErroCli> {
    match codigo {
        1 => Ok(MotivoCancelamento::ErroEmissao),
        2 => Ok(MotivoCancelamento::ServicoNaoPrestado),
        9 => Ok(MotivoCancelamento::Outros),
        outro => Err(ErroCli::uso(format!(
            "motivo {outro} inválido; use 1, 2 ou 9"
        ))),
    }
}

fn versao_aplic() -> String {
    std::env::var("CARIMBAR_VER_APLIC")
        .unwrap_or_else(|_| format!("carimbar-cli-{}", env!("CARGO_PKG_VERSION")))
}

fn agora_fiscal() -> Result<String, ErroCli> {
    let formato =
        time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]+00:00");
    time::OffsetDateTime::now_utc()
        .format(&formato)
        .map_err(|erro| ErroCli::operacional(format!("data/hora: {erro}")))
}

fn formatar_tempo(tempo: SystemTime) -> String {
    let data = time::OffsetDateTime::from(tempo);
    data.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn imprimir(json_modo: bool, valor: &Value, texto: &str) {
    if json_modo {
        match serde_json::to_string_pretty(valor) {
            Ok(saida) => println!("{saida}"),
            Err(_) => println!("{valor}"),
        }
    } else {
        println!("{texto}");
    }
}
