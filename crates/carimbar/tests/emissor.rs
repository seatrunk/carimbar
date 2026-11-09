//! Testes de integração da fachada com PKI sintética e transporte simulado.
//! Nada acessa SEFAZ, Sefin ou ADN.

use carimbar::{Emissor, EstadoOperacao, Journal, KillSwitch, KillSwitchFixo, ResultadoNfse};
use carimbar_nfse::{
    ChaveNfse, CodigoMunicipio, DataFiscal, DataHoraFiscal, Dps, Executor, InscricaoFederal,
    LocalPrestacao, NumeroDps, Prestador, RegimeTributario, SerieDps, Servico, TipoEmitente,
    TipoRetencaoIssqn, TotalTributos, TribIssqn, TributoMunicipal, Valores,
};
use carimbar_transport::{ErroEnvio, ErroTransporte, Requisicao, Resposta};
use carimbar_types::{Cnpj, ConfirmacaoProducao, Dinheiro, TpAmb};
use carimbar_vault::{ChaveMestra, Cofre};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const SENHA: &str = "senha-sintetica-123";
const CNPJ: &str = "00000001000136";
const MUNICIPIO: &str = "3550308";
const KEK: [u8; 32] = [7; 32];

struct Pki {
    pfx: Vec<u8>,
}

fn diretorio(nome: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("emissor-{nome}"));
    std::fs::create_dir_all(&dir).expect("cria diretório temporário");
    dir
}

fn openssl(args: &[&str]) {
    let saida = Command::new("openssl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .env("CARIMBAR_SENHA_TESTE", SENHA)
        .output()
        .expect("openssl disponível no PATH");
    assert!(
        saida.status.success(),
        "openssl {args:?}: {}",
        String::from_utf8_lossy(&saida.stderr)
    );
}

fn pki(nome: &str) -> Pki {
    let dir = diretorio(nome);
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
        &format!("/CN=EMPRESA SINTETICA {nome}"),
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
    Pki {
        pfx: std::fs::read(pfx).expect("lê pfx"),
    }
}

fn empresa() -> Cnpj {
    Cnpj::parse(CNPJ).expect("CNPJ sintético")
}

fn municipio() -> CodigoMunicipio {
    CodigoMunicipio::new(MUNICIPIO).expect("município")
}

fn dps(ambiente: TpAmb) -> Dps {
    Dps {
        ambiente,
        dh_emi: DataHoraFiscal::parse("2026-11-08T10:00:00-03:00").expect("dhEmi"),
        ver_aplic: "carimbar-0.0.2".to_owned(),
        serie: SerieDps::new(1).expect("série"),
        numero: NumeroDps::new(1).expect("número"),
        d_compet: DataFiscal::parse("2026-11-08").expect("dCompet"),
        tp_emit: TipoEmitente::Prestador,
        c_motivo_emis_ti: None,
        ch_nfse_rej: None,
        c_loc_emi: municipio(),
        substituicao: None,
        prestador: Prestador {
            inscricao: InscricaoFederal::parse(CNPJ).expect("CNPJ"),
            inscricao_municipal: None,
            nome: Some("Empresa Sintetica".to_owned()),
            email: None,
            regime: RegimeTributario::nao_optante(),
        },
        tomador: None,
        intermediario: None,
        servico: Servico {
            local_prestacao: LocalPrestacao::Municipio(municipio()),
            codigo_tributacao_nacional: "010101".to_owned(),
            codigo_tributacao_municipal: None,
            codigo_nbs: None,
            codigo_interno: None,
            descricao: "Servico sintetico de teste".to_owned(),
            informacoes_complementares: None,
        },
        valores: Valores {
            valor_recebido: None,
            valor_servico: "10.00".parse::<Dinheiro>().expect("valor"),
            desconto_incondicional: None,
            desconto_condicional: None,
            tributo_municipal: TributoMunicipal {
                trib_issqn: TribIssqn::Tributavel,
                tipo_retencao: TipoRetencaoIssqn::NaoRetido,
                aliquota: None,
            },
            total_tributos: TotalTributos::IndicadorZero,
        },
        ibs_cbs: None,
    }
}

fn chave_sintetica(ambiente: TpAmb) -> ChaveNfse {
    ChaveNfse::parse(&format!("{}{}{}", "355030", CNPJ, "0".repeat(30)), ambiente).expect("chave")
}

fn resposta(status: u16, corpo: impl Into<Vec<u8>>) -> Resposta {
    Resposta {
        status,
        cabecalhos: vec![("content-type".to_owned(), "application/json".to_owned())],
        corpo: corpo.into(),
    }
}

#[derive(Default)]
struct ExecutorFalso {
    respostas: Mutex<VecDeque<Result<Resposta, ErroEnvio>>>,
    pedidos: Mutex<Vec<Requisicao>>,
}

impl ExecutorFalso {
    fn com(respostas: Vec<Result<Resposta, ErroEnvio>>) -> Self {
        Self {
            respostas: Mutex::new(respostas.into()),
            pedidos: Mutex::new(Vec::new()),
        }
    }

    fn pedidos(&self) -> Vec<Requisicao> {
        self.pedidos.lock().expect("pedidos").clone()
    }
}

impl Executor for ExecutorFalso {
    fn executar(&self, requisicao: &Requisicao) -> Result<Resposta, ErroEnvio> {
        self.pedidos
            .lock()
            .expect("pedidos")
            .push(requisicao.clone());
        self.respostas
            .lock()
            .expect("respostas")
            .pop_front()
            .expect("resposta simulada disponível")
    }
}

fn corpo_autorizacao(ambiente: TpAmb) -> Vec<u8> {
    let id = dps(ambiente).id().expect("Id");
    serde_json::json!({
        "idDps": id.as_str(),
        "chaveAcesso": chave_sintetica(ambiente).as_str(),
        "nfseXmlGZipB64": carimbar_nfse::comprimir_b64("<NFSe/>").expect("comprime"),
        "dataHoraProcessamento": "2026-11-08T10:00:01-03:00",
        "alertas": []
    })
    .to_string()
    .into_bytes()
}

fn corpo_dps(ambiente: TpAmb) -> Vec<u8> {
    let id = dps(ambiente).id().expect("Id");
    serde_json::json!({
        "idDps": id.as_str(),
        "chaveAcesso": chave_sintetica(ambiente).as_str()
    })
    .to_string()
    .into_bytes()
}

fn corpo_nfse(ambiente: TpAmb) -> Vec<u8> {
    serde_json::json!({
        "chaveAcesso": chave_sintetica(ambiente).as_str(),
        "nfseXmlGZipB64": carimbar_nfse::comprimir_b64("<NFSe/>").expect("comprime")
    })
    .to_string()
    .into_bytes()
}

fn cofre_com_certificado(empresa: &Cnpj, pki: &Pki) -> Cofre {
    let mut cofre = Cofre::criar_em_memoria(Box::new(ChaveMestra::de_bytes(KEK))).expect("cofre");
    cofre
        .importar(empresa, "a1", &pki.pfx, SENHA)
        .expect("importa A1");
    cofre
}

#[test]
fn fluxo_completo_persiste_autorizacao_e_nao_reenvia() {
    let pki = pki("fluxo");
    let empresa = empresa();
    let cofre = cofre_com_certificado(&empresa, &pki);
    let journal = Journal::em_memoria().expect("journal");
    let executor = ExecutorFalso::com(vec![Ok(resposta(
        201,
        corpo_autorizacao(TpAmb::Homologacao),
    ))]);
    let mut emissor = Emissor::novo(cofre, journal, empresa, &executor).expect("emissor");

    let resultado = emissor
        .executar_nfse(&dps(TpAmb::Homologacao))
        .expect("emissão");
    let ResultadoNfse::Autorizada {
        operacao,
        chave,
        xml_nfse,
    } = resultado
    else {
        panic!("esperava autorização: {resultado:?}");
    };
    assert_eq!(chave.as_str(), chave_sintetica(TpAmb::Homologacao).as_str());
    assert_eq!(xml_nfse, "<NFSe/>");
    assert_eq!(
        emissor.journal().operacao(operacao).expect("op").estado,
        EstadoOperacao::Autorizada
    );
    assert_eq!(
        emissor
            .journal()
            .total_operacoes(&empresa, TpAmb::Homologacao)
            .expect("total"),
        1
    );
    assert_eq!(executor.pedidos().len(), 1);

    let repetido = emissor.confirmar_nfse(operacao).expect("repetição");
    assert!(matches!(repetido, ResultadoNfse::Autorizada { .. }));
    assert_eq!(executor.pedidos().len(), 1, "não pode haver segundo envio");
}

#[test]
fn preparacao_repetida_devolve_a_mesma_operacao_e_conteudo_diferente_conflita() {
    let pki = pki("idempotencia");
    let empresa = empresa();
    let cofre = cofre_com_certificado(&empresa, &pki);
    let journal = Journal::em_memoria().expect("journal");
    let executor = ExecutorFalso::default();
    let mut emissor = Emissor::novo(cofre, journal, empresa, &executor).expect("emissor");

    let primeira = emissor
        .preparar_nfse(&dps(TpAmb::Homologacao))
        .expect("prepara");
    let segunda = emissor
        .preparar_nfse(&dps(TpAmb::Homologacao))
        .expect("repete");
    assert!(!segunda.nova);
    assert_eq!(primeira.id(), segunda.id());

    let mut divergente = dps(TpAmb::Homologacao);
    divergente.valores.valor_servico = "99.00".parse::<Dinheiro>().expect("valor");
    let erro = emissor
        .preparar_nfse(&divergente)
        .expect_err("conflito de conteúdo");
    assert!(matches!(
        erro,
        carimbar::ErroEmissor::ConflitoIdempotencia { .. }
    ));
    assert_eq!(
        emissor
            .journal()
            .total_operacoes(&empresa, TpAmb::Homologacao)
            .expect("total"),
        1
    );
}

#[test]
fn reinicio_reabre_operacao_autorizada_sem_rede() {
    let pki = pki("reinicio");
    let dir = diretorio("reinicio-arquivos");
    let cofre_path = dir.join("cofre.db");
    let journal_path = dir.join("journal.db");
    let empresa = empresa();
    let _ = std::fs::remove_file(&cofre_path);
    let _ = std::fs::remove_file(&journal_path);

    let mut cofre = Cofre::criar(&cofre_path, Box::new(ChaveMestra::de_bytes(KEK))).expect("cofre");
    cofre
        .importar(&empresa, "a1", &pki.pfx, SENHA)
        .expect("importa");
    let journal = Journal::abrir(&journal_path).expect("journal");
    let executor = ExecutorFalso::com(vec![Ok(resposta(
        201,
        corpo_autorizacao(TpAmb::Homologacao),
    ))]);
    let mut emissor = Emissor::novo(cofre, journal, empresa, &executor).expect("emissor");
    let resultado = emissor
        .executar_nfse(&dps(TpAmb::Homologacao))
        .expect("emissão");
    let ResultadoNfse::Autorizada { operacao, .. } = resultado else {
        panic!("esperava autorização");
    };
    drop(emissor);

    let cofre =
        Cofre::abrir(&cofre_path, Box::new(ChaveMestra::de_bytes(KEK))).expect("reabre cofre");
    let journal = Journal::abrir(&journal_path).expect("reabre journal");
    let sem_rede = ExecutorFalso::default();
    let mut reaberto = Emissor::novo(cofre, journal, empresa, &sem_rede).expect("emissor");
    let retomado = reaberto.confirmar_nfse(operacao).expect("retoma");
    assert!(matches!(retomado, ResultadoNfse::Autorizada { .. }));
    assert!(sem_rede.pedidos().is_empty());
}

#[test]
fn resultado_desconhecido_reconcilia_antes_de_qualquer_reenvio() {
    let pki = pki("desconhecido");
    let empresa = empresa();
    let cofre = cofre_com_certificado(&empresa, &pki);
    let journal = Journal::em_memoria().expect("journal");
    let executor = ExecutorFalso::com(vec![
        Err(ErroEnvio::ResultadoDesconhecido(ErroTransporte::Timeout(
            "leitura",
        ))),
        Ok(resposta(200, corpo_dps(TpAmb::Homologacao))),
        Ok(resposta(200, corpo_nfse(TpAmb::Homologacao))),
    ]);
    let mut emissor = Emissor::novo(cofre, journal, empresa, &executor).expect("emissor");

    let resultado = emissor
        .executar_nfse(&dps(TpAmb::Homologacao))
        .expect("emissão");
    let ResultadoNfse::Desconhecido { operacao, .. } = resultado else {
        panic!("esperava desconhecido: {resultado:?}");
    };
    assert_eq!(
        emissor.journal().operacao(operacao).expect("op").estado,
        EstadoOperacao::Desconhecida
    );

    let reconciliado = emissor.reconciliar_nfse(operacao).expect("reconcilia");
    assert!(matches!(reconciliado, ResultadoNfse::Autorizada { .. }));
    assert_eq!(
        emissor.journal().operacao(operacao).expect("op").estado,
        EstadoOperacao::Autorizada
    );
    let pedidos = executor.pedidos();
    assert_eq!(pedidos.len(), 3, "uma emissão e duas consultas");
    assert!(pedidos[0].url.ends_with("/nfse"));
    assert!(pedidos[1].url.contains("/dps/"));
    assert!(pedidos[2].url.contains("/nfse/"));

    assert!(matches!(
        emissor.confirmar_nfse(operacao).expect("repetição"),
        ResultadoNfse::Autorizada { .. }
    ));
    assert_eq!(executor.pedidos().len(), 3);
}

struct KillSwitchTeste(Arc<AtomicBool>);

impl KillSwitch for KillSwitchTeste {
    fn producao_permitida(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[test]
fn producao_exige_promocao_e_respeita_o_kill_switch() {
    let pki = pki("producao");
    let empresa = empresa();
    let cnpj = empresa;
    let cofre = cofre_com_certificado(&empresa, &pki);
    let journal = Journal::em_memoria().expect("journal");
    let liberado = Arc::new(AtomicBool::new(true));
    let executor = ExecutorFalso::com(vec![Ok(resposta(201, corpo_autorizacao(TpAmb::Producao)))]);
    let mut emissor = Emissor::com_kill_switch(
        cofre,
        journal,
        empresa,
        &executor,
        Box::new(KillSwitchTeste(liberado.clone())),
    )
    .expect("emissor");

    emissor
        .promover_producao(ConfirmacaoProducao::new(cnpj), "a1")
        .expect("promove");
    let preparada = emissor
        .preparar_nfse(&dps(TpAmb::Producao))
        .expect("prepara produção");
    liberado.store(false, Ordering::SeqCst);
    let erro = emissor
        .confirmar_nfse(preparada.id())
        .expect_err("kill switch");
    assert!(matches!(erro, carimbar::ErroEmissor::ProducaoBloqueada));
    assert!(executor.pedidos().is_empty());
    assert_eq!(
        emissor
            .journal()
            .operacao(preparada.id())
            .expect("op")
            .estado,
        EstadoOperacao::Preparada
    );
}

#[test]
fn confirmacao_de_producao_fica_vinculada_ao_cnpj() {
    let pki = pki("cnpj-divergente");
    let empresa = empresa();
    let cofre = cofre_com_certificado(&empresa, &pki);
    let journal = Journal::em_memoria().expect("journal");
    let executor = ExecutorFalso::default();
    let mut emissor = Emissor::com_kill_switch(
        cofre,
        journal,
        empresa,
        &executor,
        Box::new(KillSwitchFixo(true)),
    )
    .expect("emissor");
    let outro = Cnpj::parse("12345678000195").expect("CNPJ sintético");
    let erro = emissor
        .promover_producao(ConfirmacaoProducao::new(outro), "a1")
        .expect_err("CNPJ divergente");
    assert!(matches!(
        erro,
        carimbar::ErroEmissor::ConfirmacaoDivergente(_)
    ));
}
