//! Binding Python (PyO3 + maturin, abi3-py39).
//!
//! API síncrona sobre `Cofre` e `Emissor`. Operações bloqueantes liberam o GIL.
//! Dinheiro entra e sai como texto (`Dinheiro`), nunca como `float`.

#[cfg(feature = "nfe-sha1")]
use carimbar::ResultadoNfe;
use carimbar::nfe::ErroNfe as ErroNfeModelo;
use carimbar::nfse::{ChaveNfse, ErroNfse, EventoCancelamento, Executor, MotivoCancelamento};
use carimbar::transport::{ClienteMtls, ErroEnvio, ErroTransporte, Opcoes, Requisicao, Resposta};
use carimbar::types::{ChaveAcesso, Cnpj, Dinheiro, TpAmb};
use carimbar::vault::{ChaveMestra, Cofre};
use carimbar::{Emissor, ErroEmissor, Journal, ResultadoNfse};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

create_exception!(_carimbar, ErroCarimbar, PyException);
create_exception!(_carimbar, ErroValidacao, ErroCarimbar);
create_exception!(_carimbar, ErroFiscal, ErroCarimbar);
create_exception!(_carimbar, ErroTransporteCarimbar, ErroCarimbar);
create_exception!(_carimbar, ErroProducao, ErroCarimbar);
create_exception!(_carimbar, ErroConflito, ErroCarimbar);
create_exception!(_carimbar, ErroCofre, ErroCarimbar);

fn erro_py(erro: ErroEmissor) -> PyErr {
    match &erro {
        ErroEmissor::ProducaoBloqueada => ErroProducao::new_err(erro.to_string()),
        ErroEmissor::ConflitoIdempotencia { .. } => ErroConflito::new_err(erro.to_string()),
        ErroEmissor::Cofre(_) => ErroCofre::new_err(erro.to_string()),
        ErroEmissor::Nfse(erro) => erro_nfse_py(erro),
        ErroEmissor::Nfe(erro) => erro_nfe_py(erro),
        ErroEmissor::Divergencia { .. }
        | ErroEmissor::EstadoInvalido { .. }
        | ErroEmissor::OperacaoDeOutraEmpresa(_)
        | ErroEmissor::OperacaoNaoEncontrada(_)
        | ErroEmissor::EmpresaNaoEncontrada(_)
        | ErroEmissor::CertificadoAusente(_)
        | ErroEmissor::CertificadoDeOutraEmpresa => ErroValidacao::new_err(erro.to_string()),
        _ => ErroCarimbar::new_err(erro.to_string()),
    }
}

fn erro_nfse_py(erro: &ErroNfse) -> PyErr {
    match erro {
        ErroNfse::AntesDoEnvio(_)
        | ErroNfse::ResultadoDesconhecido(_)
        | ErroNfse::Transporte(_)
        | ErroNfse::Http { .. } => ErroTransporteCarimbar::new_err(erro.to_string()),
        ErroNfse::ParametroAusente(_) | ErroNfse::Campo { .. } => {
            ErroValidacao::new_err(erro.to_string())
        }
        _ => ErroCarimbar::new_err(erro.to_string()),
    }
}

fn erro_nfe_py(erro: &ErroNfeModelo) -> PyErr {
    match erro {
        ErroNfeModelo::AntesDoEnvio(_)
        | ErroNfeModelo::ResultadoDesconhecido(_)
        | ErroNfeModelo::Transporte(_)
        | ErroNfeModelo::Http { .. }
        | ErroNfeModelo::Resposta(_) => ErroTransporteCarimbar::new_err(erro.to_string()),
        ErroNfeModelo::Rejeitada { .. } => ErroFiscal::new_err(erro.to_string()),
        ErroNfeModelo::Pendente(_) => ErroTransporteCarimbar::new_err(erro.to_string()),
        _ => ErroValidacao::new_err(erro.to_string()),
    }
}

fn kek(texto: &str) -> PyResult<[u8; 32]> {
    let texto = texto.strip_prefix("0x").unwrap_or(texto);
    if texto.len() != 64 {
        return Err(ErroValidacao::new_err(
            "KEK deve ter 64 caracteres hexadecimais",
        ));
    }
    let mut chave = [0_u8; 32];
    for (indice, par) in texto.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let par = std::str::from_utf8(par)
            .map_err(|_| ErroValidacao::new_err("KEK não é hexadecimal"))?;
        chave[indice] = u8::from_str_radix(par, 16)
            .map_err(|_| ErroValidacao::new_err("KEK não é hexadecimal"))?;
    }
    Ok(chave)
}

fn cnpj(texto: &str) -> PyResult<Cnpj> {
    Cnpj::parse(texto).map_err(|erro| ErroValidacao::new_err(erro.to_string()))
}

fn abrir_cofre(caminho: &Path, chave: [u8; 32]) -> Result<Cofre, carimbar::vault::ErroCofre> {
    if caminho.exists() {
        Cofre::abrir(caminho, Box::new(ChaveMestra::de_bytes(chave)))
    } else {
        Cofre::criar(caminho, Box::new(ChaveMestra::de_bytes(chave)))
    }
}

fn meta_dict(py: Python<'_>, meta: &carimbar::vault::MetadadosCertificado) -> PyResult<Py<PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("empresa", meta.empresa.as_str())?;
    dict.set_item("id", &meta.id)?;
    dict.set_item("titular", meta.titular.as_str())?;
    dict.set_item("sha256", &meta.sha256)?;
    dict.set_item("valido_ate", format!("{:?}", meta.valido_ate))?;
    Ok(dict.unbind())
}

/// Cofre multiempresa.
#[pyclass(name = "Cofre")]
struct CofrePy {
    cofre: Mutex<Cofre>,
}

#[pymethods]
impl CofrePy {
    #[new]
    fn new(caminho: &str, kek_hex: &str) -> PyResult<Self> {
        let cofre = abrir_cofre(Path::new(caminho), kek(kek_hex)?)
            .map_err(|erro| ErroCofre::new_err(erro.to_string()))?;
        Ok(Self {
            cofre: Mutex::new(cofre),
        })
    }

    fn importar(
        &mut self,
        py: Python<'_>,
        empresa: &str,
        id: &str,
        pfx: &[u8],
        senha: &str,
    ) -> PyResult<Py<PyDict>> {
        let empresa = cnpj(empresa)?;
        let meta = self
            .cofre
            .lock()
            .expect("cofre")
            .importar(&empresa, id, pfx, senha)
            .map_err(|erro| ErroCofre::new_err(erro.to_string()))?;
        meta_dict(py, &meta)
    }

    fn listar(&self, py: Python<'_>, empresa: &str) -> PyResult<Vec<Py<PyDict>>> {
        let empresa = cnpj(empresa)?;
        let metas = self
            .cofre
            .lock()
            .expect("cofre")
            .listar(&empresa)
            .map_err(|erro| ErroCofre::new_err(erro.to_string()))?;
        metas.iter().map(|meta| meta_dict(py, meta)).collect()
    }

    fn remover(&mut self, empresa: &str, id: &str) -> PyResult<()> {
        let empresa = cnpj(empresa)?;
        self.cofre
            .lock()
            .expect("cofre")
            .remover(&empresa, id)
            .map_err(|erro| ErroCofre::new_err(erro.to_string()))
    }

    fn rotacionar_dek(&mut self, empresa: &str) -> PyResult<u32> {
        let empresa = cnpj(empresa)?;
        self.cofre
            .lock()
            .expect("cofre")
            .rotacionar_dek(&empresa)
            .map_err(|erro| ErroCofre::new_err(erro.to_string()))
    }

    fn rotacionar_kek(&mut self, nova_kek_hex: &str) -> PyResult<()> {
        self.cofre
            .lock()
            .expect("cofre")
            .rotacionar_kek(Box::new(ChaveMestra::de_bytes(kek(nova_kek_hex)?)))
            .map_err(|erro| ErroCofre::new_err(erro.to_string()))
    }

    fn verificar_auditoria(&self) -> PyResult<()> {
        self.cofre
            .lock()
            .expect("cofre")
            .verificar_auditoria()
            .map_err(|erro| ErroCofre::new_err(erro.to_string()))
    }
}

#[derive(Clone)]
struct ConfigPy {
    cofre: PathBuf,
    journal: PathBuf,
    empresa: Cnpj,
    kek: [u8; 32],
    certificado: Option<String>,
    raizes: Option<PathBuf>,
    replay: Option<PathBuf>,
}

fn criar_executor(config: &ConfigPy, cofre: &Cofre) -> Result<Box<dyn Executor>, ErroEmissor> {
    if let Some(caminho) = &config.replay {
        let executor = ReplayExecutor::de_arquivo(caminho).map_err(ErroEmissor::Journal)?;
        return Ok(Box::new(executor));
    }
    let id = match &config.certificado {
        Some(id) => id.clone(),
        None => cofre
            .listar(&config.empresa)
            .map_err(ErroEmissor::Cofre)?
            .first()
            .map(|meta| meta.id.clone())
            .ok_or(ErroEmissor::CertificadoAusente(config.empresa))?,
    };
    let certificado = cofre
        .certificado(&config.empresa, &id)
        .map_err(ErroEmissor::Cofre)?;
    let raizes = ler_raizes(config.raizes.as_deref())?;
    let cliente = ClienteMtls::com_a1(&certificado, &raizes, Opcoes::default())
        .map_err(|erro| ErroEmissor::Nfse(ErroNfse::AntesDoEnvio(erro)))?;
    Ok(Box::new(cliente))
}

fn ler_raizes(diretorio: Option<&Path>) -> Result<Vec<Vec<u8>>, ErroEmissor> {
    let diretorio = diretorio.ok_or_else(|| {
        ErroEmissor::Journal("defina raizes com um diretório de raízes DER".to_owned())
    })?;
    let mut raizes = Vec::new();
    for entrada in
        std::fs::read_dir(diretorio).map_err(|erro| ErroEmissor::Journal(erro.to_string()))?
    {
        let entrada = entrada.map_err(|erro| ErroEmissor::Journal(erro.to_string()))?;
        let caminho = entrada.path();
        let extensao = caminho
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(extensao.as_str(), "der" | "cer" | "crt") {
            raizes.push(
                std::fs::read(&caminho).map_err(|erro| ErroEmissor::Journal(erro.to_string()))?,
            );
        }
    }
    if raizes.is_empty() {
        return Err(ErroEmissor::Journal(format!(
            "nenhuma raiz DER em {}",
            diretorio.display()
        )));
    }
    Ok(raizes)
}

fn executar_com_config<T>(
    config: &ConfigPy,
    f: impl FnOnce(&mut Emissor<'_>) -> Result<T, ErroEmissor>,
) -> Result<T, ErroEmissor> {
    let cofre = abrir_cofre(&config.cofre, config.kek).map_err(ErroEmissor::Cofre)?;
    let journal = Journal::abrir(&config.journal)?;
    let executor = criar_executor(config, &cofre)?;
    let mut emissor = Emissor::novo(cofre, journal, config.empresa, executor.as_ref())?;
    f(&mut emissor)
}

/// Emissor síncrono.
#[pyclass(name = "Emissor")]
struct EmissorPy {
    config: ConfigPy,
}

#[pymethods]
impl EmissorPy {
    #[new]
    #[pyo3(signature = (cofre, journal, empresa, kek_hex, certificado=None, raizes=None))]
    fn new(
        cofre: &str,
        journal: &str,
        empresa: &str,
        kek_hex: &str,
        certificado: Option<String>,
        raizes: Option<String>,
    ) -> PyResult<Self> {
        Ok(Self {
            config: ConfigPy {
                cofre: PathBuf::from(cofre),
                journal: PathBuf::from(journal),
                empresa: cnpj(empresa)?,
                kek: kek(kek_hex)?,
                certificado,
                raizes: raizes.map(PathBuf::from),
                replay: None,
            },
        })
    }

    /// Cria o emissor com um arquivo de respostas simuladas (somente testes).
    #[staticmethod]
    #[pyo3(signature = (cofre, journal, empresa, kek_hex, replay))]
    fn com_replay(
        cofre: &str,
        journal: &str,
        empresa: &str,
        kek_hex: &str,
        replay: &str,
    ) -> PyResult<Self> {
        Ok(Self {
            config: ConfigPy {
                cofre: PathBuf::from(cofre),
                journal: PathBuf::from(journal),
                empresa: cnpj(empresa)?,
                kek: kek(kek_hex)?,
                certificado: None,
                raizes: None,
                replay: Some(PathBuf::from(replay)),
            },
        })
    }

    fn ambiente(&self, py: Python<'_>) -> PyResult<String> {
        let config = self.config.clone();
        let ambiente = py
            .detach(move || executar_com_config(&config, |emissor| Ok(emissor.ambiente().tp_amb())))
            .map_err(erro_py)?;
        Ok(match ambiente {
            TpAmb::Producao => "producao".to_owned(),
            TpAmb::Homologacao => "homologacao".to_owned(),
        })
    }

    fn promover_producao(
        &self,
        py: Python<'_>,
        certificado: &str,
        confirmar: &str,
    ) -> PyResult<()> {
        let config = self.config.clone();
        let certificado = certificado.to_owned();
        let confirmar = cnpj(confirmar)?;
        py.detach(move || {
            let cofre = abrir_cofre(&config.cofre, config.kek).map_err(ErroEmissor::Cofre)?;
            let journal = Journal::abrir(&config.journal)?;
            let executor = NoopExecutor;
            let mut emissor = Emissor::novo(cofre, journal, config.empresa, &executor)?;
            emissor.promover_producao(
                carimbar::types::ConfirmacaoProducao::new(confirmar),
                &certificado,
            )
        })
        .map_err(erro_py)
    }

    fn rebaixar_homologacao(&self, py: Python<'_>) -> PyResult<()> {
        let config = self.config.clone();
        py.detach(move || {
            let cofre = abrir_cofre(&config.cofre, config.kek).map_err(ErroEmissor::Cofre)?;
            let journal = Journal::abrir(&config.journal)?;
            let executor = NoopExecutor;
            let mut emissor = Emissor::novo(cofre, journal, config.empresa, &executor)?;
            emissor.rebaixar_homologacao()
        })
        .map_err(erro_py)
    }

    fn preparar_emissao(&self, py: Python<'_>, dps_xml: &str) -> PyResult<Py<PyDict>> {
        let config = self.config.clone();
        let dps_xml = dps_xml.to_owned();
        let (operacao, estado) = py
            .detach(move || {
                executar_com_config(&config, |emissor| {
                    let preparada = emissor.preparar_nfse_xml(&dps_xml)?;
                    Ok((preparada.id(), preparada.estado().as_str()))
                })
            })
            .map_err(erro_py)?;
        let dict = PyDict::new(py);
        dict.set_item("operacao", operacao)?;
        dict.set_item("estado", estado)?;
        Ok(dict.unbind())
    }

    #[pyo3(signature = (dps_xml, confirmar_producao=false))]
    fn emitir(
        &self,
        py: Python<'_>,
        dps_xml: &str,
        confirmar_producao: bool,
    ) -> PyResult<Py<PyDict>> {
        let config = self.config.clone();
        let dps_xml = dps_xml.to_owned();
        let resultado = py
            .detach(move || {
                executar_com_config(&config, |emissor| {
                    if emissor.ambiente().eh_producao() && !confirmar_producao {
                        return Err(ErroEmissor::ProducaoBloqueada);
                    }
                    let preparada = emissor.preparar_nfse_xml(&dps_xml)?;
                    emissor.confirmar_nfse(preparada.id())
                })
            })
            .map_err(erro_py)?;
        resultado_nfse(py, resultado)
    }

    #[pyo3(signature = (operacao, confirmar_producao=false))]
    fn confirmar_emissao(
        &self,
        py: Python<'_>,
        operacao: i64,
        confirmar_producao: bool,
    ) -> PyResult<Py<PyDict>> {
        let config = self.config.clone();
        let resultado = py
            .detach(move || {
                executar_com_config(&config, |emissor| {
                    if emissor.ambiente().eh_producao() && !confirmar_producao {
                        return Err(ErroEmissor::ProducaoBloqueada);
                    }
                    emissor.confirmar_nfse(operacao)
                })
            })
            .map_err(erro_py)?;
        resultado_nfse(py, resultado)
    }

    fn consultar(&self, py: Python<'_>, chave: &str) -> PyResult<String> {
        let config = self.config.clone();
        let chave = chave.to_owned();
        py.detach(move || {
            executar_com_config(&config, |emissor| {
                let chave = ChaveNfse::parse(&chave, emissor.ambiente().tp_amb())
                    .map_err(ErroEmissor::Nfse)?;
                emissor.consultar_nfse(&chave)
            })
        })
        .map_err(erro_py)
    }

    #[pyo3(signature = (chave, descricao, motivo=1, confirmar_producao=false))]
    fn cancelar(
        &self,
        py: Python<'_>,
        chave: &str,
        descricao: &str,
        motivo: u8,
        confirmar_producao: bool,
    ) -> PyResult<Py<PyDict>> {
        let motivo = motivo_cancelamento(motivo)?;
        let config = self.config.clone();
        let chave = chave.to_owned();
        let descricao = descricao.to_owned();
        let resultado = py
            .detach(move || {
                executar_com_config(&config, |emissor| {
                    if emissor.ambiente().eh_producao() && !confirmar_producao {
                        return Err(ErroEmissor::ProducaoBloqueada);
                    }
                    let ambiente = emissor.ambiente().tp_amb();
                    let evento = EventoCancelamento {
                        ambiente,
                        chave: ChaveNfse::parse(&chave, ambiente).map_err(ErroEmissor::Nfse)?,
                        autor: carimbar::nfse::InscricaoFederal::parse(config.empresa.as_str())
                            .map_err(ErroEmissor::Nfse)?,
                        ver_aplic: format!("carimbar-py-{}", env!("CARGO_PKG_VERSION")),
                        dh_evento: carimbar::nfse::DataHoraFiscal::parse(&agora())
                            .map_err(ErroEmissor::Nfse)?,
                        motivo,
                        descricao,
                    };
                    emissor.executar_cancelamento(&evento)
                })
            })
            .map_err(erro_py)?;
        resultado_nfse(py, resultado)
    }

    fn gerar_danfse(&self, py: Python<'_>, operacao: i64) -> PyResult<Py<PyBytes>> {
        let config = self.config.clone();
        let pdf = py
            .detach(move || executar_com_config(&config, |emissor| emissor.gerar_danfse(operacao)))
            .map_err(erro_py)?;
        Ok(PyBytes::new(py, &pdf).unbind())
    }

    #[pyo3(signature = (limite_paginas=10))]
    fn sincronizar_adn(&self, py: Python<'_>, limite_paginas: u32) -> PyResult<Py<PyDict>> {
        let config = self.config.clone();
        let resultado = py
            .detach(move || {
                executar_com_config(&config, |emissor| emissor.sincronizar_adn(limite_paginas))
            })
            .map_err(erro_py)?;
        let dict = PyDict::new(py);
        dict.set_item("inicio_nsu", resultado.inicio_nsu)?;
        dict.set_item("ultimo_nsu", resultado.ultimo_nsu)?;
        dict.set_item("documentos_novos", resultado.documentos_novos)?;
        dict.set_item("paginas", resultado.paginas)?;
        Ok(dict.unbind())
    }

    #[pyo3(signature = (municipio, forcar=false))]
    fn consultar_convenio(
        &self,
        py: Python<'_>,
        municipio: &str,
        forcar: bool,
    ) -> PyResult<Py<PyDict>> {
        let config = self.config.clone();
        let municipio = municipio.to_owned();
        let convenio = py
            .detach(move || {
                executar_com_config(&config, |emissor| {
                    emissor.consultar_convenio(&municipio, forcar)
                })
            })
            .map_err(erro_py)?;
        let dict = PyDict::new(py);
        dict.set_item("tipo_convenio", convenio.tipo_convenio)?;
        dict.set_item(
            "aderente_ambiente_nacional",
            convenio.aderente_ambiente_nacional,
        )?;
        dict.set_item(
            "aderente_emissor_nacional",
            convenio.aderente_emissor_nacional,
        )?;
        dict.set_item("situacao_emissao_padrao", convenio.situacao_emissao_padrao)?;
        dict.set_item("aderente_man", convenio.aderente_man)?;
        dict.set_item(
            "permite_aproveitamento_creditos",
            convenio.permite_aproveitamento_creditos,
        )?;
        Ok(dict.unbind())
    }

    #[pyo3(signature = (municipio, servico, competencia, forcar=false))]
    fn consultar_aliquota(
        &self,
        py: Python<'_>,
        municipio: &str,
        servico: &str,
        competencia: &str,
        forcar: bool,
    ) -> PyResult<Py<PyDict>> {
        let config = self.config.clone();
        let municipio = municipio.to_owned();
        let servico = servico.to_owned();
        let competencia = competencia.to_owned();
        let consulta = py
            .detach(move || {
                executar_com_config(&config, |emissor| {
                    emissor.consultar_aliquota(&municipio, &servico, &competencia, forcar)
                })
            })
            .map_err(erro_py)?;
        let dict = PyDict::new(py);
        dict.set_item("mensagem", consulta.mensagem)?;
        let aliquotas = PyDict::new(py);
        for (codigo, lista) in &consulta.aliquotas {
            let itens = lista
                .iter()
                .map(|aliquota| {
                    let item = PyDict::new(py);
                    item.set_item("incidencia", aliquota.incidencia.clone())?;
                    item.set_item("aliquota", aliquota.aliquota.clone())?;
                    item.set_item("data_inicio", &aliquota.data_inicio)?;
                    item.set_item("data_fim", aliquota.data_fim.clone())?;
                    Ok(item.unbind())
                })
                .collect::<PyResult<Vec<_>>>()?;
            aliquotas.set_item(codigo, itens)?;
        }
        dict.set_item("aliquotas", aliquotas)?;
        Ok(dict.unbind())
    }

    fn estado_operacao(&self, py: Python<'_>, operacao: i64) -> PyResult<Py<PyDict>> {
        let config = self.config.clone();
        let operacao_dados = py
            .detach(move || {
                executar_com_config(&config, |emissor| emissor.journal().operacao(operacao))
            })
            .map_err(erro_py)?;
        let dict = PyDict::new(py);
        dict.set_item("operacao", operacao_dados.id)?;
        dict.set_item("estado", operacao_dados.estado.as_str())?;
        dict.set_item("chave", operacao_dados.chave_nfse)?;
        dict.set_item("erro", operacao_dados.erro)?;
        Ok(dict.unbind())
    }

    #[cfg(feature = "nfe-sha1")]
    #[pyo3(signature = (nfe_xml, confirmar_producao=false))]
    fn emitir_nfe(
        &self,
        py: Python<'_>,
        nfe_xml: &str,
        confirmar_producao: bool,
    ) -> PyResult<Py<PyDict>> {
        let config = self.config.clone();
        let nfe_xml = nfe_xml.to_owned();
        let resultado = py
            .detach(move || {
                executar_com_config(&config, |emissor| {
                    if emissor.ambiente().eh_producao() && !confirmar_producao {
                        return Err(ErroEmissor::ProducaoBloqueada);
                    }
                    let preparada = emissor.preparar_nfe_xml(&nfe_xml)?;
                    emissor.confirmar_nfe(preparada.id())
                })
            })
            .map_err(erro_py)?;
        resultado_nfe(py, resultado)
    }

    #[cfg(feature = "nfe-sha1")]
    fn consultar_nfe(&self, py: Python<'_>, chave: &str) -> PyResult<Py<PyDict>> {
        let config = self.config.clone();
        let chave =
            ChaveAcesso::parse(chave).map_err(|erro| ErroValidacao::new_err(erro.to_string()))?;
        let consulta = py
            .detach(move || executar_com_config(&config, |emissor| emissor.consultar_nfe(&chave)))
            .map_err(erro_py)?;
        let dict = PyDict::new(py);
        dict.set_item("codigo", consulta.codigo)?;
        dict.set_item("motivo", consulta.motivo)?;
        dict.set_item(
            "protocolo",
            consulta.protocolo.map(|protocolo| protocolo.numero),
        )?;
        Ok(dict.unbind())
    }

    #[cfg(feature = "nfe-sha1")]
    fn gerar_danfe_nfe(&self, py: Python<'_>, operacao: i64) -> PyResult<Py<PyBytes>> {
        let config = self.config.clone();
        let pdf = py
            .detach(move || {
                executar_com_config(&config, |emissor| emissor.gerar_danfe_nfe(operacao))
            })
            .map_err(erro_py)?;
        Ok(PyBytes::new(py, &pdf).unbind())
    }
}

fn resultado_nfse(py: Python<'_>, resultado: ResultadoNfse) -> PyResult<Py<PyDict>> {
    let dict = PyDict::new(py);
    match resultado {
        ResultadoNfse::Autorizada {
            operacao,
            chave,
            xml_nfse,
        } => {
            dict.set_item("estado", "autorizada")?;
            dict.set_item("operacao", operacao)?;
            dict.set_item("chave", chave.as_str())?;
            dict.set_item("xml_nfse", xml_nfse)?;
            Ok(dict.unbind())
        }
        ResultadoNfse::Rejeitada { detalhe, .. } => Err(ErroFiscal::new_err(detalhe)),
        ResultadoNfse::Desconhecido { detalhe, .. } => {
            Err(ErroTransporteCarimbar::new_err(detalhe))
        }
        ResultadoNfse::NaoEncontrada { detalhe, .. } => {
            Err(ErroTransporteCarimbar::new_err(detalhe))
        }
        ResultadoNfse::Cancelada {
            operacao,
            xml_evento,
        } => {
            dict.set_item("estado", "cancelada")?;
            dict.set_item("operacao", operacao)?;
            dict.set_item("xml_evento", xml_evento)?;
            Ok(dict.unbind())
        }
    }
}

#[cfg(feature = "nfe-sha1")]
fn resultado_nfe(py: Python<'_>, resultado: ResultadoNfe) -> PyResult<Py<PyDict>> {
    let dict = PyDict::new(py);
    match resultado {
        ResultadoNfe::Autorizada {
            operacao,
            chave,
            xml,
        } => {
            dict.set_item("estado", "autorizada")?;
            dict.set_item("operacao", operacao)?;
            dict.set_item("chave", chave.as_str())?;
            dict.set_item("xml", xml)?;
            Ok(dict.unbind())
        }
        ResultadoNfe::Recibo { operacao, recibo } => {
            dict.set_item("estado", "recibo")?;
            dict.set_item("operacao", operacao)?;
            dict.set_item("recibo", recibo)?;
            Ok(dict.unbind())
        }
        ResultadoNfe::Rejeitada { codigo, motivo, .. } => {
            Err(ErroFiscal::new_err(format!("{codigo}: {motivo}")))
        }
        ResultadoNfe::Desconhecida { detalhe, .. } => Err(ErroTransporteCarimbar::new_err(detalhe)),
        ResultadoNfe::NaoEncontrada { detalhe, .. } => {
            Err(ErroTransporteCarimbar::new_err(detalhe))
        }
        ResultadoNfe::Cancelada {
            operacao,
            protocolo,
        } => {
            dict.set_item("estado", "cancelada")?;
            dict.set_item("operacao", operacao)?;
            dict.set_item("protocolo", protocolo)?;
            Ok(dict.unbind())
        }
    }
}

fn motivo_cancelamento(codigo: u8) -> PyResult<MotivoCancelamento> {
    match codigo {
        1 => Ok(MotivoCancelamento::ErroEmissao),
        2 => Ok(MotivoCancelamento::ServicoNaoPrestado),
        9 => Ok(MotivoCancelamento::Outros),
        outro => Err(ErroValidacao::new_err(format!(
            "motivo {outro} inválido; use 1, 2 ou 9"
        ))),
    }
}

fn agora() -> String {
    let formato =
        time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]+00:00");
    time::OffsetDateTime::now_utc()
        .format(&formato)
        .unwrap_or_else(|_| "2026-01-01T00:00:00+00:00".to_owned())
}

/// Dinheiro em texto, sem `float`.
#[pyclass(name = "Dinheiro")]
struct DinheiroPy {
    valor: Dinheiro,
}

#[pymethods]
impl DinheiroPy {
    #[new]
    fn new(valor: &str) -> PyResult<Self> {
        let valor = valor
            .parse::<Dinheiro>()
            .map_err(|erro| ErroValidacao::new_err(erro.to_string()))?;
        Ok(Self { valor })
    }

    fn texto(&self) -> String {
        self.valor.para_xml()
    }

    fn somar(&self, outro: &DinheiroPy) -> PyResult<Self> {
        let valor = self
            .valor
            .checked_add(outro.valor)
            .ok_or_else(|| ErroValidacao::new_err("soma excedida"))?;
        Ok(Self { valor })
    }

    fn __str__(&self) -> String {
        self.valor.para_xml()
    }
}

struct NoopExecutor;

impl Executor for NoopExecutor {
    fn executar(&self, _requisicao: &Requisicao) -> Result<Resposta, ErroEnvio> {
        Err(ErroEnvio::AntesDoEnvio(ErroTransporte::Configuracao(
            "transporte não configurado".to_owned(),
        )))
    }
}

struct ReplayExecutor {
    respostas: Mutex<VecDeque<(Option<String>, u16, String)>>,
}

impl ReplayExecutor {
    fn de_arquivo(caminho: &Path) -> Result<Self, String> {
        let texto = std::fs::read_to_string(caminho).map_err(|erro| erro.to_string())?;
        let valor: Value = serde_json::from_str(&texto).map_err(|erro| erro.to_string())?;
        let lista = valor
            .get("respostas")
            .and_then(Value::as_array)
            .ok_or_else(|| "replay: respostas ausente".to_owned())?;
        let mut respostas = VecDeque::new();
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
            respostas.push_back((
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

impl Executor for ReplayExecutor {
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
        let (_, status, corpo) = respostas.remove(indice).expect("resposta");
        Ok(Resposta {
            status,
            cabecalhos: vec![("content-type".to_owned(), "application/json".to_owned())],
            corpo: corpo.into_bytes(),
        })
    }
}

/// Retorna `True` se o CNPJ tem DV válido.
#[pyfunction]
fn validar_cnpj(cnpj: &str) -> bool {
    Cnpj::parse(cnpj).is_ok()
}

/// Retorna `True` se a chave de acesso de 44 posições tem DV válido.
#[pyfunction]
fn validar_chave_acesso(chave: &str) -> bool {
    ChaveAcesso::parse(chave).is_ok()
}

#[pymodule]
fn _carimbar(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_function(wrap_pyfunction!(validar_cnpj, m)?)?;
    m.add_function(wrap_pyfunction!(validar_chave_acesso, m)?)?;
    m.add_class::<CofrePy>()?;
    m.add_class::<EmissorPy>()?;
    m.add_class::<DinheiroPy>()?;
    m.add("ErroCarimbar", m.py().get_type::<ErroCarimbar>())?;
    m.add("ErroValidacao", m.py().get_type::<ErroValidacao>())?;
    m.add("ErroFiscal", m.py().get_type::<ErroFiscal>())?;
    m.add(
        "ErroTransporteCarimbar",
        m.py().get_type::<ErroTransporteCarimbar>(),
    )?;
    m.add(
        "ErroTransporte",
        m.py().get_type::<ErroTransporteCarimbar>(),
    )?;
    m.add("ErroProducao", m.py().get_type::<ErroProducao>())?;
    m.add("ErroConflito", m.py().get_type::<ErroConflito>())?;
    m.add("ErroCofre", m.py().get_type::<ErroCofre>())?;
    Ok(())
}
