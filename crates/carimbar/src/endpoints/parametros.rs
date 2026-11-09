//! Endpoints da API de parâmetros municipais do ADN.

use crate::types::{Ambiente, TpAmb};

use crate::endpoints::erro::ErroEndpoints;
use crate::endpoints::{Endpoint, Fonte, VERIFICADO_EM};

/// Consulta da API de parâmetros municipais.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ServicoParametros {
    /// Convênio do município (`/{codigoMunicipio}/convenio`).
    Convenio,
    /// Alíquota por serviço e competência.
    Aliquota,
    /// Histórico de alíquotas por serviço.
    HistoricoAliquotas,
    /// Benefício municipal por número e competência.
    Beneficio,
    /// Regimes especiais por serviço e competência.
    RegimesEspeciais,
    /// Retenções por competência.
    Retencoes,
}

impl ServicoParametros {
    /// Nome para diagnóstico.
    pub fn nome(self) -> &'static str {
        match self {
            Self::Convenio => "parametros:convenio",
            Self::Aliquota => "parametros:aliquota",
            Self::HistoricoAliquotas => "parametros:historico-aliquotas",
            Self::Beneficio => "parametros:beneficio",
            Self::RegimesEspeciais => "parametros:regimes-especiais",
            Self::Retencoes => "parametros:retencoes",
        }
    }

    /// Caminho relativo ao host de parametrização, com placeholders.
    pub fn caminho(self) -> &'static str {
        match self {
            Self::Convenio => "/{codigoMunicipio}/convenio",
            Self::Aliquota => "/{codigoMunicipio}/{codigoServico}/{competencia}/aliquota",
            Self::HistoricoAliquotas => "/{codigoMunicipio}/{codigoServico}/historicoaliquotas",
            Self::Beneficio => "/{codigoMunicipio}/{numeroBeneficio}/{competencia}/beneficio",
            Self::RegimesEspeciais => {
                "/{codigoMunicipio}/{codigoServico}/{competencia}/regimes_especiais"
            }
            Self::Retencoes => "/{codigoMunicipio}/{competencia}/retencoes",
        }
    }

    /// Versão do contrato publicado.
    pub fn versao(self) -> &'static str {
        "1.00"
    }
}

/// Consulta de URL da parametrização municipal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsultaParametros {
    /// Serviço desejado.
    pub servico: ServicoParametros,
    /// Ambiente do endpoint.
    pub tp_amb: TpAmb,
}

impl ConsultaParametros {
    /// Consulta no ambiente informado.
    pub fn new(servico: ServicoParametros, tp_amb: TpAmb) -> Self {
        Self { servico, tp_amb }
    }

    /// Consulta a partir do [`Ambiente`].
    pub fn para(ambiente: &Ambiente, servico: ServicoParametros) -> Self {
        Self::new(servico, ambiente.tp_amb())
    }
}

/// Resolve a URL de um serviço de parametrização municipal.
pub fn resolver_parametros(consulta: &ConsultaParametros) -> Result<Endpoint, ErroEndpoints> {
    Ok(Endpoint {
        url: format!("{}{}", base(consulta.tp_amb), consulta.servico.caminho()),
        versao: consulta.servico.versao(),
        autorizador: None,
        fonte: Fonte::PortalNfse,
        verificado_em: VERIFICADO_EM,
    })
}

fn base(tp_amb: TpAmb) -> &'static str {
    match tp_amb {
        TpAmb::Producao => "https://adn.nfse.gov.br/parametrizacao",
        TpAmb::Homologacao => "https://adn.producaorestrita.nfse.gov.br/parametrizacao",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caminhos_e_ambientes() {
        let producao = resolver_parametros(&ConsultaParametros::new(
            ServicoParametros::Convenio,
            TpAmb::Producao,
        ))
        .expect("produção");
        assert_eq!(
            producao.url,
            "https://adn.nfse.gov.br/parametrizacao/{codigoMunicipio}/convenio"
        );
        let homologacao = resolver_parametros(&ConsultaParametros::new(
            ServicoParametros::Aliquota,
            TpAmb::Homologacao,
        ))
        .expect("homologação");
        assert_eq!(
            homologacao.url,
            "https://adn.producaorestrita.nfse.gov.br/parametrizacao/{codigoMunicipio}/{codigoServico}/{competencia}/aliquota"
        );
        assert_eq!(
            resolver_parametros(&ConsultaParametros::new(
                ServicoParametros::Retencoes,
                TpAmb::Producao
            ))
            .expect("retenções")
            .placeholders(),
            vec!["codigoMunicipio", "competencia"]
        );
    }
}
