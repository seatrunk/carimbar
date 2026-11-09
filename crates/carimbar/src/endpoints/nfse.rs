use crate::types::{Ambiente, TpAmb};

use crate::endpoints::erro::ErroEndpoints;
use crate::endpoints::{Endpoint, Fonte, VERIFICADO_EM};

/// Serviço da NFS-e Nacional (Sefin Nacional ou ADN).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ServicoNfse {
    /// Emissão da NFS-e a partir da DPS assinada (`POST /nfse`).
    EmitirDps,
    /// Consulta da NFS-e pela chave de acesso (`GET /nfse/{chave}`).
    ConsultarNfse,
    /// Registro de evento (`POST /nfse/{chave}/eventos`).
    RegistrarEvento,
    /// Consulta de um evento específico.
    ConsultarEvento,
    /// Consulta da DPS pelo identificador (`GET /dps/{idDps}`).
    ConsultarDps,
    /// Distribuição de DF-e do ADN (`GET /contribuintes/DFe/{nsu}`).
    DistribuicaoDfe,
    /// Eventos de uma NFS-e no ADN (`GET /contribuintes/NFSe/{chave}/Eventos`).
    EventosPorChave,
    /// DANFSe em PDF (`GET /danfse/{chave}`).
    Danfse,
}

impl ServicoNfse {
    /// Nome do serviço para diagnóstico.
    pub fn nome(self) -> &'static str {
        match self {
            Self::EmitirDps => "NFS-e:emitir",
            Self::ConsultarNfse => "NFS-e:consultar",
            Self::RegistrarEvento => "NFS-e:evento",
            Self::ConsultarEvento => "NFS-e:evento:consultar",
            Self::ConsultarDps => "NFS-e:consultar-dps",
            Self::DistribuicaoDfe => "ADN:distribuicao",
            Self::EventosPorChave => "ADN:eventos",
            Self::Danfse => "ADN:danfse",
        }
    }

    /// Caminho relativo ao host do serviço, com placeholders entre chaves.
    pub fn caminho(self) -> &'static str {
        match self {
            Self::EmitirDps => "/nfse",
            Self::ConsultarNfse => "/nfse/{chave}",
            Self::RegistrarEvento => "/nfse/{chave}/eventos",
            Self::ConsultarEvento => "/nfse/{chave}/eventos/{tipoEvento}/{numeroSequencial}",
            Self::ConsultarDps => "/dps/{idDps}",
            Self::DistribuicaoDfe => "/contribuintes/DFe/{nsu}",
            Self::EventosPorChave => "/contribuintes/NFSe/{chave}/Eventos",
            Self::Danfse => "/danfse/{chave}",
        }
    }

    /// Versão do contrato publicado para o serviço.
    pub fn versao(self) -> &'static str {
        match self {
            Self::DistribuicaoDfe | Self::EventosPorChave | Self::Danfse => "1.00",
            _ => "1.01",
        }
    }

    fn usa_adn(self) -> bool {
        matches!(
            self,
            Self::DistribuicaoDfe | Self::EventosPorChave | Self::Danfse
        )
    }
}

/// Consulta de URL da NFS-e Nacional.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsultaNfse {
    /// Serviço desejado.
    pub servico: ServicoNfse,
    /// Ambiente do XML e do endpoint.
    pub tp_amb: TpAmb,
}

impl ConsultaNfse {
    /// Consulta no ambiente informado.
    pub fn new(servico: ServicoNfse, tp_amb: TpAmb) -> Self {
        Self { servico, tp_amb }
    }

    /// Consulta com o `tpAmb` retirado do [`Ambiente`] já validado pelo chamador.
    pub fn para(ambiente: &Ambiente, servico: ServicoNfse) -> Self {
        Self::new(servico, ambiente.tp_amb())
    }
}

/// Resolve a URL de um serviço da NFS-e Nacional.
pub fn resolver_nfse(consulta: &ConsultaNfse) -> Result<Endpoint, ErroEndpoints> {
    let base = if consulta.servico.usa_adn() {
        adn(consulta.tp_amb)
    } else {
        sefin(consulta.tp_amb)
    };
    Ok(Endpoint {
        url: format!("{base}{}", consulta.servico.caminho()),
        versao: consulta.servico.versao(),
        autorizador: None,
        fonte: Fonte::PortalNfse,
        verificado_em: VERIFICADO_EM,
    })
}

fn sefin(tp_amb: TpAmb) -> &'static str {
    match tp_amb {
        TpAmb::Producao => "https://sefin.nfse.gov.br/SefinNacional",
        TpAmb::Homologacao => "https://sefin.producaorestrita.nfse.gov.br/SefinNacional",
    }
}

fn adn(tp_amb: TpAmb) -> &'static str {
    match tp_amb {
        TpAmb::Producao => "https://adn.nfse.gov.br",
        TpAmb::Homologacao => "https://adn.producaorestrita.nfse.gov.br",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sefin_e_adn_separados_por_ambiente() {
        let prod = resolver_nfse(&ConsultaNfse::new(ServicoNfse::EmitirDps, TpAmb::Producao))
            .expect("produção");
        assert_eq!(prod.url, "https://sefin.nfse.gov.br/SefinNacional/nfse");
        assert_eq!(prod.versao, "1.01");
        assert!(prod.autorizador.is_none());

        let hom = resolver_nfse(&ConsultaNfse::new(
            ServicoNfse::EmitirDps,
            TpAmb::Homologacao,
        ))
        .expect("homologação");
        assert_eq!(
            hom.url,
            "https://sefin.producaorestrita.nfse.gov.br/SefinNacional/nfse"
        );

        let adn = resolver_nfse(&ConsultaNfse::new(
            ServicoNfse::DistribuicaoDfe,
            TpAmb::Producao,
        ))
        .expect("ADN");
        assert_eq!(adn.url, "https://adn.nfse.gov.br/contribuintes/DFe/{nsu}");
        assert_eq!(adn.versao, "1.00");
        assert_eq!(adn.fonte, Fonte::PortalNfse);
    }

    #[test]
    fn caminhos_publicados() {
        let url = |servico| {
            resolver_nfse(&ConsultaNfse::new(servico, TpAmb::Homologacao))
                .expect("URL")
                .url
        };
        assert!(url(ServicoNfse::ConsultarNfse).ends_with("/nfse/{chave}"));
        assert!(url(ServicoNfse::RegistrarEvento).ends_with("/nfse/{chave}/eventos"));
        assert!(
            url(ServicoNfse::ConsultarEvento)
                .ends_with("/nfse/{chave}/eventos/{tipoEvento}/{numeroSequencial}")
        );
        assert!(url(ServicoNfse::ConsultarDps).ends_with("/dps/{idDps}"));
        assert!(url(ServicoNfse::EventosPorChave).ends_with("/NFSe/{chave}/Eventos"));
        assert!(url(ServicoNfse::Danfse).ends_with("/danfse/{chave}"));
    }
}
