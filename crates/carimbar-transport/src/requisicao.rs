use crate::erro::ErroTransporte;

/// Método HTTP usado na requisição.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Metodo {
    /// `GET`, usado em consultas.
    Get,
    /// `POST`, usado em SOAP, JSON e consultas com corpo.
    Post,
    /// `HEAD`, usado para verificar existência sem corpo.
    Head,
}

impl Metodo {
    /// Texto do método na linha de requisição.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Head => "HEAD",
        }
    }
}

/// Requisição HTTP montada localmente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requisicao {
    /// Método.
    pub metodo: Metodo,
    /// URL absoluta `https://`.
    pub url: String,
    /// `Content-Type`, quando há corpo.
    pub tipo_conteudo: Option<String>,
    /// `Accept`.
    pub aceitar: Option<String>,
    /// Corpo já serializado.
    pub corpo: Vec<u8>,
}

impl Requisicao {
    /// `GET` sem corpo.
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            metodo: Metodo::Get,
            url: url.into(),
            tipo_conteudo: None,
            aceitar: None,
            corpo: Vec::new(),
        }
    }

    /// `GET` que espera JSON, usado nas consultas da Sefin/ADN.
    pub fn get_json(url: impl Into<String>) -> Self {
        Self {
            metodo: Metodo::Get,
            url: url.into(),
            tipo_conteudo: None,
            aceitar: Some("application/json".to_owned()),
            corpo: Vec::new(),
        }
    }

    /// `HEAD` sem corpo.
    pub fn head(url: impl Into<String>) -> Self {
        Self {
            metodo: Metodo::Head,
            url: url.into(),
            tipo_conteudo: None,
            aceitar: None,
            corpo: Vec::new(),
        }
    }

    /// `POST` com corpo JSON.
    pub fn post_json(url: impl Into<String>, json: impl Into<Vec<u8>>) -> Self {
        Self {
            metodo: Metodo::Post,
            url: url.into(),
            tipo_conteudo: Some("application/json; charset=utf-8".to_owned()),
            aceitar: Some("application/json".to_owned()),
            corpo: json.into(),
        }
    }

    /// `POST` SOAP 1.2, envolvendo `corpo_xml` no `Envelope` padrão.
    ///
    /// A `acao` vai no parâmetro `action` do `Content-Type`, como exigem os
    /// serviços da SEFAZ. Quebras de linha e aspas na ação são recusadas para
    /// não permitir injeção de cabeçalho.
    pub fn post_soap12(
        url: impl Into<String>,
        acao: &str,
        corpo_xml: &str,
    ) -> Result<Self, ErroTransporte> {
        if acao.trim().is_empty()
            || acao.contains(['"', '\r', '\n'])
            || (!acao.starts_with("http://") && !acao.starts_with("https://"))
        {
            return Err(ErroTransporte::Configuracao(format!(
                "ação SOAP inválida: {acao:?}"
            )));
        }
        Ok(Self {
            metodo: Metodo::Post,
            url: url.into(),
            tipo_conteudo: Some(format!(
                "application/soap+xml; charset=utf-8; action=\"{acao}\""
            )),
            aceitar: Some("application/soap+xml".to_owned()),
            corpo: envelope_soap12(corpo_xml).into_bytes(),
        })
    }

    /// `POST` com tipo de conteúdo explícito.
    pub fn post(
        url: impl Into<String>,
        tipo_conteudo: impl Into<String>,
        corpo: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            metodo: Metodo::Post,
            url: url.into(),
            tipo_conteudo: Some(tipo_conteudo.into()),
            aceitar: None,
            corpo: corpo.into(),
        }
    }
}

/// Monta o `Envelope` SOAP 1.2 com o corpo informado.
pub fn envelope_soap12(corpo_xml: &str) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="utf-8"?>"#,
            r#"<soap12:Envelope xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" "#,
            r#"xmlns:xsd="http://www.w3.org/2001/XMLSchema" "#,
            r#"xmlns:soap12="http://www.w3.org/2003/05/soap-envelope">"#,
            "<soap12:Body>{}</soap12:Body>",
            "</soap12:Envelope>"
        ),
        corpo_xml.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_soap12_envolve_o_corpo() {
        let envelope = envelope_soap12("<nfeDadosMsg/>");
        assert!(envelope.contains("http://www.w3.org/2003/05/soap-envelope"));
        assert!(envelope.contains("<soap12:Body><nfeDadosMsg/></soap12:Body>"));
    }

    #[test]
    fn soap_recusa_acao_com_quebra_de_linha() {
        let erro = Requisicao::post_soap12("https://exemplo.invalid", "http://x\r\nX: y", "<a/>")
            .expect_err("ação inválida");
        assert!(matches!(erro, ErroTransporte::Configuracao(_)));
    }

    #[test]
    fn metodos_serializam() {
        assert_eq!(Metodo::Get.as_str(), "GET");
        assert_eq!(Metodo::Post.as_str(), "POST");
        assert_eq!(Metodo::Head.as_str(), "HEAD");
    }

    #[test]
    fn get_json_aceita_json_sem_corpo() {
        let requisicao = Requisicao::get_json("https://adn.invalid/consulta");
        assert_eq!(requisicao.metodo, Metodo::Get);
        assert_eq!(requisicao.aceitar.as_deref(), Some("application/json"));
        assert!(requisicao.tipo_conteudo.is_none());
        assert!(requisicao.corpo.is_empty());
    }
}
