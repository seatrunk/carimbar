//! Tabela UF × modelo × ambiente × serviço → URL, versionada como dado com
//! data de verificação e fonte.
//!
//! A resolução é puramente local: nenhum teste ou consulta acessa SEFAZ,
//! Sefin ou ADN. O crate não embute certificados; o transporte mTLS é
//! responsabilidade de `carimbar::transport`.
//!
//! # Escopo e limites
//!
//! - NF-e modelo 55: autorizadores normais publicados no Portal Nacional da
//!   NF-e, SVC-AN, SVC-RS e distribuição do Ambiente Nacional.
//! - NFS-e Nacional: Sefin Nacional e ADN (contribuintes e DANFSe).
//! - Sefins municipais que não aderiram ao padrão nacional não são resolvidas.
//! - As URLs foram conferidas em [`VERIFICADO_EM`]. Endpoints mudam por nota
//!   técnica; antes de emitir, reconfira a fonte indicada em [`Endpoint::fonte`].
//!
//! # Exemplo
//!
//! ```
//! use carimbar::endpoints::{ConsultaNfe, ServicoNfe, Uf};
//! use carimbar::types::TpAmb;
//!
//! let consulta = ConsultaNfe::new(Uf::SaoPaulo, ServicoNfe::Autorizacao, TpAmb::Homologacao);
//! let endpoint = carimbar::endpoints::resolver_nfe(&consulta).expect("URL publicada");
//! assert!(endpoint.url.contains("homologacao.nfe.fazenda.sp.gov.br"));
//! assert_eq!(endpoint.versao, "4.00");
//! ```

mod erro;
mod nfe;
mod nfse;
mod parametros;
mod uf;

pub use erro::ErroEndpoints;
pub use nfe::{
    Autorizador, ConsultaNfe, Contingencia, ServicoNfe, autorizador_principal, resolver_nfe,
};
pub use nfse::{ConsultaNfse, ServicoNfse, resolver_nfse};
pub use parametros::{ConsultaParametros, ServicoParametros, resolver_parametros};
pub use uf::Uf;

/// Data em que as URLs deste crate foram conferidas nas fontes oficiais.
pub const VERIFICADO_EM: &str = "2026-11-08";

/// Fonte oficial usada para conferir as URLs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Fonte {
    /// Portal Nacional da NF-e, página "Relação de Serviços Web".
    PortalNfeNacional,
    /// Portal da NFS-e, documentação técnica atual.
    PortalNfse,
}

impl Fonte {
    /// URL da fonte.
    pub fn url(self) -> &'static str {
        match self {
            Self::PortalNfeNacional => {
                "https://www.nfe.fazenda.gov.br/portal/webServices.aspx?tipoConteudo=OUC%2FYVNWZfo%3D"
            }
            Self::PortalNfse => {
                "https://www.gov.br/nfse/pt-br/biblioteca/documentacao-tecnica/documentacao-atual"
            }
        }
    }
}

/// URL resolvida, com a fonte e a data de verificação.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// URL do serviço. Pode conter placeholders entre chaves.
    pub url: String,
    /// Versão do leiaute ou contrato publicado.
    pub versao: &'static str,
    /// Autorizador da NF-e; `None` para a NFS-e Nacional.
    pub autorizador: Option<Autorizador>,
    /// Fonte oficial conferida.
    pub fonte: Fonte,
    /// Data da conferência.
    pub verificado_em: &'static str,
}

impl Endpoint {
    /// Nomes dos placeholders presentes na URL, na ordem em que aparecem.
    pub fn placeholders(&self) -> Vec<&str> {
        let mut nomes = Vec::new();
        let mut resto = self.url.as_str();
        while let Some(abre) = resto.find('{') {
            let depois = &resto[abre + 1..];
            let Some(fecha) = depois.find('}') else {
                break;
            };
            let nome = &depois[..fecha];
            if !nomes.contains(&nome) {
                nomes.push(nome);
            }
            resto = &depois[fecha + 1..];
        }
        nomes
    }

    /// Preenche os placeholders da URL, aplicando escape percentual nos valores.
    ///
    /// Falha se um placeholder ficar sem valor, se um valor for vazio ou se for
    /// informado um nome que a URL não usa. Nada é enviado pela rede.
    pub fn preencher(&self, parametros: &[(&str, &str)]) -> Result<String, ErroEndpoints> {
        for (nome, valor) in parametros {
            if valor.trim().is_empty() {
                return Err(ErroEndpoints::ParametroAusente((*nome).to_owned()));
            }
            if !self.placeholders().contains(nome) {
                return Err(ErroEndpoints::PlaceholderDesconhecido((*nome).to_owned()));
            }
        }

        let mut saida = String::with_capacity(self.url.len());
        let bytes = self.url.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'{' {
                let fim = self.url[i + 1..]
                    .find('}')
                    .map(|pos| i + 1 + pos)
                    .ok_or_else(|| ErroEndpoints::Placeholder(self.url.clone()))?;
                let nome = &self.url[i + 1..fim];
                let valor = parametros
                    .iter()
                    .find(|(n, _)| *n == nome)
                    .map(|(_, v)| *v)
                    .ok_or_else(|| ErroEndpoints::Placeholder(nome.to_owned()))?;
                saida.push_str(&escapar(valor));
                i = fim + 1;
            } else {
                saida.push(char::from(bytes[i]));
                i += 1;
            }
        }
        Ok(saida)
    }
}

fn escapar(valor: &str) -> String {
    let mut saida = String::with_capacity(valor.len());
    for byte in valor.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            saida.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(saida, "%{byte:02X}");
        }
    }
    saida
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(url: &str) -> Endpoint {
        Endpoint {
            url: url.to_owned(),
            versao: "1.01",
            autorizador: None,
            fonte: Fonte::PortalNfse,
            verificado_em: VERIFICADO_EM,
        }
    }

    #[test]
    fn preenche_e_escapa_parametros() {
        let e = endpoint("https://exemplo.invalid/NFSe/{chave}/Eventos/{tipo}");
        let url = e
            .preencher(&[("chave", "123/abc"), ("tipo", "cancelamento")])
            .expect("preenchida");
        assert_eq!(
            url,
            "https://exemplo.invalid/NFSe/123%2Fabc/Eventos/cancelamento"
        );
        assert_eq!(e.placeholders(), vec!["chave", "tipo"]);
    }

    #[test]
    fn recusa_placeholder_ausente_vazio_ou_desconhecido() {
        let e = endpoint("https://exemplo.invalid/{chave}");
        assert_eq!(
            e.preencher(&[]),
            Err(ErroEndpoints::Placeholder("chave".to_owned()))
        );
        assert_eq!(
            e.preencher(&[("chave", " ")]),
            Err(ErroEndpoints::ParametroAusente("chave".to_owned()))
        );
        assert_eq!(
            e.preencher(&[("chave", "1"), ("extra", "2")]),
            Err(ErroEndpoints::PlaceholderDesconhecido("extra".to_owned()))
        );
    }

    #[test]
    fn url_sem_placeholders_nao_muda() {
        let e = endpoint("https://exemplo.invalid/nfse");
        assert!(e.placeholders().is_empty());
        assert_eq!(
            e.preencher(&[]).as_deref(),
            Ok("https://exemplo.invalid/nfse")
        );
    }
}
