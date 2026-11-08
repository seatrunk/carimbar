//! Leitura, escrita, limites e rejeição de construções inseguras.

use carimbar_xml::{Documento, Elemento, ErroXml, Limites, No, Nome};

fn parse(xml: &str) -> Documento {
    Documento::parse(xml.as_bytes()).expect("XML válido")
}

#[test]
fn le_e_escreve_round_trip() {
    let entrada = r#"<?xml version="1.0" encoding="UTF-8"?><DPS xmlns="http://www.sped.fazenda.gov.br/nfse" versao="1.01"><infDPS Id="DPS1"><tpAmb>2</tpAmb><serie>1</serie></infDPS></DPS>"#;
    let documento = parse(entrada);
    assert_eq!(documento.raiz().nome().local(), "DPS");
    assert_eq!(documento.raiz().atributo("versao"), Some("1.01"));
    assert_eq!(
        documento
            .raiz()
            .primeiro_filho("infDPS")
            .and_then(|inf| inf.primeiro_filho("tpAmb"))
            .map(Elemento::texto),
        Some("2".to_owned())
    );

    let escrito = documento.to_xml().expect("serializa");
    let relido = parse(&escrito);
    assert_eq!(documento, relido);
    assert_eq!(escrito, relido.to_xml().expect("serializa de novo"));
}

#[test]
fn escapa_texto_e_atributos() {
    let raiz = Elemento::new(Nome::new("a"))
        .com_atributo_texto("v", "x\"y\tz\nw\r")
        .com_texto("1 & 2 < 3 > 4\r");
    let documento = Documento::new(raiz);
    let xml = documento.to_xml().expect("serializa");
    assert_eq!(
        xml,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><a v=\"x&quot;y&#x9;z&#xA;w&#xD;\">1 &amp; 2 &lt; 3 &gt; 4&#xD;</a>"
    );

    let relido = parse(&xml);
    assert_eq!(relido.raiz().atributo("v"), Some("x\"y\tz\nw\r"));
    assert_eq!(relido.raiz().texto(), "1 & 2 < 3 > 4\r");
}

#[test]
fn preserva_comentarios_e_instrucoes() {
    let entrada = "<?xml version=\"1.0\"?><!--antes--><r><?p d?><x/></r><!--depois-->";
    let documento = parse(entrada);
    assert_eq!(documento.antes_da_raiz().len(), 1);
    assert_eq!(documento.depois_da_raiz().len(), 1);
    assert_eq!(
        documento.to_xml().expect("serializa"),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><!--antes--><r><?p d?><x/></r><!--depois-->"
    );
}

#[test]
fn cdata_vira_texto() {
    let documento = parse("<a><![CDATA[1 & < 2]]></a>");
    assert_eq!(documento.raiz().texto(), "1 & < 2");
    assert_eq!(
        documento.to_xml().expect("serializa"),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><a>1 &amp; &lt; 2</a>"
    );
}

#[test]
fn rejeita_dtd() {
    let erro = Documento::parse(b"<!DOCTYPE a><a/>").expect_err("DTD rejeitada");
    assert!(matches!(erro, ErroXml::DtdProibida { .. }), "{erro}");
}

#[test]
fn rejeita_entidade_desconhecida() {
    assert!(Documento::parse(b"<a>&foo;</a>").is_err());
}

#[test]
fn rejeita_codificacao_diferente_de_utf8() {
    let erro = Documento::parse(br#"<?xml version="1.0" encoding="ISO-8859-1"?><a/>"#)
        .expect_err("codificação rejeitada");
    assert!(matches!(erro, ErroXml::Codificacao(_)), "{erro}");
}

#[test]
fn rejeita_declaracao_fora_do_inicio() {
    assert!(Documento::parse(b"<a><?xml version=\"1.0\"?></a>").is_err());
}

#[test]
fn rejeita_xml_1_1() {
    let erro = Documento::parse(br#"<?xml version="1.1"?><a/>"#).expect_err("versão rejeitada");
    assert!(matches!(erro, ErroXml::Codificacao(_)), "{erro}");
}

#[test]
fn limita_bytes() {
    let limites = Limites {
        bytes_max: 3,
        ..Limites::padrao()
    };
    let erro = Documento::parse_com_limites(b"<a/>", &limites).expect_err("limite de bytes");
    assert!(matches!(
        erro,
        ErroXml::Limite {
            limite: "bytes",
            ..
        }
    ));
}

#[test]
fn limita_profundidade() {
    let limites = Limites {
        profundidade_max: 2,
        ..Limites::padrao()
    };
    assert!(Documento::parse_com_limites(b"<a><b/></a>", &limites).is_ok());
    let erro = Documento::parse_com_limites(b"<a><b><c/></b></a>", &limites)
        .expect_err("limite de profundidade");
    assert!(matches!(
        erro,
        ErroXml::Limite {
            limite: "profundidade",
            ..
        }
    ));
}

#[test]
fn limita_elementos() {
    let limites = Limites {
        elementos_max: 2,
        ..Limites::padrao()
    };
    let erro = Documento::parse_com_limites(b"<a><b/><c/></a>", &limites)
        .expect_err("limite de elementos");
    assert!(matches!(
        erro,
        ErroXml::Limite {
            limite: "elementos",
            ..
        }
    ));
}

#[test]
fn limita_atributos() {
    let limites = Limites {
        atributos_por_elemento_max: 1,
        ..Limites::padrao()
    };
    let erro = Documento::parse_com_limites(b"<a x=\"1\" y=\"2\"/>", &limites)
        .expect_err("limite de atributos");
    assert!(matches!(
        erro,
        ErroXml::Limite {
            limite: "atributos por elemento",
            ..
        }
    ));
}

#[test]
fn rejeita_atributo_repetido() {
    assert!(Documento::parse(b"<a x=\"1\" x=\"2\"/>").is_err());
}

#[test]
fn rejeita_prefixo_nao_declarado() {
    let erro = Documento::parse(b"<p:a/>").expect_err("prefixo não declarado");
    assert!(matches!(erro, ErroXml::Namespace(_)), "{erro}");
}

#[test]
fn rejeita_prefixo_com_uri_vazia() {
    let erro = Documento::parse(br#"<a xmlns:p=""/>"#).expect_err("URI vazia");
    assert!(matches!(erro, ErroXml::Namespace(_)), "{erro}");
}

#[test]
fn rejeita_dois_elementos_raiz() {
    assert!(Documento::parse(b"<a/><b/>").is_err());
}

#[test]
fn rejeita_texto_fora_da_raiz() {
    assert!(Documento::parse(b"<a/>texto").is_err());
}

#[test]
fn rejeita_comentario_invalido_construido() {
    let raiz = Elemento::new(Nome::new("a")).com_filho(No::Comentario("a--b".to_owned()));
    let documento = Documento::new(raiz);
    assert!(documento.to_xml().is_err());
}

#[test]
fn preserva_namespaces_e_prefixos() {
    let entrada = r#"<r xmlns="urn:r" xmlns:p="urn:p"><p:x p:a="1"/></r>"#;
    let documento = parse(entrada);
    let filho = documento.raiz().primeiro_filho("x").expect("filho x");
    assert_eq!(filho.nome().prefixo(), Some("p"));
    assert!(documento.canonicalizar().is_ok());
}
