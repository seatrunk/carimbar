use carimbar_types::{Ambiente, TpAmb};

use crate::erro::ErroEndpoints;
use crate::{Endpoint, Fonte, Uf, VERIFICADO_EM};

/// Autorizador da NF-e.
///
/// A lista reflete os autorizadores publicados no Portal Nacional da NF-e em
/// [`VERIFICADO_EM`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Autorizador {
    /// SEFAZ Amazonas.
    Am,
    /// SEFAZ Bahia.
    Ba,
    /// SEFAZ Goiás.
    Go,
    /// SEFAZ Minas Gerais.
    Mg,
    /// SEFAZ Mato Grosso do Sul.
    Ms,
    /// SEFAZ Mato Grosso.
    Mt,
    /// SEFAZ Pernambuco.
    Pe,
    /// SEFAZ Paraná.
    Pr,
    /// SEFAZ Rio Grande do Sul.
    Rs,
    /// SEFAZ São Paulo.
    Sp,
    /// SEFAZ Virtual do Ambiente Nacional.
    Svan,
    /// SEFAZ Virtual do Rio Grande do Sul.
    Svrs,
    /// SEFAZ Virtual de Contingência do Ambiente Nacional.
    SvcAn,
    /// SEFAZ Virtual de Contingência do Rio Grande do Sul.
    SvcRs,
    /// Ambiente Nacional (distribuição de DF-e).
    An,
}

impl Autorizador {
    /// Sigla usada nos portais oficiais.
    pub fn sigla(self) -> &'static str {
        match self {
            Self::Am => "AM",
            Self::Ba => "BA",
            Self::Go => "GO",
            Self::Mg => "MG",
            Self::Ms => "MS",
            Self::Mt => "MT",
            Self::Pe => "PE",
            Self::Pr => "PR",
            Self::Rs => "RS",
            Self::Sp => "SP",
            Self::Svan => "SVAN",
            Self::Svrs => "SVRS",
            Self::SvcAn => "SVC-AN",
            Self::SvcRs => "SVC-RS",
            Self::An => "AN",
        }
    }
}

/// Sefaz Virtual de Contingência usada pela UF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Contingencia {
    /// SVC do Ambiente Nacional.
    SvcAn,
    /// SVC do Rio Grande do Sul.
    SvcRs,
}

impl Contingencia {
    /// Autorizador correspondente.
    pub fn autorizador(self) -> Autorizador {
        match self {
            Self::SvcAn => Autorizador::SvcAn,
            Self::SvcRs => Autorizador::SvcRs,
        }
    }
}

/// Serviço da NF-e modelo 55.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ServicoNfe {
    /// Envio do lote de NF-e (`NFeAutorizacao4`).
    Autorizacao,
    /// Consulta do resultado do lote (`NFeRetAutorizacao4`).
    RetAutorizacao,
    /// Consulta do protocolo pela chave (`NFeConsultaProtocolo4`).
    ConsultaProtocolo,
    /// Status do serviço (`NFeStatusServico4`).
    StatusServico,
    /// Inutilização de numeração (`NFeInutilizacao4`).
    Inutilizacao,
    /// Recepção de eventos (`NFeRecepcaoEvento4`).
    RecepcaoEvento,
    /// Consulta do cadastro de contribuinte (`CadConsultaCadastro4`).
    ConsultaCadastro,
    /// Distribuição de DF-e do Ambiente Nacional (`NFeDistribuicaoDFe`).
    DistribuicaoDfe,
}

impl ServicoNfe {
    /// Nome do serviço para diagnóstico.
    pub fn nome(self) -> &'static str {
        match self {
            Self::Autorizacao => "NFeAutorizacao",
            Self::RetAutorizacao => "NFeRetAutorizacao",
            Self::ConsultaProtocolo => "NFeConsultaProtocolo",
            Self::StatusServico => "NFeStatusServico",
            Self::Inutilizacao => "NFeInutilizacao",
            Self::RecepcaoEvento => "NFeRecepcaoEvento",
            Self::ConsultaCadastro => "CadConsultaCadastro",
            Self::DistribuicaoDfe => "NFeDistribuicaoDFe",
        }
    }

    /// Versão do leiaute publicada para o serviço.
    pub fn versao(self) -> &'static str {
        match self {
            Self::DistribuicaoDfe => "1.00",
            _ => "4.00",
        }
    }
}

/// Consulta de URL da NF-e.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsultaNfe {
    /// UF do emitente ou do ator que consulta.
    pub uf: Uf,
    /// Serviço desejado.
    pub servico: ServicoNfe,
    /// Ambiente do XML e do endpoint.
    pub tp_amb: TpAmb,
    /// Contingência explícita; ausente usa o autorizador normal da UF.
    pub contingencia: Option<Contingencia>,
}

impl ConsultaNfe {
    /// Consulta no autorizador normal da UF.
    pub fn new(uf: Uf, servico: ServicoNfe, tp_amb: TpAmb) -> Self {
        Self {
            uf,
            servico,
            tp_amb,
            contingencia: None,
        }
    }

    /// Consulta com o `tpAmb` retirado do [`Ambiente`] já validado pelo chamador.
    ///
    /// É a forma preferida: a mesma fonte decide o XML e a URL.
    pub fn para(ambiente: &Ambiente, uf: Uf, servico: ServicoNfe) -> Self {
        Self::new(uf, servico, ambiente.tp_amb())
    }

    /// Consulta em uma Sefaz Virtual de Contingência.
    pub fn em_contingencia(mut self, contingencia: Contingencia) -> Self {
        self.contingencia = Some(contingencia);
        self
    }
}

/// Autorizador normal da UF para os serviços do sistema NF-e.
pub fn autorizador_principal(uf: Uf) -> Autorizador {
    match uf {
        Uf::Amazonas => Autorizador::Am,
        Uf::Bahia => Autorizador::Ba,
        Uf::Goias => Autorizador::Go,
        Uf::MinasGerais => Autorizador::Mg,
        Uf::MatoGrossoDoSul => Autorizador::Ms,
        Uf::MatoGrosso => Autorizador::Mt,
        Uf::Pernambuco => Autorizador::Pe,
        Uf::Parana => Autorizador::Pr,
        Uf::RioGrandeDoSul => Autorizador::Rs,
        Uf::SaoPaulo => Autorizador::Sp,
        Uf::Maranhao => Autorizador::Svan,
        Uf::Acre
        | Uf::Alagoas
        | Uf::Amapa
        | Uf::Ceara
        | Uf::DistritoFederal
        | Uf::EspiritoSanto
        | Uf::Para
        | Uf::Paraiba
        | Uf::Piaui
        | Uf::RioDeJaneiro
        | Uf::RioGrandeDoNorte
        | Uf::Rondonia
        | Uf::Roraima
        | Uf::SantaCatarina
        | Uf::Sergipe
        | Uf::Tocantins => Autorizador::Svrs,
    }
}

fn svc_an_atende(uf: Uf) -> bool {
    matches!(
        uf,
        Uf::Acre
            | Uf::Alagoas
            | Uf::Amapa
            | Uf::Ceara
            | Uf::DistritoFederal
            | Uf::EspiritoSanto
            | Uf::MinasGerais
            | Uf::Para
            | Uf::Paraiba
            | Uf::Piaui
            | Uf::RioDeJaneiro
            | Uf::RioGrandeDoNorte
            | Uf::Rondonia
            | Uf::Roraima
            | Uf::RioGrandeDoSul
            | Uf::SantaCatarina
            | Uf::Sergipe
            | Uf::SaoPaulo
            | Uf::Tocantins
    )
}

fn svc_rs_atende(uf: Uf) -> bool {
    matches!(
        uf,
        Uf::Amazonas
            | Uf::Bahia
            | Uf::Goias
            | Uf::Maranhao
            | Uf::MatoGrossoDoSul
            | Uf::MatoGrosso
            | Uf::Pernambuco
            | Uf::Parana
    )
}

/// Resolve a URL de um serviço da NF-e.
pub fn resolver_nfe(consulta: &ConsultaNfe) -> Result<Endpoint, ErroEndpoints> {
    if consulta.servico == ServicoNfe::DistribuicaoDfe {
        return Ok(Endpoint {
            url: distribicao_dfe(consulta.tp_amb),
            versao: ServicoNfe::DistribuicaoDfe.versao(),
            autorizador: Some(Autorizador::An),
            fonte: Fonte::PortalNfeNacional,
            verificado_em: VERIFICADO_EM,
        });
    }

    let autorizador = match consulta.contingencia {
        Some(Contingencia::SvcAn) if svc_an_atende(consulta.uf) => Autorizador::SvcAn,
        Some(Contingencia::SvcRs) if svc_rs_atende(consulta.uf) => Autorizador::SvcRs,
        Some(outra) => {
            return Err(ErroEndpoints::NaoDisponivel {
                servico: consulta.servico.nome(),
                contexto: format!(
                    "{} ({} não atende)",
                    consulta.uf,
                    outra.autorizador().sigla()
                ),
                ambiente: ambiente(consulta.tp_amb),
            });
        }
        None => autorizador_principal(consulta.uf),
    };

    // A contingência cobre autorização, retorno, consulta, status, inutilização
    // e eventos; consulta de cadastro continua no autorizador normal.
    if consulta.contingencia.is_some() && consulta.servico == ServicoNfe::ConsultaCadastro {
        return Err(ErroEndpoints::NaoDisponivel {
            servico: consulta.servico.nome(),
            contexto: format!("{} em contingência", consulta.uf),
            ambiente: ambiente(consulta.tp_amb),
        });
    }

    // A SVRS só publica consulta de cadastro para cinco UFs.
    if autorizador == Autorizador::Svrs
        && consulta.servico == ServicoNfe::ConsultaCadastro
        && !matches!(
            consulta.uf,
            Uf::Acre | Uf::EspiritoSanto | Uf::RioGrandeDoNorte | Uf::Paraiba | Uf::SantaCatarina
        )
    {
        return Err(ErroEndpoints::NaoDisponivel {
            servico: consulta.servico.nome(),
            contexto: format!("{} via {}", consulta.uf, autorizador.sigla()),
            ambiente: ambiente(consulta.tp_amb),
        });
    }

    let urls = urls(autorizador, consulta.tp_amb);
    let url = urls
        .para(consulta.servico)
        .ok_or_else(|| ErroEndpoints::NaoDisponivel {
            servico: consulta.servico.nome(),
            contexto: format!("{} via {}", consulta.uf, autorizador.sigla()),
            ambiente: ambiente(consulta.tp_amb),
        })?;

    Ok(Endpoint {
        url: url.to_owned(),
        versao: consulta.servico.versao(),
        autorizador: Some(autorizador),
        fonte: Fonte::PortalNfeNacional,
        verificado_em: VERIFICADO_EM,
    })
}

fn ambiente(tp_amb: TpAmb) -> &'static str {
    match tp_amb {
        TpAmb::Producao => "produção",
        TpAmb::Homologacao => "homologação",
    }
}

fn distribicao_dfe(tp_amb: TpAmb) -> String {
    match tp_amb {
        TpAmb::Producao => {
            "https://www1.nfe.fazenda.gov.br/NFeDistribuicaoDFe/NFeDistribuicaoDFe.asmx"
        }
        TpAmb::Homologacao => {
            "https://hom1.nfe.fazenda.gov.br/NFeDistribuicaoDFe/NFeDistribuicaoDFe.asmx"
        }
    }
    .to_owned()
}

struct UrlsNfe {
    autorizacao: &'static str,
    ret_autorizacao: &'static str,
    consulta_protocolo: &'static str,
    status_servico: &'static str,
    inutilizacao: &'static str,
    recepcao_evento: &'static str,
    consulta_cadastro: Option<&'static str>,
}

impl UrlsNfe {
    fn para(&self, servico: ServicoNfe) -> Option<&'static str> {
        let url = match servico {
            ServicoNfe::Autorizacao => self.autorizacao,
            ServicoNfe::RetAutorizacao => self.ret_autorizacao,
            ServicoNfe::ConsultaProtocolo => self.consulta_protocolo,
            ServicoNfe::StatusServico => self.status_servico,
            ServicoNfe::Inutilizacao => self.inutilizacao,
            ServicoNfe::RecepcaoEvento => self.recepcao_evento,
            ServicoNfe::ConsultaCadastro => self.consulta_cadastro?,
            ServicoNfe::DistribuicaoDfe => return None,
        };
        Some(url)
    }
}

const CAD_SVRS: &str =
    "https://cad.svrs.rs.gov.br/ws/cadconsultacadastro/cadconsultacadastro4.asmx";

fn urls(autorizador: Autorizador, tp_amb: TpAmb) -> &'static UrlsNfe {
    match (autorizador, tp_amb) {
        (Autorizador::Am, TpAmb::Producao) => &AM_PROD,
        (Autorizador::Am, TpAmb::Homologacao) => &AM_HOM,
        (Autorizador::Ba, TpAmb::Producao) => &BA_PROD,
        (Autorizador::Ba, TpAmb::Homologacao) => &BA_HOM,
        (Autorizador::Go, TpAmb::Producao) => &GO_PROD,
        (Autorizador::Go, TpAmb::Homologacao) => &GO_HOM,
        (Autorizador::Mg, TpAmb::Producao) => &MG_PROD,
        (Autorizador::Mg, TpAmb::Homologacao) => &MG_HOM,
        (Autorizador::Ms, TpAmb::Producao) => &MS_PROD,
        (Autorizador::Ms, TpAmb::Homologacao) => &MS_HOM,
        (Autorizador::Mt, TpAmb::Producao) => &MT_PROD,
        (Autorizador::Mt, TpAmb::Homologacao) => &MT_HOM,
        (Autorizador::Pe, TpAmb::Producao) => &PE_PROD,
        (Autorizador::Pe, TpAmb::Homologacao) => &PE_HOM,
        (Autorizador::Pr, TpAmb::Producao) => &PR_PROD,
        (Autorizador::Pr, TpAmb::Homologacao) => &PR_HOM,
        (Autorizador::Rs, TpAmb::Producao) => &RS_PROD,
        (Autorizador::Rs, TpAmb::Homologacao) => &RS_HOM,
        (Autorizador::Sp, TpAmb::Producao) => &SP_PROD,
        (Autorizador::Sp, TpAmb::Homologacao) => &SP_HOM,
        (Autorizador::Svan, TpAmb::Producao) => &SVAN_PROD,
        (Autorizador::Svan, TpAmb::Homologacao) => &SVAN_HOM,
        (Autorizador::Svrs, TpAmb::Producao) => &SVRS_PROD,
        (Autorizador::Svrs, TpAmb::Homologacao) => &SVRS_HOM,
        // A SVC-AN de produção foi unificada com a SVAN em 2025; a de
        // homologação usa o mesmo host `hom.sefazvirtual`.
        (Autorizador::SvcAn, TpAmb::Producao) => &SVAN_PROD,
        (Autorizador::SvcAn, TpAmb::Homologacao) => &SVAN_HOM,
        // A SVC-RS é servida pelos mesmos endereços da SVRS.
        (Autorizador::SvcRs, TpAmb::Producao) => &SVRS_PROD,
        (Autorizador::SvcRs, TpAmb::Homologacao) => &SVRS_HOM,
        (Autorizador::An, _) => &AN,
    }
}

const AN: UrlsNfe = UrlsNfe {
    autorizacao: "https://www1.nfe.fazenda.gov.br/NFeDistribuicaoDFe/NFeDistribuicaoDFe.asmx",
    ret_autorizacao: "",
    consulta_protocolo: "",
    status_servico: "",
    inutilizacao: "",
    recepcao_evento: "",
    consulta_cadastro: None,
};

const AM_PROD: UrlsNfe = UrlsNfe {
    autorizacao: "https://nfe.sefaz.am.gov.br/services2/services/NfeAutorizacao4",
    ret_autorizacao: "https://nfe.sefaz.am.gov.br/services2/services/NfeRetAutorizacao4",
    consulta_protocolo: "https://nfe.sefaz.am.gov.br/services2/services/NfeConsulta4",
    status_servico: "https://nfe.sefaz.am.gov.br/services2/services/NfeStatusServico4",
    inutilizacao: "https://nfe.sefaz.am.gov.br/services2/services/NfeInutilizacao4",
    recepcao_evento: "https://nfe.sefaz.am.gov.br/services2/services/RecepcaoEvento4",
    consulta_cadastro: None,
};

const AM_HOM: UrlsNfe = UrlsNfe {
    autorizacao: "https://homnfe.sefaz.am.gov.br/services2/services/NfeAutorizacao4",
    ret_autorizacao: "https://homnfe.sefaz.am.gov.br/services2/services/NfeRetAutorizacao4",
    consulta_protocolo: "https://homnfe.sefaz.am.gov.br/services2/services/NfeConsulta4",
    status_servico: "https://homnfe.sefaz.am.gov.br/services2/services/NfeStatusServico4",
    inutilizacao: "https://homnfe.sefaz.am.gov.br/services2/services/NfeInutilizacao4",
    recepcao_evento: "https://homnfe.sefaz.am.gov.br/services2/services/RecepcaoEvento4",
    consulta_cadastro: None,
};

const BA_PROD: UrlsNfe = UrlsNfe {
    autorizacao: "https://nfe.sefaz.ba.gov.br/webservices/NFeAutorizacao4/NFeAutorizacao4.asmx",
    ret_autorizacao: "https://nfe.sefaz.ba.gov.br/webservices/NFeRetAutorizacao4/NFeRetAutorizacao4.asmx",
    consulta_protocolo: "https://nfe.sefaz.ba.gov.br/webservices/NFeConsultaProtocolo4/NFeConsultaProtocolo4.asmx",
    status_servico: "https://nfe.sefaz.ba.gov.br/webservices/NFeStatusServico4/NFeStatusServico4.asmx",
    inutilizacao: "https://nfe.sefaz.ba.gov.br/webservices/NFeInutilizacao4/NFeInutilizacao4.asmx",
    recepcao_evento: "https://nfe.sefaz.ba.gov.br/webservices/NFeRecepcaoEvento4/NFeRecepcaoEvento4.asmx",
    consulta_cadastro: Some(
        "https://nfe.sefaz.ba.gov.br/webservices/CadConsultaCadastro4/CadConsultaCadastro4.asmx",
    ),
};

const BA_HOM: UrlsNfe = UrlsNfe {
    autorizacao: "https://hnfe.sefaz.ba.gov.br/webservices/NFeAutorizacao4/NFeAutorizacao4.asmx",
    ret_autorizacao: "https://hnfe.sefaz.ba.gov.br/webservices/NFeRetAutorizacao4/NFeRetAutorizacao4.asmx",
    consulta_protocolo: "https://hnfe.sefaz.ba.gov.br/webservices/NFeConsultaProtocolo4/NFeConsultaProtocolo4.asmx",
    status_servico: "https://hnfe.sefaz.ba.gov.br/webservices/NFeStatusServico4/NFeStatusServico4.asmx",
    inutilizacao: "https://hnfe.sefaz.ba.gov.br/webservices/NFeInutilizacao4/NFeInutilizacao4.asmx",
    recepcao_evento: "https://hnfe.sefaz.ba.gov.br/webservices/NFeRecepcaoEvento4/NFeRecepcaoEvento4.asmx",
    consulta_cadastro: Some(
        "https://hnfe.sefaz.ba.gov.br/webservices/CadConsultaCadastro4/CadConsultaCadastro4.asmx",
    ),
};

const GO_PROD: UrlsNfe = UrlsNfe {
    autorizacao: "https://nfe.sefaz.go.gov.br/nfe/services/NFeAutorizacao4",
    ret_autorizacao: "https://nfe.sefaz.go.gov.br/nfe/services/NFeRetAutorizacao4",
    consulta_protocolo: "https://nfe.sefaz.go.gov.br/nfe/services/NFeConsultaProtocolo4",
    status_servico: "https://nfe.sefaz.go.gov.br/nfe/services/NFeStatusServico4",
    inutilizacao: "https://nfe.sefaz.go.gov.br/nfe/services/NFeInutilizacao4",
    recepcao_evento: "https://nfe.sefaz.go.gov.br/nfe/services/NFeRecepcaoEvento4",
    consulta_cadastro: Some("https://nfe.sefaz.go.gov.br/nfe/services/CadConsultaCadastro4"),
};

const GO_HOM: UrlsNfe = UrlsNfe {
    autorizacao: "https://homolog.sefaz.go.gov.br/nfe/services/NFeAutorizacao4",
    ret_autorizacao: "https://homolog.sefaz.go.gov.br/nfe/services/NFeRetAutorizacao4",
    consulta_protocolo: "https://homolog.sefaz.go.gov.br/nfe/services/NFeConsultaProtocolo4",
    status_servico: "https://homolog.sefaz.go.gov.br/nfe/services/NFeStatusServico4",
    inutilizacao: "https://homolog.sefaz.go.gov.br/nfe/services/NFeInutilizacao4",
    recepcao_evento: "https://homolog.sefaz.go.gov.br/nfe/services/NFeRecepcaoEvento4",
    consulta_cadastro: Some("https://homolog.sefaz.go.gov.br/nfe/services/CadConsultaCadastro4"),
};

const MG_PROD: UrlsNfe = UrlsNfe {
    autorizacao: "https://nfe.fazenda.mg.gov.br/nfe2/services/NFeAutorizacao4",
    ret_autorizacao: "https://nfe.fazenda.mg.gov.br/nfe2/services/NFeRetAutorizacao4",
    consulta_protocolo: "https://nfe.fazenda.mg.gov.br/nfe2/services/NFeConsultaProtocolo4",
    status_servico: "https://nfe.fazenda.mg.gov.br/nfe2/services/NFeStatusServico4",
    inutilizacao: "https://nfe.fazenda.mg.gov.br/nfe2/services/NFeInutilizacao4",
    recepcao_evento: "https://nfe.fazenda.mg.gov.br/nfe2/services/NFeRecepcaoEvento4",
    consulta_cadastro: None,
};

const MG_HOM: UrlsNfe = UrlsNfe {
    autorizacao: "https://hnfe.fazenda.mg.gov.br/nfe2/services/NFeAutorizacao4",
    ret_autorizacao: "https://hnfe.fazenda.mg.gov.br/nfe2/services/NFeRetAutorizacao4",
    consulta_protocolo: "https://hnfe.fazenda.mg.gov.br/nfe2/services/NFeConsultaProtocolo4",
    status_servico: "https://hnfe.fazenda.mg.gov.br/nfe2/services/NFeStatusServico4",
    inutilizacao: "https://hnfe.fazenda.mg.gov.br/nfe2/services/NFeInutilizacao4",
    recepcao_evento: "https://hnfe.fazenda.mg.gov.br/nfe2/services/NFeRecepcaoEvento4",
    consulta_cadastro: None,
};

const MS_PROD: UrlsNfe = UrlsNfe {
    autorizacao: "https://nfe.sefaz.ms.gov.br/ws/NFeAutorizacao4",
    ret_autorizacao: "https://nfe.sefaz.ms.gov.br/ws/NFeRetAutorizacao4",
    consulta_protocolo: "https://nfe.sefaz.ms.gov.br/ws/NFeConsultaProtocolo4",
    status_servico: "https://nfe.sefaz.ms.gov.br/ws/NFeStatusServico4",
    inutilizacao: "https://nfe.sefaz.ms.gov.br/ws/NFeInutilizacao4",
    recepcao_evento: "https://nfe.sefaz.ms.gov.br/ws/NFeRecepcaoEvento4",
    consulta_cadastro: Some("https://nfe.sefaz.ms.gov.br/ws/CadConsultaCadastro4"),
};

const MS_HOM: UrlsNfe = UrlsNfe {
    autorizacao: "https://hom.nfe.sefaz.ms.gov.br/ws/NFeAutorizacao4",
    ret_autorizacao: "https://hom.nfe.sefaz.ms.gov.br/ws/NFeRetAutorizacao4",
    consulta_protocolo: "https://hom.nfe.sefaz.ms.gov.br/ws/NFeConsultaProtocolo4",
    status_servico: "https://hom.nfe.sefaz.ms.gov.br/ws/NFeStatusServico4",
    inutilizacao: "https://hom.nfe.sefaz.ms.gov.br/ws/NFeInutilizacao4",
    recepcao_evento: "https://hom.nfe.sefaz.ms.gov.br/ws/NFeRecepcaoEvento4",
    consulta_cadastro: Some("https://hom.nfe.sefaz.ms.gov.br/ws/CadConsultaCadastro4"),
};

const MT_PROD: UrlsNfe = UrlsNfe {
    autorizacao: "https://nfe.sefaz.mt.gov.br/nfews/v2/services/NfeAutorizacao4",
    ret_autorizacao: "https://nfe.sefaz.mt.gov.br/nfews/v2/services/NfeRetAutorizacao4",
    consulta_protocolo: "https://nfe.sefaz.mt.gov.br/nfews/v2/services/NfeConsulta4",
    status_servico: "https://nfe.sefaz.mt.gov.br/nfews/v2/services/NfeStatusServico4",
    inutilizacao: "https://nfe.sefaz.mt.gov.br/nfews/v2/services/NfeInutilizacao4",
    recepcao_evento: "https://nfe.sefaz.mt.gov.br/nfews/v2/services/RecepcaoEvento4",
    consulta_cadastro: Some("https://nfe.sefaz.mt.gov.br/nfews/v2/services/CadConsultaCadastro4"),
};

const MT_HOM: UrlsNfe = UrlsNfe {
    autorizacao: "https://homologacao.sefaz.mt.gov.br/nfews/v2/services/NfeAutorizacao4",
    ret_autorizacao: "https://homologacao.sefaz.mt.gov.br/nfews/v2/services/NfeRetAutorizacao4",
    consulta_protocolo: "https://homologacao.sefaz.mt.gov.br/nfews/v2/services/NfeConsulta4",
    status_servico: "https://homologacao.sefaz.mt.gov.br/nfews/v2/services/NfeStatusServico4",
    inutilizacao: "https://homologacao.sefaz.mt.gov.br/nfews/v2/services/NfeInutilizacao4",
    recepcao_evento: "https://homologacao.sefaz.mt.gov.br/nfews/v2/services/RecepcaoEvento4",
    consulta_cadastro: Some(
        "https://homologacao.sefaz.mt.gov.br/nfews/v2/services/CadConsultaCadastro4",
    ),
};

const PE_PROD: UrlsNfe = UrlsNfe {
    autorizacao: "https://nfe.sefaz.pe.gov.br/nfe-service/services/NFeAutorizacao4",
    ret_autorizacao: "https://nfe.sefaz.pe.gov.br/nfe-service/services/NFeRetAutorizacao4",
    consulta_protocolo: "https://nfe.sefaz.pe.gov.br/nfe-service/services/NFeConsultaProtocolo4",
    status_servico: "https://nfe.sefaz.pe.gov.br/nfe-service/services/NFeStatusServico4",
    inutilizacao: "https://nfe.sefaz.pe.gov.br/nfe-service/services/NFeInutilizacao4",
    recepcao_evento: "https://nfe.sefaz.pe.gov.br/nfe-service/services/NFeRecepcaoEvento4",
    consulta_cadastro: Some(
        "https://nfe.sefaz.pe.gov.br/nfe-service/services/CadConsultaCadastro4",
    ),
};

const PE_HOM: UrlsNfe = UrlsNfe {
    autorizacao: "https://nfehomolog.sefaz.pe.gov.br/nfe-service/services/NFeAutorizacao4",
    ret_autorizacao: "https://nfehomolog.sefaz.pe.gov.br/nfe-service/services/NFeRetAutorizacao4",
    consulta_protocolo: "https://nfehomolog.sefaz.pe.gov.br/nfe-service/services/NFeConsultaProtocolo4",
    status_servico: "https://nfehomolog.sefaz.pe.gov.br/nfe-service/services/NFeStatusServico4",
    inutilizacao: "https://nfehomolog.sefaz.pe.gov.br/nfe-service/services/NFeInutilizacao4",
    recepcao_evento: "https://nfehomolog.sefaz.pe.gov.br/nfe-service/services/NFeRecepcaoEvento4",
    consulta_cadastro: Some(
        "https://nfehomolog.sefaz.pe.gov.br/nfe-service/services/CadConsultaCadastro4",
    ),
};

const PR_PROD: UrlsNfe = UrlsNfe {
    autorizacao: "https://nfe.sefa.pr.gov.br/nfe/NFeAutorizacao4",
    ret_autorizacao: "https://nfe.sefa.pr.gov.br/nfe/NFeRetAutorizacao4",
    consulta_protocolo: "https://nfe.sefa.pr.gov.br/nfe/NFeConsultaProtocolo4",
    status_servico: "https://nfe.sefa.pr.gov.br/nfe/NFeStatusServico4",
    inutilizacao: "https://nfe.sefa.pr.gov.br/nfe/NFeInutilizacao4",
    recepcao_evento: "https://nfe.sefa.pr.gov.br/nfe/NFeRecepcaoEvento4",
    consulta_cadastro: Some("https://nfe.sefa.pr.gov.br/nfe/CadConsultaCadastro4"),
};

const PR_HOM: UrlsNfe = UrlsNfe {
    autorizacao: "https://homologacao.nfe.sefa.pr.gov.br/nfe/NFeAutorizacao4",
    ret_autorizacao: "https://homologacao.nfe.sefa.pr.gov.br/nfe/NFeRetAutorizacao4",
    consulta_protocolo: "https://homologacao.nfe.sefa.pr.gov.br/nfe/NFeConsultaProtocolo4",
    status_servico: "https://homologacao.nfe.sefa.pr.gov.br/nfe/NFeStatusServico4",
    inutilizacao: "https://homologacao.nfe.sefa.pr.gov.br/nfe/NFeInutilizacao4",
    recepcao_evento: "https://homologacao.nfe.sefa.pr.gov.br/nfe/NFeRecepcaoEvento4",
    consulta_cadastro: Some("https://homologacao.nfe.sefa.pr.gov.br/nfe/CadConsultaCadastro4"),
};

const RS_PROD: UrlsNfe = UrlsNfe {
    autorizacao: "https://nfe.sefazrs.rs.gov.br/ws/NfeAutorizacao/NFeAutorizacao4.asmx",
    ret_autorizacao: "https://nfe.sefazrs.rs.gov.br/ws/NfeRetAutorizacao/NFeRetAutorizacao4.asmx",
    consulta_protocolo: "https://nfe.sefazrs.rs.gov.br/ws/NfeConsulta/NfeConsulta4.asmx",
    status_servico: "https://nfe.sefazrs.rs.gov.br/ws/NfeStatusServico/NfeStatusServico4.asmx",
    inutilizacao: "https://nfe.sefazrs.rs.gov.br/ws/nfeinutilizacao/nfeinutilizacao4.asmx",
    recepcao_evento: "https://nfe.sefazrs.rs.gov.br/ws/recepcaoevento/recepcaoevento4.asmx",
    consulta_cadastro: Some(CAD_SVRS),
};

const RS_HOM: UrlsNfe = UrlsNfe {
    autorizacao: "https://nfe-homologacao.sefazrs.rs.gov.br/ws/NfeAutorizacao/NFeAutorizacao4.asmx",
    ret_autorizacao: "https://nfe-homologacao.sefazrs.rs.gov.br/ws/NfeRetAutorizacao/NFeRetAutorizacao4.asmx",
    consulta_protocolo: "https://nfe-homologacao.sefazrs.rs.gov.br/ws/NfeConsulta/NfeConsulta4.asmx",
    status_servico: "https://nfe-homologacao.sefazrs.rs.gov.br/ws/NfeStatusServico/NfeStatusServico4.asmx",
    inutilizacao: "https://nfe-homologacao.sefazrs.rs.gov.br/ws/nfeinutilizacao/nfeinutilizacao4.asmx",
    recepcao_evento: "https://nfe-homologacao.sefazrs.rs.gov.br/ws/recepcaoevento/recepcaoevento4.asmx",
    consulta_cadastro: Some(CAD_SVRS),
};

const SP_PROD: UrlsNfe = UrlsNfe {
    autorizacao: "https://nfe.fazenda.sp.gov.br/ws/nfeautorizacao4.asmx",
    ret_autorizacao: "https://nfe.fazenda.sp.gov.br/ws/nferetautorizacao4.asmx",
    consulta_protocolo: "https://nfe.fazenda.sp.gov.br/ws/nfeconsultaprotocolo4.asmx",
    status_servico: "https://nfe.fazenda.sp.gov.br/ws/nfestatusservico4.asmx",
    inutilizacao: "https://nfe.fazenda.sp.gov.br/ws/nfeinutilizacao4.asmx",
    recepcao_evento: "https://nfe.fazenda.sp.gov.br/ws/nferecepcaoevento4.asmx",
    consulta_cadastro: Some("https://nfe.fazenda.sp.gov.br/ws/cadconsultacadastro4.asmx"),
};

const SP_HOM: UrlsNfe = UrlsNfe {
    autorizacao: "https://homologacao.nfe.fazenda.sp.gov.br/ws/nfeautorizacao4.asmx",
    ret_autorizacao: "https://homologacao.nfe.fazenda.sp.gov.br/ws/nferetautorizacao4.asmx",
    consulta_protocolo: "https://homologacao.nfe.fazenda.sp.gov.br/ws/nfeconsultaprotocolo4.asmx",
    status_servico: "https://homologacao.nfe.fazenda.sp.gov.br/ws/nfestatusservico4.asmx",
    inutilizacao: "https://homologacao.nfe.fazenda.sp.gov.br/ws/nfeinutilizacao4.asmx",
    recepcao_evento: "https://homologacao.nfe.fazenda.sp.gov.br/ws/nferecepcaoevento4.asmx",
    consulta_cadastro: Some(
        "https://homologacao.nfe.fazenda.sp.gov.br/ws/cadconsultacadastro4.asmx",
    ),
};

const SVAN_PROD: UrlsNfe = UrlsNfe {
    autorizacao: "https://www.sefazvirtual.fazenda.gov.br/NFeAutorizacao4/NFeAutorizacao4.asmx",
    ret_autorizacao: "https://www.sefazvirtual.fazenda.gov.br/NFeRetAutorizacao4/NFeRetAutorizacao4.asmx",
    consulta_protocolo: "https://www.sefazvirtual.fazenda.gov.br/NFeConsultaProtocolo4/NFeConsultaProtocolo4.asmx",
    status_servico: "https://www.sefazvirtual.fazenda.gov.br/NFeStatusServico4/NFeStatusServico4.asmx",
    inutilizacao: "https://www.sefazvirtual.fazenda.gov.br/NFeInutilizacao4/NFeInutilizacao4.asmx",
    recepcao_evento: "https://www.sefazvirtual.fazenda.gov.br/NFeRecepcaoEvento4/NFeRecepcaoEvento4.asmx",
    consulta_cadastro: None,
};

const SVAN_HOM: UrlsNfe = UrlsNfe {
    autorizacao: "https://hom.sefazvirtual.fazenda.gov.br/NFeAutorizacao4/NFeAutorizacao4.asmx",
    ret_autorizacao: "https://hom.sefazvirtual.fazenda.gov.br/NFeRetAutorizacao4/NFeRetAutorizacao4.asmx",
    consulta_protocolo: "https://hom.sefazvirtual.fazenda.gov.br/NFeConsultaProtocolo4/NFeConsultaProtocolo4.asmx",
    status_servico: "https://hom.sefazvirtual.fazenda.gov.br/NFeStatusServico4/NFeStatusServico4.asmx",
    inutilizacao: "https://hom.sefazvirtual.fazenda.gov.br/NFeInutilizacao4/NFeInutilizacao4.asmx",
    recepcao_evento: "https://hom.sefazvirtual.fazenda.gov.br/NFeRecepcaoEvento4/NFeRecepcaoEvento4.asmx",
    consulta_cadastro: None,
};

const SVRS_PROD: UrlsNfe = UrlsNfe {
    autorizacao: "https://nfe.svrs.rs.gov.br/ws/NfeAutorizacao/NFeAutorizacao4.asmx",
    ret_autorizacao: "https://nfe.svrs.rs.gov.br/ws/NfeRetAutorizacao/NFeRetAutorizacao4.asmx",
    consulta_protocolo: "https://nfe.svrs.rs.gov.br/ws/NfeConsulta/NfeConsulta4.asmx",
    status_servico: "https://nfe.svrs.rs.gov.br/ws/NfeStatusServico/NfeStatusServico4.asmx",
    inutilizacao: "https://nfe.svrs.rs.gov.br/ws/nfeinutilizacao/nfeinutilizacao4.asmx",
    recepcao_evento: "https://nfe.svrs.rs.gov.br/ws/recepcaoevento/recepcaoevento4.asmx",
    consulta_cadastro: Some(CAD_SVRS),
};

const SVRS_HOM: UrlsNfe = UrlsNfe {
    autorizacao: "https://nfe-homologacao.svrs.rs.gov.br/ws/NfeAutorizacao/NFeAutorizacao4.asmx",
    ret_autorizacao: "https://nfe-homologacao.svrs.rs.gov.br/ws/NfeRetAutorizacao/NFeRetAutorizacao4.asmx",
    consulta_protocolo: "https://nfe-homologacao.svrs.rs.gov.br/ws/NfeConsulta/NfeConsulta4.asmx",
    status_servico: "https://nfe-homologacao.svrs.rs.gov.br/ws/NfeStatusServico/NfeStatusServico4.asmx",
    inutilizacao: "https://nfe-homologacao.svrs.rs.gov.br/ws/nfeinutilizacao/nfeinutilizacao4.asmx",
    recepcao_evento: "https://nfe-homologacao.svrs.rs.gov.br/ws/recepcaoevento/recepcaoevento4.asmx",
    consulta_cadastro: Some(CAD_SVRS),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todas_as_ufs_tem_autorizador_principal() {
        for uf in Uf::TODAS {
            let autorizador = autorizador_principal(*uf);
            assert_ne!(autorizador, Autorizador::An, "{uf}");
        }
        assert_eq!(autorizador_principal(Uf::Maranhao), Autorizador::Svan);
        assert_eq!(autorizador_principal(Uf::Ceara), Autorizador::Svrs);
        assert_eq!(autorizador_principal(Uf::SaoPaulo), Autorizador::Sp);
    }

    #[test]
    fn resolve_servicos_principais_nos_dois_ambientes() {
        for tp_amb in [TpAmb::Producao, TpAmb::Homologacao] {
            for uf in Uf::TODAS {
                for servico in [
                    ServicoNfe::Autorizacao,
                    ServicoNfe::RetAutorizacao,
                    ServicoNfe::ConsultaProtocolo,
                    ServicoNfe::StatusServico,
                    ServicoNfe::Inutilizacao,
                    ServicoNfe::RecepcaoEvento,
                ] {
                    let endpoint = resolver_nfe(&ConsultaNfe::new(*uf, servico, tp_amb))
                        .unwrap_or_else(|e| panic!("{uf} {servico:?} {tp_amb:?}: {e}"));
                    assert!(endpoint.url.starts_with("https://"), "{endpoint:?}");
                    assert!(!endpoint.url.contains(' '));
                }
            }
        }
    }

    #[test]
    fn producao_e_homologacao_nao_se_misturam() {
        let prod = resolver_nfe(&ConsultaNfe::new(
            Uf::SaoPaulo,
            ServicoNfe::Autorizacao,
            TpAmb::Producao,
        ))
        .expect("produção");
        let hom = resolver_nfe(&ConsultaNfe::new(
            Uf::SaoPaulo,
            ServicoNfe::Autorizacao,
            TpAmb::Homologacao,
        ))
        .expect("homologação");
        assert!(prod.url.contains("nfe.fazenda.sp.gov.br/ws/"));
        assert!(hom.url.contains("homologacao.nfe.fazenda.sp.gov.br/ws/"));
        assert_ne!(prod.url, hom.url);
    }

    #[test]
    fn consulta_cadastro_respeita_as_ufs_da_svrs() {
        let cadastro = |uf| {
            resolver_nfe(&ConsultaNfe::new(
                uf,
                ServicoNfe::ConsultaCadastro,
                TpAmb::Producao,
            ))
        };
        assert!(cadastro(Uf::SaoPaulo).is_ok());
        assert!(cadastro(Uf::Bahia).is_ok());
        assert!(cadastro(Uf::Acre).is_ok());
        assert!(cadastro(Uf::Ceara).is_err());
        assert!(cadastro(Uf::Amazonas).is_err());
        assert!(cadastro(Uf::MinasGerais).is_err());
    }

    #[test]
    fn contingencia_so_aceita_uf_atendida() {
        let svc_an = ConsultaNfe::new(Uf::SaoPaulo, ServicoNfe::Autorizacao, TpAmb::Producao)
            .em_contingencia(Contingencia::SvcAn);
        assert!(resolver_nfe(&svc_an).is_ok());

        let svc_rs_com_sp =
            ConsultaNfe::new(Uf::SaoPaulo, ServicoNfe::Autorizacao, TpAmb::Producao)
                .em_contingencia(Contingencia::SvcRs);
        assert!(resolver_nfe(&svc_rs_com_sp).is_err());

        let svc_rs_com_ba = ConsultaNfe::new(Uf::Bahia, ServicoNfe::Autorizacao, TpAmb::Producao)
            .em_contingencia(Contingencia::SvcRs);
        assert!(resolver_nfe(&svc_rs_com_ba).is_ok());
    }

    #[test]
    fn distribuicao_usa_o_ambiente_nacional() {
        let prod = resolver_nfe(&ConsultaNfe::new(
            Uf::SaoPaulo,
            ServicoNfe::DistribuicaoDfe,
            TpAmb::Producao,
        ))
        .expect("produção");
        assert_eq!(prod.autorizador, Some(Autorizador::An));
        assert!(prod.url.contains("www1.nfe.fazenda.gov.br"));
        assert_eq!(prod.versao, "1.00");

        let hom = resolver_nfe(&ConsultaNfe::new(
            Uf::SaoPaulo,
            ServicoNfe::DistribuicaoDfe,
            TpAmb::Homologacao,
        ))
        .expect("homologação");
        assert!(hom.url.contains("hom1.nfe.fazenda.gov.br"));
    }
}
