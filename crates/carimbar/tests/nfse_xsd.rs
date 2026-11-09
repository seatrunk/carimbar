//! Validação dos XMLs gerados contra os pacotes oficiais vendorizados.
//!
//! Esses testes não acessam a rede. Eles usam os XSD de produção restrita e de
//! produção já presentes em `schemas/`, com CNPJ e chave sintéticos.
//!
//! Só compila com a feature `xsd` ligada.

#![cfg(feature = "xsd")]

use carimbar::dsig::{ErroDsig, Signer};
use carimbar::nfse::{
    ChaveNfse, CodigoMunicipio, DataFiscal, DataHoraFiscal, Dps, EventoCancelamento,
    InscricaoFederal, LocalPrestacao, MotivoCancelamento, NumeroDps, Prestador, RegimeTributario,
    SerieDps, Servico, TipoEmitente, TipoRetencaoIssqn, TotalTributos, TribIssqn, TributoMunicipal,
    Valores,
};
#[cfg(feature = "xsd")]
use carimbar::nfse::{ClienteSefin, Executor};
#[cfg(feature = "xsd")]
use carimbar::transport::{ErroEnvio, Requisicao, Resposta};
use carimbar::types::{Dinheiro, TpAmb};
use std::path::{Path, PathBuf};

struct AssinanteFalso;

impl Signer for AssinanteFalso {
    fn certificado_der(&self) -> &[u8] {
        &[0x30, 0x82, 0x01, 0x00]
    }

    fn assinar_rsa_sha256(&self, _dados: &[u8]) -> Result<Vec<u8>, ErroDsig> {
        Ok(vec![0x30, 0x31, 0x32, 0x33])
    }
}

fn esquema(pacote: &str, arquivo: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas")
        .join(pacote)
        .join(arquivo)
}

fn municipio() -> CodigoMunicipio {
    CodigoMunicipio::new("3550308").expect("município")
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
            inscricao: InscricaoFederal::parse("00000001000136").expect("CNPJ"),
            inscricao_municipal: None,
            nome: None,
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

fn chave(ambiente: TpAmb) -> ChaveNfse {
    let valor = format!("{}{}{}", "355030", "00000001000136", "0".repeat(30));
    ChaveNfse::parse(&valor, ambiente).expect("chave sintética")
}

fn evento(ambiente: TpAmb) -> EventoCancelamento {
    EventoCancelamento {
        ambiente,
        chave: chave(ambiente),
        autor: InscricaoFederal::parse("00000001000136").expect("CNPJ"),
        ver_aplic: "carimbar-0.0.2".to_owned(),
        dh_evento: DataHoraFiscal::parse("2026-11-08T11:00:00-03:00").expect("dhEvento"),
        motivo: MotivoCancelamento::ErroEmissao,
        descricao: "Erro na emissao da nota sintetica".to_owned(),
    }
}

#[test]
fn dps_de_homologacao_passa_no_xsd_restrito_sem_e_com_assinatura() {
    let schema = esquema("nfse_v1.01_20261020", "DPS_v1.01.xsd");
    let documento = dps(TpAmb::Homologacao).para_xml().expect("XML");
    carimbar::xml::xsd::validar(documento.as_bytes(), &schema).expect("DPS sem assinatura");
    let assinada = dps(TpAmb::Homologacao)
        .para_xml_assinado(&AssinanteFalso)
        .expect("assinada");
    carimbar::xml::xsd::validar(assinada.as_bytes(), &schema).expect("DPS assinada");
}

#[test]
fn dps_de_producao_passa_no_xsd_de_producao() {
    let schema = esquema("nfse_v1.01_20260209", "DPS_v1.01.xsd");
    let documento = dps(TpAmb::Producao).para_xml().expect("XML");
    carimbar::xml::xsd::validar(documento.as_bytes(), &schema).expect("DPS de produção");
}

#[test]
fn dps_restrita_com_fin_nfse_nao_passa_no_xsd_de_producao() {
    let schema = esquema("nfse_v1.01_20260209", "DPS_v1.01.xsd");
    let homologacao = dps(TpAmb::Homologacao).para_xml().expect("XML");
    carimbar::xml::xsd::validar(homologacao.as_bytes(), &schema)
        .expect_err("finNFSe não existe na produção");
}

#[test]
fn evento_de_homologacao_passa_no_xsd_de_producao_sem_e_com_assinatura() {
    // O pacote de produção restrita vendorizado em 2026-10-20 traz um padrão
    // estrito de `TSIdPedRegEvt` que não aceita o Id documentado
    // (`PRE` + chave + tipo) para CNPJs cujo terceiro dígito não é 1 ou 2.
    // O pacote de produção corrigiu o padrão para `PRE[0-9]{56}`; por isso a
    // validação estrutural do pedido é feita contra ele.
    let schema = esquema("nfse_v1.01_20260209", "pedRegEvento_v1.01.xsd");
    let documento = evento(TpAmb::Homologacao).para_xml().expect("XML");
    carimbar::xml::xsd::validar(documento.as_bytes(), &schema).expect("pedido sem assinatura");
    let assinado = evento(TpAmb::Homologacao)
        .para_xml_assinado(&AssinanteFalso)
        .expect("assinado");
    carimbar::xml::xsd::validar(assinado.as_bytes(), &schema).expect("pedido assinado");
}

#[cfg(feature = "xsd")]
struct ExecutorPanico;

#[cfg(feature = "xsd")]
impl Executor for ExecutorPanico {
    fn executar(&self, _requisicao: &Requisicao) -> Result<Resposta, ErroEnvio> {
        panic!("o envio não deveria ocorrer quando o XSD falha");
    }
}

#[cfg(feature = "xsd")]
#[test]
fn cliente_valida_o_xml_antes_de_enviar() {
    let executor = ExecutorPanico;
    let cliente = ClienteSefin::new(TpAmb::Homologacao, &executor);
    let esquema_producao = esquema("nfse_v1.01_20260209", "DPS_v1.01.xsd");
    let erro = cliente
        .emitir_validando(&dps(TpAmb::Homologacao), &AssinanteFalso, &esquema_producao)
        .expect_err("finNFSe não existe no XSD de produção");
    assert!(matches!(erro, carimbar::nfse::ErroNfse::Xsd(_)));
}
