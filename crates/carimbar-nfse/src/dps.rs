//! Modelo da DPS v1.01, serialização na ordem do XSD oficial e assinatura.

use crate::erro::ErroNfse;
use crate::identificadores::{
    ChaveNfse, CodigoMunicipio, IdDps, InscricaoFederal, NumeroDps, SerieDps,
};
use crate::tempo::{DataFiscal, DataHoraFiscal};
use carimbar_dsig::{Signer, assinar};
use carimbar_types::Decimal;
use carimbar_types::{Dinheiro, TpAmb};
use carimbar_xml::{Documento, Elemento, No, Nome};
use std::fmt;

/// Namespace do leiaute NFS-e.
pub const NS_NFSE: &str = "http://www.sped.fazenda.gov.br/nfse";
/// Versão do leiaute suportada.
pub const VERSAO_LEIAUTE: &str = "1.01";

/// Emitente da DPS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoEmitente {
    /// `1` — prestador.
    Prestador,
    /// `2` — tomador.
    Tomador,
    /// `3` — intermediário.
    Intermediario,
}

impl TipoEmitente {
    /// Código no XML.
    pub fn codigo(self) -> u8 {
        match self {
            Self::Prestador => 1,
            Self::Tomador => 2,
            Self::Intermediario => 3,
        }
    }
}

/// Motivo da emissão pelo tomador ou intermediário.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotivoEmissaoTi {
    /// `1` — importação de serviço.
    Importacao,
    /// `2` — obrigação por legislação municipal.
    LegislacaoMunicipal,
    /// `3` — recusa de emissão pelo prestador.
    RecusaPrestador,
    /// `4` — rejeição da NFS-e emitida pelo prestador.
    RejeicaoPrestador,
}

impl MotivoEmissaoTi {
    /// Código no XML.
    pub fn codigo(self) -> u8 {
        match self {
            Self::Importacao => 1,
            Self::LegislacaoMunicipal => 2,
            Self::RecusaPrestador => 3,
            Self::RejeicaoPrestador => 4,
        }
    }
}

/// Situação do contribuinte perante o Simples Nacional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpSimplesNacional {
    /// `1` — não optante.
    NaoOptante,
    /// `2` — MEI.
    Mei,
    /// `3` — ME ou EPP.
    MeEpp,
}

impl OpSimplesNacional {
    /// Código no XML.
    pub fn codigo(self) -> u8 {
        match self {
            Self::NaoOptante => 1,
            Self::Mei => 2,
            Self::MeEpp => 3,
        }
    }
}

/// Regime de apuração do Simples Nacional para ME/EPP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegimeApuracaoSimples {
    /// `1` — tributos federais e municipal pelo Simples Nacional.
    SimplesNacional,
    /// `2` — federais pelo Simples Nacional e ISSQN fora.
    FederalSimplesIssFora,
    /// `3` — federais e municipal fora do Simples Nacional.
    ForaSimplesNacional,
}

impl RegimeApuracaoSimples {
    /// Código no XML.
    pub fn codigo(self) -> u8 {
        match self {
            Self::SimplesNacional => 1,
            Self::FederalSimplesIssFora => 2,
            Self::ForaSimplesNacional => 3,
        }
    }
}

/// Regime especial de tributação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegimeEspecialTributacao {
    /// `0` — nenhum.
    Nenhum,
    /// `1` — ato cooperado.
    AtoCooperado,
    /// `2` — estimativa.
    Estimativa,
    /// `3` — microempresa municipal.
    MicroempresaMunicipal,
    /// `4` — notário ou registrador.
    Notario,
    /// `5` — profissional autônomo.
    ProfissionalAutonomo,
    /// `6` — sociedade de profissionais.
    SociedadeProfissionais,
    /// `9` — outros.
    Outros,
}

impl RegimeEspecialTributacao {
    /// Código no XML.
    pub fn codigo(self) -> u8 {
        match self {
            Self::Nenhum => 0,
            Self::AtoCooperado => 1,
            Self::Estimativa => 2,
            Self::MicroempresaMunicipal => 3,
            Self::Notario => 4,
            Self::ProfissionalAutonomo => 5,
            Self::SociedadeProfissionais => 6,
            Self::Outros => 9,
        }
    }
}

/// Regimes de tributação do prestador.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegimeTributario {
    /// Situação perante o Simples Nacional.
    pub op_simples_nacional: OpSimplesNacional,
    /// Regime de apuração, quando ME/EPP.
    pub regime_apuracao: Option<RegimeApuracaoSimples>,
    /// Regime especial.
    pub regime_especial: RegimeEspecialTributacao,
}

impl RegimeTributario {
    /// Regime sem optância pelo Simples Nacional e sem regime especial.
    pub fn nao_optante() -> Self {
        Self {
            op_simples_nacional: OpSimplesNacional::NaoOptante,
            regime_apuracao: None,
            regime_especial: RegimeEspecialTributacao::Nenhum,
        }
    }
}

/// Prestador de serviços.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prestador {
    /// CNPJ ou CPF.
    pub inscricao: InscricaoFederal,
    /// Inscrição municipal.
    pub inscricao_municipal: Option<String>,
    /// Nome ou razão social.
    pub nome: Option<String>,
    /// E-mail.
    pub email: Option<String>,
    /// Regimes de tributação.
    pub regime: RegimeTributario,
}

/// Tomador ou intermediário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pessoa {
    /// CNPJ ou CPF.
    pub inscricao: InscricaoFederal,
    /// Inscrição municipal.
    pub inscricao_municipal: Option<String>,
    /// Nome ou razão social.
    pub nome: Option<String>,
    /// E-mail.
    pub email: Option<String>,
}

/// Local da prestação do serviço.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalPrestacao {
    /// Município brasileiro.
    Municipio(CodigoMunicipio),
    /// País no exterior, código ISO de duas letras.
    Pais(String),
}

/// Dados da NFS-e substituída.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Substituicao {
    /// Chave da NFS-e substituída.
    pub chave: ChaveNfse,
    /// Código de justificativa.
    pub motivo: MotivoSubstituicao,
    /// Descrição do motivo, obrigatória para o código `99`.
    pub descricao: Option<String>,
}

/// Código de justificativa de substituição.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotivoSubstituicao {
    /// `01` — desenquadramento do Simples Nacional.
    DesenquadramentoSimples,
    /// `02` — enquadramento no Simples Nacional.
    EnquadramentoSimples,
    /// `03` — inclusão retroativa de imunidade/isenção.
    InclusaoRetroativaImunidade,
    /// `04` — exclusão retroativa de imunidade/isenção.
    ExclusaoRetroativaImunidade,
    /// `05` — rejeição pelo tomador ou intermediário.
    Rejeicao,
    /// `99` — outros.
    Outros,
}

impl MotivoSubstituicao {
    /// Código no XML.
    pub fn codigo(self) -> &'static str {
        match self {
            Self::DesenquadramentoSimples => "01",
            Self::EnquadramentoSimples => "02",
            Self::InclusaoRetroativaImunidade => "03",
            Self::ExclusaoRetroativaImunidade => "04",
            Self::Rejeicao => "05",
            Self::Outros => "99",
        }
    }
}

/// Serviço prestado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Servico {
    /// Local da prestação.
    pub local_prestacao: LocalPrestacao,
    /// Código de tributação nacional do ISSQN (6 dígitos).
    pub codigo_tributacao_nacional: String,
    /// Código de tributação municipal (3 dígitos).
    pub codigo_tributacao_municipal: Option<String>,
    /// Código NBS (9 dígitos).
    pub codigo_nbs: Option<String>,
    /// Código interno do contribuinte.
    pub codigo_interno: Option<String>,
    /// Descrição do serviço.
    pub descricao: String,
    /// Informações complementares.
    pub informacoes_complementares: Option<String>,
}

/// Tributação do ISSQN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TribIssqn {
    /// `1` — operação tributável.
    Tributavel,
    /// `2` — imunidade.
    Imunidade,
    /// `3` — exportação de serviço.
    Exportacao,
    /// `4` — não incidência.
    NaoIncidencia,
}

impl TribIssqn {
    /// Código no XML.
    pub fn codigo(self) -> u8 {
        match self {
            Self::Tributavel => 1,
            Self::Imunidade => 2,
            Self::Exportacao => 3,
            Self::NaoIncidencia => 4,
        }
    }
}

/// Tipo de retenção do ISSQN.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoRetencaoIssqn {
    /// `1` — não retido.
    NaoRetido,
    /// `2` — retido pelo tomador.
    Tomador,
    /// `3` — retido pelo intermediário.
    Intermediario,
}

impl TipoRetencaoIssqn {
    /// Código no XML.
    pub fn codigo(self) -> u8 {
        match self {
            Self::NaoRetido => 1,
            Self::Tomador => 2,
            Self::Intermediario => 3,
        }
    }
}

/// Tributação municipal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TributoMunicipal {
    /// Tributação do ISSQN.
    pub trib_issqn: TribIssqn,
    /// Retenção do ISSQN.
    pub tipo_retencao: TipoRetencaoIssqn,
    /// Alíquota em percentual, quando o município não a parametriza.
    pub aliquota: Option<Decimal>,
}

/// Total aproximado de tributos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotalTributos {
    /// `indTotTrib = 0`.
    IndicadorZero,
    /// `pTotTribSN` para optante do Simples Nacional.
    PercentualSimplesNacional(Decimal),
}

/// Valores do serviço e tributação.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Valores {
    /// Valor recebido pelo intermediário.
    pub valor_recebido: Option<Dinheiro>,
    /// Valor do serviço.
    pub valor_servico: Dinheiro,
    /// Desconto incondicionado.
    pub desconto_incondicional: Option<Dinheiro>,
    /// Desconto condicionado.
    pub desconto_condicional: Option<Dinheiro>,
    /// Tributo municipal.
    pub tributo_municipal: TributoMunicipal,
    /// Total aproximado de tributos.
    pub total_tributos: TotalTributos,
}

/// Destinatário do serviço no grupo IBS/CBS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destinatario {
    /// CNPJ ou CPF.
    pub inscricao: InscricaoFederal,
    /// Nome ou razão social.
    pub nome: String,
}

/// Grupo declarado de IBS/CBS.
///
/// O MVP cobre os campos obrigatórios (`indDest`, `valores/trib/CST` e
/// `cClassTrib`) e os indicadores opcionais mais usados. Grupos de crédito
/// presumido, diferimento, imóveis e bens móveis ficam para incremento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IbsCbs {
    /// Indicador de uso ou consumo pessoal.
    pub ind_final: Option<bool>,
    /// Código indicador da operação (6 dígitos).
    pub c_ind_op: Option<String>,
    /// Tipo de operação com ente governamental ou imóvel.
    pub tp_oper: Option<u8>,
    /// `0` quando o destinatário é o tomador; `1` quando é outra pessoa.
    pub ind_dest: u8,
    /// Destinatário, obrigatório quando `ind_dest = 1` e houver identificação.
    pub destinatario: Option<Destinatario>,
    /// CST do IBS/CBS (3 dígitos).
    pub cst: String,
    /// Classificação tributária do IBS/CBS (6 dígitos).
    pub c_class_trib: String,
}

/// Declaração de Prestação de Serviços.
///
/// A ordem dos campos no XML segue o XSD oficial. O `Id` não é armazenado:
/// [`Dps::id`] o deriva de município, inscrição, série, número e ambiente, o
/// que impede um Id divergente dos demais campos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dps {
    /// Ambiente do XML e do endpoint.
    pub ambiente: TpAmb,
    /// Data e hora da emissão.
    pub dh_emi: DataHoraFiscal,
    /// Versão do aplicativo emissor, até 20 posições.
    pub ver_aplic: String,
    /// Série.
    pub serie: SerieDps,
    /// Número.
    pub numero: NumeroDps,
    /// Data de início da prestação.
    pub d_compet: DataFiscal,
    /// Emitente da DPS.
    pub tp_emit: TipoEmitente,
    /// Motivo da emissão pelo tomador/intermediário.
    pub c_motivo_emis_ti: Option<MotivoEmissaoTi>,
    /// Chave da NFS-e rejeitada.
    pub ch_nfse_rej: Option<ChaveNfse>,
    /// Município emissor.
    pub c_loc_emi: CodigoMunicipio,
    /// Dados da NFS-e substituída.
    pub substituicao: Option<Substituicao>,
    /// Prestador.
    pub prestador: Prestador,
    /// Tomador.
    pub tomador: Option<Pessoa>,
    /// Intermediário.
    pub intermediario: Option<Pessoa>,
    /// Serviço.
    pub servico: Servico,
    /// Valores e tributos.
    pub valores: Valores,
    /// Grupo IBS/CBS.
    pub ibs_cbs: Option<IbsCbs>,
}

impl Dps {
    /// Deriva e valida o `Id` a partir dos campos da DPS.
    pub fn id(&self) -> Result<IdDps, ErroNfse> {
        IdDps::gerar(
            &self.c_loc_emi,
            &self.prestador.inscricao,
            self.serie,
            self.numero,
            self.ambiente,
        )
    }

    /// Valida os campos que não são garantidos pelos tipos.
    pub fn validar(&self) -> Result<(), ErroNfse> {
        self.id()?;
        exigir_texto("verAplic", &self.ver_aplic, 1, 20)?;
        validar_prestador(&self.prestador)?;
        if let Some(tomador) = &self.tomador {
            validar_pessoa("toma", tomador)?;
        }
        if let Some(intermediario) = &self.intermediario {
            validar_pessoa("interm", intermediario)?;
        }
        if self.ambiente == TpAmb::Producao {
            for (campo, pessoa) in [
                ("prest", Some(&self.prestador.inscricao)),
                ("toma", self.tomador.as_ref().map(|p| &p.inscricao)),
                ("interm", self.intermediario.as_ref().map(|p| &p.inscricao)),
            ] {
                if pessoa.is_some_and(InscricaoFederal::alfanumerica) {
                    return Err(ErroNfse::campo(
                        campo,
                        "CNPJ alfanumérico não é aceito na produção",
                    ));
                }
            }
        }
        if let Some(chave) = &self.ch_nfse_rej {
            ChaveNfse::parse(chave.as_str(), self.ambiente)?;
        }
        if let Some(substituicao) = &self.substituicao {
            ChaveNfse::parse(substituicao.chave.as_str(), self.ambiente)?;
            if substituicao.motivo == MotivoSubstituicao::Outros && substituicao.descricao.is_none()
            {
                return Err(ErroNfse::campo(
                    "subst/xMotivo",
                    "obrigatório quando o motivo é 99",
                ));
            }
            if let Some(descricao) = &substituicao.descricao {
                exigir_texto("subst/xMotivo", descricao, 15, 255)?;
            }
        }
        if self.tp_emit != TipoEmitente::Prestador && self.c_motivo_emis_ti.is_none() {
            return Err(ErroNfse::campo(
                "cMotivoEmisTI",
                "obrigatório quando o emitente não é o prestador",
            ));
        }
        validar_servico(&self.servico)?;
        validar_valores(&self.valores)?;
        if let Some(ibs_cbs) = &self.ibs_cbs {
            validar_ibs_cbs(ibs_cbs)?;
        }
        Ok(())
    }

    /// Monta o documento XML da DPS (sem assinatura).
    pub fn documento(&self) -> Result<Documento, ErroNfse> {
        self.validar()?;
        let id = self.id()?;
        let mut inf = el("infDPS").com_atributo_texto("Id", id.as_str());
        inf = inf
            .com_filho(No::Elemento(txt("tpAmb", self.ambiente.codigo())))
            .com_filho(No::Elemento(txt("dhEmi", self.dh_emi.to_string())))
            .com_filho(No::Elemento(txt("verAplic", &self.ver_aplic)))
            .com_filho(No::Elemento(txt("serie", self.serie.para_xml())))
            .com_filho(No::Elemento(txt("nDPS", self.numero.para_xml())))
            .com_filho(No::Elemento(txt("dCompet", self.d_compet.to_string())))
            .com_filho(No::Elemento(txt("tpEmit", self.tp_emit.codigo())));
        if let Some(motivo) = self.c_motivo_emis_ti {
            inf = inf.com_filho(No::Elemento(txt("cMotivoEmisTI", motivo.codigo())));
        }
        // `finNFSe` existe apenas no pacote de produção restrita e só aceita 0.
        if self.ambiente == TpAmb::Homologacao {
            inf = inf.com_filho(No::Elemento(txt("finNFSe", 0)));
        }
        if let Some(chave) = &self.ch_nfse_rej {
            inf = inf.com_filho(No::Elemento(txt("chNFSeRej", chave.as_str())));
        }
        inf = inf.com_filho(No::Elemento(txt("cLocEmi", self.c_loc_emi.as_str())));
        if let Some(substituicao) = &self.substituicao {
            inf = inf.com_filho(No::Elemento(montar_substituicao(substituicao)));
        }
        inf = inf.com_filho(No::Elemento(montar_prestador(&self.prestador)));
        if let Some(tomador) = &self.tomador {
            inf = inf.com_filho(No::Elemento(montar_pessoa("toma", tomador)));
        }
        if let Some(intermediario) = &self.intermediario {
            inf = inf.com_filho(No::Elemento(montar_pessoa("interm", intermediario)));
        }
        inf = inf
            .com_filho(No::Elemento(montar_servico(&self.servico)))
            .com_filho(No::Elemento(montar_valores(&self.valores)));
        if let Some(ibs_cbs) = &self.ibs_cbs {
            inf = inf.com_filho(No::Elemento(montar_ibs_cbs(ibs_cbs)));
        }

        let raiz = el("DPS")
            .com_namespace(None, NS_NFSE)
            .com_atributo_texto("versao", VERSAO_LEIAUTE)
            .com_filho(No::Elemento(inf));
        Ok(Documento::new(raiz))
    }

    /// Serializa a DPS sem assinatura.
    pub fn para_xml(&self) -> Result<String, ErroNfse> {
        Ok(self.documento()?.to_xml()?)
    }

    /// Assina a DPS com XMLDSig sobre `infDPS`.
    pub fn assinar(&self, assinante: &dyn Signer) -> Result<Documento, ErroNfse> {
        let documento = self.documento()?;
        let id = self.id()?;
        Ok(assinar(&documento, id.as_str(), assinante)?)
    }

    /// Serializa a DPS assinada.
    pub fn para_xml_assinado(&self, assinante: &dyn Signer) -> Result<String, ErroNfse> {
        Ok(self.assinar(assinante)?.to_xml()?)
    }

    /// Valida a DPS contra um XSD informado pelo chamador.
    ///
    /// Os schemas oficiais não são embutidos no crate; veja `schemas/` no
    /// repositório e `docs/contratos-fiscais.md`.
    #[cfg(feature = "xsd")]
    pub fn validar_xsd(&self, esquema: impl AsRef<std::path::Path>) -> Result<(), ErroNfse> {
        let xml = self.para_xml()?;
        carimbar_xml::xsd::validar(xml.as_bytes(), esquema)
            .map_err(|erro| ErroNfse::Xsd(erro.to_string()))
    }
}

fn montar_substituicao(substituicao: &Substituicao) -> Elemento {
    let mut subst = el("subst")
        .com_filho(No::Elemento(txt("chSubstda", substituicao.chave.as_str())))
        .com_filho(No::Elemento(txt("cMotivo", substituicao.motivo.codigo())));
    if let Some(descricao) = &substituicao.descricao {
        subst = subst.com_filho(No::Elemento(txt("xMotivo", descricao)));
    }
    subst
}

fn montar_prestador(prestador: &Prestador) -> Elemento {
    let mut prest = el("prest").com_filho(No::Elemento(identificacao(&prestador.inscricao)));
    if let Some(im) = &prestador.inscricao_municipal {
        prest = prest.com_filho(No::Elemento(txt("IM", im)));
    }
    if let Some(nome) = &prestador.nome {
        prest = prest.com_filho(No::Elemento(txt("xNome", nome)));
    }
    if let Some(email) = &prestador.email {
        prest = prest.com_filho(No::Elemento(txt("email", email)));
    }
    let mut reg = el("regTrib").com_filho(No::Elemento(txt(
        "opSimpNac",
        prestador.regime.op_simples_nacional.codigo(),
    )));
    if let Some(regime) = prestador.regime.regime_apuracao {
        reg = reg.com_filho(No::Elemento(txt("regApTribSN", regime.codigo())));
    }
    reg = reg.com_filho(No::Elemento(txt(
        "regEspTrib",
        prestador.regime.regime_especial.codigo(),
    )));
    prest.com_filho(No::Elemento(reg))
}

fn montar_pessoa(local: &str, pessoa: &Pessoa) -> Elemento {
    let mut elemento = el(local).com_filho(No::Elemento(identificacao(&pessoa.inscricao)));
    if let Some(im) = &pessoa.inscricao_municipal {
        elemento = elemento.com_filho(No::Elemento(txt("IM", im)));
    }
    if let Some(nome) = &pessoa.nome {
        elemento = elemento.com_filho(No::Elemento(txt("xNome", nome)));
    }
    if let Some(email) = &pessoa.email {
        elemento = elemento.com_filho(No::Elemento(txt("email", email)));
    }
    elemento
}

fn identificacao(inscricao: &InscricaoFederal) -> Elemento {
    match inscricao {
        InscricaoFederal::Cnpj(cnpj) => txt("CNPJ", cnpj.as_str()),
        InscricaoFederal::Cpf(cpf) => txt("CPF", cpf),
    }
}

fn montar_servico(servico: &Servico) -> Elemento {
    let loc = match &servico.local_prestacao {
        LocalPrestacao::Municipio(municipio) => {
            el("locPrest").com_filho(No::Elemento(txt("cLocPrestacao", municipio.as_str())))
        }
        LocalPrestacao::Pais(pais) => {
            el("locPrest").com_filho(No::Elemento(txt("cPaisPrestacao", pais)))
        }
    };
    let mut c_serv = el("cServ").com_filho(No::Elemento(txt(
        "cTribNac",
        &servico.codigo_tributacao_nacional,
    )));
    if let Some(codigo) = &servico.codigo_tributacao_municipal {
        c_serv = c_serv.com_filho(No::Elemento(txt("cTribMun", codigo)));
    }
    if let Some(codigo) = &servico.codigo_nbs {
        c_serv = c_serv.com_filho(No::Elemento(txt("cNBS", codigo)));
    }
    if let Some(codigo) = &servico.codigo_interno {
        c_serv = c_serv.com_filho(No::Elemento(txt("cIntContrib", codigo)));
    }
    c_serv = c_serv.com_filho(No::Elemento(txt("xDescServ", &servico.descricao)));
    let mut serv = el("serv")
        .com_filho(No::Elemento(loc))
        .com_filho(No::Elemento(c_serv));
    if let Some(complementares) = &servico.informacoes_complementares {
        serv = serv.com_filho(No::Elemento(
            el("infoCompl").com_filho(No::Elemento(txt("xInfComp", complementares))),
        ));
    }
    serv
}

fn montar_valores(valores: &Valores) -> Elemento {
    let mut servico_prestado = el("vServPrest");
    if let Some(recebido) = valores.valor_recebido {
        servico_prestado =
            servico_prestado.com_filho(No::Elemento(txt("vReceb", recebido.para_xml())));
    }
    servico_prestado =
        servico_prestado.com_filho(No::Elemento(txt("vServ", valores.valor_servico.para_xml())));

    let mut raiz = el("valores").com_filho(No::Elemento(servico_prestado));
    if valores.desconto_incondicional.is_some() || valores.desconto_condicional.is_some() {
        let mut descontos = el("vDescCondIncond");
        if let Some(valor) = valores.desconto_incondicional {
            descontos = descontos.com_filho(No::Elemento(txt("vDescIncond", valor.para_xml())));
        }
        if let Some(valor) = valores.desconto_condicional {
            descontos = descontos.com_filho(No::Elemento(txt("vDescCond", valor.para_xml())));
        }
        raiz = raiz.com_filho(No::Elemento(descontos));
    }
    raiz = raiz.com_filho(No::Elemento(montar_tributos(valores)));
    raiz
}

fn montar_tributos(valores: &Valores) -> Elemento {
    let mut municipal = el("tribMun").com_filho(No::Elemento(txt(
        "tribISSQN",
        valores.tributo_municipal.trib_issqn.codigo(),
    )));
    municipal = municipal.com_filho(No::Elemento(txt(
        "tpRetISSQN",
        valores.tributo_municipal.tipo_retencao.codigo(),
    )));
    if let Some(aliquota) = valores.tributo_municipal.aliquota {
        municipal = municipal.com_filho(No::Elemento(txt("pAliq", formatar_decimal(aliquota))));
    }
    let total = match valores.total_tributos {
        TotalTributos::IndicadorZero => el("totTrib").com_filho(No::Elemento(txt("indTotTrib", 0))),
        TotalTributos::PercentualSimplesNacional(percentual) => el("totTrib").com_filho(
            No::Elemento(txt("pTotTribSN", formatar_decimal(percentual))),
        ),
    };
    el("trib")
        .com_filho(No::Elemento(municipal))
        .com_filho(No::Elemento(total))
}

fn montar_ibs_cbs(ibs_cbs: &IbsCbs) -> Elemento {
    let mut raiz = el("IBSCBS");
    if let Some(ind_final) = ibs_cbs.ind_final {
        raiz = raiz.com_filho(No::Elemento(txt("indFinal", u8::from(ind_final))));
    }
    if let Some(c_ind_op) = &ibs_cbs.c_ind_op {
        raiz = raiz.com_filho(No::Elemento(txt("cIndOp", c_ind_op)));
    }
    if let Some(tp_oper) = ibs_cbs.tp_oper {
        raiz = raiz.com_filho(No::Elemento(txt("tpOper", tp_oper)));
    }
    raiz = raiz.com_filho(No::Elemento(txt("indDest", ibs_cbs.ind_dest)));
    if let Some(destinatario) = &ibs_cbs.destinatario {
        let mut dest = el("dest").com_filho(No::Elemento(identificacao(&destinatario.inscricao)));
        dest = dest.com_filho(No::Elemento(txt("xNome", &destinatario.nome)));
        raiz = raiz.com_filho(No::Elemento(dest));
    }
    let trib = el("trib")
        .com_filho(No::Elemento(txt("CST", &ibs_cbs.cst)))
        .com_filho(No::Elemento(txt("cClassTrib", &ibs_cbs.c_class_trib)));
    let valores = el("valores").com_filho(No::Elemento(trib));
    raiz.com_filho(No::Elemento(valores))
}

fn validar_prestador(prestador: &Prestador) -> Result<(), ErroNfse> {
    if let Some(im) = &prestador.inscricao_municipal {
        exigir_texto("prest/IM", im, 1, 15)?;
    }
    if let Some(nome) = &prestador.nome {
        exigir_texto("prest/xNome", nome, 1, 300)?;
    }
    if let Some(email) = &prestador.email {
        exigir_texto("prest/email", email, 1, 80)?;
    }
    if prestador.regime.regime_apuracao.is_some()
        && prestador.regime.op_simples_nacional != OpSimplesNacional::MeEpp
    {
        return Err(ErroNfse::campo(
            "prest/regTrib/regApTribSN",
            "só é aceito para opSimpNac = 3 (ME/EPP)",
        ));
    }
    Ok(())
}

fn validar_pessoa(campo: &'static str, pessoa: &Pessoa) -> Result<(), ErroNfse> {
    if let Some(im) = &pessoa.inscricao_municipal {
        exigir_texto("IM", im, 1, 15).map_err(|_| ErroNfse::campo(campo, "IM inválida"))?;
    }
    if let Some(nome) = &pessoa.nome {
        exigir_texto("xNome", nome, 1, 300)?;
    }
    if let Some(email) = &pessoa.email {
        exigir_texto("email", email, 1, 80)?;
    }
    Ok(())
}

fn validar_servico(servico: &Servico) -> Result<(), ErroNfse> {
    if let LocalPrestacao::Pais(pais) = &servico.local_prestacao
        && (pais.len() != 2 || !pais.bytes().all(|b| b.is_ascii_uppercase()))
    {
        return Err(ErroNfse::campo(
            "serv/locPrest/cPaisPrestacao",
            "esperado código ISO de 2 letras maiúsculas",
        ));
    }
    exigir_digitos(
        "serv/cServ/cTribNac",
        &servico.codigo_tributacao_nacional,
        6,
    )?;
    if let Some(codigo) = &servico.codigo_tributacao_municipal {
        exigir_digitos("serv/cServ/cTribMun", codigo, 3)?;
    }
    if let Some(codigo) = &servico.codigo_nbs {
        exigir_digitos("serv/cServ/cNBS", codigo, 9)?;
    }
    if let Some(codigo) = &servico.codigo_interno {
        let valido =
            (1..=20).contains(&codigo.len()) && codigo.bytes().all(|b| b.is_ascii_alphanumeric());
        if !valido {
            return Err(ErroNfse::campo(
                "serv/cServ/cIntContrib",
                "esperado 1 a 20 letras ou dígitos",
            ));
        }
    }
    exigir_texto("serv/cServ/xDescServ", &servico.descricao, 1, 2000)?;
    if let Some(complementares) = &servico.informacoes_complementares {
        exigir_texto("serv/infoCompl/xInfComp", complementares, 1, 2000)?;
    }
    Ok(())
}

fn validar_valores(valores: &Valores) -> Result<(), ErroNfse> {
    validar_dinheiro("valores/vServPrest/vServ", valores.valor_servico)?;
    if let Some(valor) = valores.valor_recebido {
        validar_dinheiro("valores/vServPrest/vReceb", valor)?;
    }
    if let Some(valor) = valores.desconto_incondicional {
        validar_dinheiro("valores/vDescCondIncond/vDescIncond", valor)?;
    }
    if let Some(valor) = valores.desconto_condicional {
        validar_dinheiro("valores/vDescCondIncond/vDescCond", valor)?;
    }
    if let Some(aliquota) = valores.tributo_municipal.aliquota
        && (aliquota.is_sign_negative() || aliquota.scale() > 2 || aliquota > Decimal::new(999, 2))
    {
        return Err(ErroNfse::campo(
            "valores/trib/tribMun/pAliq",
            "percentual entre 0 e 9,99 com até 2 casas",
        ));
    }
    if let TotalTributos::PercentualSimplesNacional(percentual) = valores.total_tributos
        && (percentual.is_sign_negative()
            || percentual.scale() > 2
            || percentual > Decimal::new(9999, 2))
    {
        return Err(ErroNfse::campo(
            "valores/trib/totTrib/pTotTribSN",
            "percentual entre 0 e 99,99 com até 2 casas",
        ));
    }
    Ok(())
}

fn validar_dinheiro(campo: &'static str, valor: Dinheiro) -> Result<(), ErroNfse> {
    // TSDec15V2: até 15 dígitos inteiros, ponto e duas casas.
    if valor.para_xml().len() > 18 {
        return Err(ErroNfse::campo(campo, "valor acima de 15 dígitos inteiros"));
    }
    Ok(())
}

fn validar_ibs_cbs(ibs_cbs: &IbsCbs) -> Result<(), ErroNfse> {
    if ibs_cbs.ind_dest > 1 {
        return Err(ErroNfse::campo("IBSCBS/indDest", "esperado 0 ou 1"));
    }
    if let Some(c_ind_op) = &ibs_cbs.c_ind_op {
        exigir_digitos("IBSCBS/cIndOp", c_ind_op, 6)?;
    }
    if let Some(tp_oper) = ibs_cbs.tp_oper
        && !(1..=6).contains(&tp_oper)
    {
        return Err(ErroNfse::campo("IBSCBS/tpOper", "esperado 1..=6"));
    }
    if ibs_cbs.ind_dest == 1 && ibs_cbs.destinatario.is_none() {
        return Err(ErroNfse::campo(
            "IBSCBS/dest",
            "obrigatório quando indDest = 1",
        ));
    }
    if let Some(destinatario) = &ibs_cbs.destinatario {
        exigir_texto("IBSCBS/dest/xNome", &destinatario.nome, 1, 150)?;
    }
    exigir_digitos("IBSCBS/valores/trib/CST", &ibs_cbs.cst, 3)?;
    exigir_digitos("IBSCBS/valores/trib/cClassTrib", &ibs_cbs.c_class_trib, 6)?;
    Ok(())
}

pub(crate) fn el(local: &str) -> Elemento {
    Elemento::new(Nome::new(local))
}

pub(crate) fn txt(local: &str, valor: impl ToString) -> Elemento {
    el(local).com_texto(valor.to_string())
}

pub(crate) fn exigir_texto(
    campo: &'static str,
    valor: &str,
    minimo: usize,
    maximo: usize,
) -> Result<(), ErroNfse> {
    let tamanho = valor.chars().count();
    if tamanho < minimo || tamanho > maximo {
        return Err(ErroNfse::campo(
            campo,
            format!("esperado {minimo}..={maximo} caracteres, recebido {tamanho}"),
        ));
    }
    Ok(())
}

pub(crate) fn exigir_digitos(
    campo: &'static str,
    valor: &str,
    tamanho: usize,
) -> Result<(), ErroNfse> {
    if valor.len() != tamanho || !valor.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ErroNfse::campo(
            campo,
            format!("esperado {tamanho} dígitos: {valor:?}"),
        ));
    }
    Ok(())
}

pub(crate) fn formatar_decimal(valor: Decimal) -> String {
    format!("{valor:.2}")
}

impl fmt::Display for TipoEmitente {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.codigo())
    }
}
