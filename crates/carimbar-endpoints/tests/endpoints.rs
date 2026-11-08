//! Testes de resolução da tabela de endpoints. Nenhum teste abre conexão.

use carimbar_endpoints::{
    Autorizador, ConsultaNfe, ConsultaNfse, Contingencia, ErroEndpoints, ServicoNfe, ServicoNfse,
    Uf, VERIFICADO_EM, resolver_nfe, resolver_nfse,
};
use carimbar_types::TpAmb;

#[test]
fn toda_uf_resolve_autorizacao_nos_dois_ambientes() {
    for uf in Uf::TODAS {
        for tp_amb in [TpAmb::Producao, TpAmb::Homologacao] {
            let endpoint = resolver_nfe(&ConsultaNfe::new(*uf, ServicoNfe::Autorizacao, tp_amb))
                .unwrap_or_else(|e| panic!("{uf} {tp_amb:?}: {e}"));
            assert_eq!(endpoint.verificado_em, VERIFICADO_EM);
            assert_eq!(endpoint.versao, "4.00");
            assert!(endpoint.url.starts_with("https://"), "{endpoint:?}");
            assert!(!endpoint.url.contains("{"), "{endpoint:?}");
            assert!(endpoint.fonte.url().contains("nfe.fazenda.gov.br"));
        }
    }
}

#[test]
fn usa_o_tpamb_do_ambiente_validado() {
    use carimbar_types::{Ambiente, ConfirmacaoProducao};

    let ambiente = Ambiente::Homologacao;
    let endpoint = resolver_nfe(&ConsultaNfe::para(
        &ambiente,
        Uf::SaoPaulo,
        ServicoNfe::Autorizacao,
    ))
    .expect("homologação");
    assert!(endpoint.url.contains("homologacao.nfe.fazenda.sp.gov.br"));

    let ambiente = Ambiente::producao(ConfirmacaoProducao::new(
        carimbar_types::Cnpj::parse("00000001000136").expect("CNPJ sintético"),
    ));
    let endpoint = resolver_nfe(&ConsultaNfe::para(
        &ambiente,
        Uf::SaoPaulo,
        ServicoNfe::Autorizacao,
    ))
    .expect("produção");
    assert!(endpoint.url.contains("nfe.fazenda.sp.gov.br"));
}

#[test]
fn contingencia_nao_oferece_consulta_cadastro() {
    let consulta = ConsultaNfe::new(Uf::SaoPaulo, ServicoNfe::ConsultaCadastro, TpAmb::Producao)
        .em_contingencia(Contingencia::SvcAn);
    assert!(matches!(
        resolver_nfe(&consulta),
        Err(ErroEndpoints::NaoDisponivel {
            servico: "CadConsultaCadastro",
            ..
        })
    ));
}

#[test]
fn producao_nunca_reaproveita_host_de_homologacao() {
    for uf in Uf::TODAS {
        let prod = resolver_nfe(&ConsultaNfe::new(
            *uf,
            ServicoNfe::StatusServico,
            TpAmb::Producao,
        ))
        .expect("produção");
        let hom = resolver_nfe(&ConsultaNfe::new(
            *uf,
            ServicoNfe::StatusServico,
            TpAmb::Homologacao,
        ))
        .expect("homologação");
        assert_ne!(prod.url, hom.url, "{uf}");
        for host_de_homolog in ["hom", "homolog", "hnfe", "nfehomolog"] {
            assert!(
                !prod.url.contains(host_de_homolog),
                "{uf}: produção aponta para homologação: {}",
                prod.url
            );
        }
    }
}

#[test]
fn autorizadores_principais_conferem() {
    let esperado = [
        (Uf::Amazonas, Autorizador::Am),
        (Uf::Bahia, Autorizador::Ba),
        (Uf::Goias, Autorizador::Go),
        (Uf::MinasGerais, Autorizador::Mg),
        (Uf::MatoGrossoDoSul, Autorizador::Ms),
        (Uf::MatoGrosso, Autorizador::Mt),
        (Uf::Pernambuco, Autorizador::Pe),
        (Uf::Parana, Autorizador::Pr),
        (Uf::RioGrandeDoSul, Autorizador::Rs),
        (Uf::SaoPaulo, Autorizador::Sp),
        (Uf::Maranhao, Autorizador::Svan),
        (Uf::Acre, Autorizador::Svrs),
        (Uf::Ceara, Autorizador::Svrs),
        (Uf::Tocantins, Autorizador::Svrs),
    ];
    for (uf, autorizador) in esperado {
        assert_eq!(
            carimbar_endpoints::autorizador_principal(uf),
            autorizador,
            "{uf}"
        );
    }
}

#[test]
fn consulta_cadastro_combina_com_o_portal() {
    for uf in [
        Uf::Bahia,
        Uf::Goias,
        Uf::MatoGrossoDoSul,
        Uf::MatoGrosso,
        Uf::Pernambuco,
        Uf::Parana,
        Uf::RioGrandeDoSul,
        Uf::SaoPaulo,
        Uf::Acre,
        Uf::EspiritoSanto,
        Uf::RioGrandeDoNorte,
        Uf::Paraiba,
        Uf::SantaCatarina,
    ] {
        assert!(
            resolver_nfe(&ConsultaNfe::new(
                uf,
                ServicoNfe::ConsultaCadastro,
                TpAmb::Producao
            ))
            .is_ok(),
            "{uf}"
        );
    }
    for uf in [
        Uf::Amazonas,
        Uf::MinasGerais,
        Uf::Maranhao,
        Uf::Ceara,
        Uf::Alagoas,
    ] {
        assert_eq!(
            resolver_nfe(&ConsultaNfe::new(
                uf,
                ServicoNfe::ConsultaCadastro,
                TpAmb::Producao
            )),
            Err(ErroEndpoints::NaoDisponivel {
                servico: "CadConsultaCadastro",
                contexto: format!(
                    "{} via {}",
                    uf,
                    carimbar_endpoints::autorizador_principal(uf).sigla()
                ),
                ambiente: "produção",
            }),
            "{uf}"
        );
    }
}

#[test]
fn contingencia_respeita_a_uf() {
    let sp_an = ConsultaNfe::new(Uf::SaoPaulo, ServicoNfe::Autorizacao, TpAmb::Producao)
        .em_contingencia(Contingencia::SvcAn);
    assert_eq!(
        resolver_nfe(&sp_an).expect("SVC-AN").autorizador,
        Some(Autorizador::SvcAn)
    );

    let ba_rs = ConsultaNfe::new(Uf::Bahia, ServicoNfe::Autorizacao, TpAmb::Producao)
        .em_contingencia(Contingencia::SvcRs);
    assert_eq!(
        resolver_nfe(&ba_rs).expect("SVC-RS").autorizador,
        Some(Autorizador::SvcRs)
    );

    let sp_rs = ConsultaNfe::new(Uf::SaoPaulo, ServicoNfe::Autorizacao, TpAmb::Producao)
        .em_contingencia(Contingencia::SvcRs);
    assert!(matches!(
        resolver_nfe(&sp_rs),
        Err(ErroEndpoints::NaoDisponivel { .. })
    ));
}

#[test]
fn distribuicao_e_sempre_ambiente_nacional() {
    for uf in [Uf::SaoPaulo, Uf::Amazonas, Uf::Maranhao] {
        let endpoint = resolver_nfe(&ConsultaNfe::new(
            uf,
            ServicoNfe::DistribuicaoDfe,
            TpAmb::Producao,
        ))
        .expect("distribuição");
        assert_eq!(endpoint.autorizador, Some(Autorizador::An));
        assert_eq!(
            endpoint.url,
            "https://www1.nfe.fazenda.gov.br/NFeDistribuicaoDFe/NFeDistribuicaoDFe.asmx"
        );
    }
}

#[test]
fn nfse_nao_mistura_sefin_com_adn() {
    for tp_amb in [TpAmb::Producao, TpAmb::Homologacao] {
        let sefin =
            resolver_nfse(&ConsultaNfse::new(ServicoNfse::EmitirDps, tp_amb)).expect("Sefin");
        assert!(sefin.url.contains("sefin."), "{sefin:?}");
        assert!(!sefin.url.contains("adn."));

        let adn = resolver_nfse(&ConsultaNfse::new(ServicoNfse::Danfse, tp_amb)).expect("ADN");
        assert!(adn.url.contains("adn."), "{adn:?}");
        assert!(!adn.url.contains("sefin."));
    }
}

#[test]
fn preenche_urls_com_chave_e_nsu() {
    let chave = "35503081234567890123456789012345678901234567";
    let consulta = resolver_nfse(&ConsultaNfse::new(
        ServicoNfse::ConsultarNfse,
        TpAmb::Homologacao,
    ))
    .expect("consulta");
    assert_eq!(
        consulta
            .preencher(&[("chave", chave)])
            .expect("URL preenchida"),
        format!("https://sefin.producaorestrita.nfse.gov.br/SefinNacional/nfse/{chave}")
    );

    let distribuicao = resolver_nfse(&ConsultaNfse::new(
        ServicoNfse::DistribuicaoDfe,
        TpAmb::Producao,
    ))
    .expect("distribuição");
    assert_eq!(
        distribuicao
            .preencher(&[("nsu", "000000000000001")])
            .expect("NSU"),
        "https://adn.nfse.gov.br/contribuintes/DFe/000000000000001"
    );
}
