//! Eventos de cancelamento/CC-e e inutilização da NF-e.

use crate::erro::ErroNfe;
use carimbar_endpoints::Uf;
use carimbar_types::{Cnpj, TpAmb};
use carimbar_xml::{Documento, Elemento};
#[cfg(feature = "nfe-sha1")]
use carimbar_xml::{No, Nome};

/// Evento de cancelamento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventoCancelamento {
    /// Ambiente.
    pub ambiente: TpAmb,
    /// UF.
    pub uf: Uf,
    /// CNPJ do autor.
    pub cnpj: Cnpj,
    /// Chave da NF-e.
    pub chave: String,
    /// Protocolo de autorização.
    pub protocolo: String,
    /// Data/hora do evento.
    pub dh_evento: String,
    /// Justificativa (15 a 255 caracteres).
    pub justificativa: String,
    /// Sequencial do evento.
    pub n_seq_evento: u8,
}

/// Evento de carta de correção.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventoCce {
    /// Ambiente.
    pub ambiente: TpAmb,
    /// UF.
    pub uf: Uf,
    /// CNPJ do autor.
    pub cnpj: Cnpj,
    /// Chave da NF-e.
    pub chave: String,
    /// Data/hora do evento.
    pub dh_evento: String,
    /// Correção.
    pub correcao: String,
    /// Sequencial do evento.
    pub n_seq_evento: u8,
}

/// Inutilização de numeração.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inutilizacao {
    /// Ambiente.
    pub ambiente: TpAmb,
    /// UF.
    pub uf: Uf,
    /// CNPJ.
    pub cnpj: Cnpj,
    /// Ano (2 dígitos).
    pub ano: u8,
    /// Série.
    pub serie: u32,
    /// Número inicial.
    pub numero_inicial: u64,
    /// Número final.
    pub numero_final: u64,
    /// Justificativa (15 a 255 caracteres).
    pub justificativa: String,
}

impl EventoCancelamento {
    /// Monta o `evento` assinado.
    #[cfg(feature = "nfe-sha1")]
    pub fn assinar(&self, assinante: &dyn carimbar_dsig::Signer) -> Result<String, ErroNfe> {
        let id = format!("ID{}{:06}{:03}", self.chave, 110111, self.n_seq_evento);
        let inf = el("infEvento")
            .com_atributo_texto("Id", &id)
            .com_filho(No::Elemento(txt("cOrgao", self.uf.codigo())))
            .com_filho(No::Elemento(txt("tpAmb", self.ambiente.codigo())))
            .com_filho(No::Elemento(txt("CNPJ", self.cnpj.as_str())))
            .com_filho(No::Elemento(txt("chNFe", &self.chave)))
            .com_filho(No::Elemento(txt("dhEvento", &self.dh_evento)))
            .com_filho(No::Elemento(txt("tpEvento", 110111)))
            .com_filho(No::Elemento(txt("nSeqEvento", self.n_seq_evento)))
            .com_filho(No::Elemento(txt("verEvento", "1.00")))
            .com_filho(No::Elemento(
                el("detEvento")
                    .com_atributo_texto("versao", "1.00")
                    .com_filho(No::Elemento(txt("descEvento", "Cancelamento")))
                    .com_filho(No::Elemento(txt("nProt", &self.protocolo)))
                    .com_filho(No::Elemento(txt("xJust", &self.justificativa))),
            ));
        let raiz = el("evento")
            .com_namespace(None, crate::modelo::NS_NFE)
            .com_atributo_texto("versao", "1.00")
            .com_filho(No::Elemento(inf));
        assinar_elemento(Documento::new(raiz), &id, assinante)
    }

    /// Monta o `envEvento` assinado.
    #[cfg(feature = "nfe-sha1")]
    pub fn env_evento(&self, assinante: &dyn carimbar_dsig::Signer) -> Result<String, ErroNfe> {
        let evento = self.assinar(assinante)?;
        Ok(format!(
            "<envEvento xmlns=\"{}\" versao=\"1.00\"><idLote>1</idLote>{evento}</envEvento>",
            crate::modelo::NS_NFE
        ))
    }
}

impl EventoCce {
    /// Monta o `envEvento` assinado.
    #[cfg(feature = "nfe-sha1")]
    pub fn env_evento(&self, assinante: &dyn carimbar_dsig::Signer) -> Result<String, ErroNfe> {
        let id = format!("ID{}{:06}{:03}", self.chave, 110110, self.n_seq_evento);
        let inf = el("infEvento")
            .com_atributo_texto("Id", &id)
            .com_filho(No::Elemento(txt("cOrgao", self.uf.codigo())))
            .com_filho(No::Elemento(txt("tpAmb", self.ambiente.codigo())))
            .com_filho(No::Elemento(txt("CNPJ", self.cnpj.as_str())))
            .com_filho(No::Elemento(txt("chNFe", &self.chave)))
            .com_filho(No::Elemento(txt("dhEvento", &self.dh_evento)))
            .com_filho(No::Elemento(txt("tpEvento", 110110)))
            .com_filho(No::Elemento(txt("nSeqEvento", self.n_seq_evento)))
            .com_filho(No::Elemento(txt("verEvento", "1.00")))
            .com_filho(No::Elemento(
                el("detEvento")
                    .com_atributo_texto("versao", "1.00")
                    .com_filho(No::Elemento(txt(
                        "descEvento",
                        "Carta de Correcao",
                    )))
                    .com_filho(No::Elemento(txt("xCorrecao", &self.correcao)))
                    .com_filho(No::Elemento(txt(
                        "xCondUso",
                        "A Carta de Correcao e disciplinada pelo paragrafo 1o-A do art. 7o do Convenio S/N, de 15 de dezembro de 1970 e pode ser utilizada para regularizacao de erro ocorrido na emissao de documento fiscal, desde que o erro nao esteja relacionado com: I - as variaveis que determinam o valor do imposto tais como: base de calculo, aliquota, diferenca de preco, quantidade, valor da operacao ou da prestacao; II - a correcao de dados cadastrais que implique mudanca do remetente ou do destinatario; III - a data de emissao ou de saida.",
                    ))),
            ));
        let raiz = el("evento")
            .com_namespace(None, crate::modelo::NS_NFE)
            .com_atributo_texto("versao", "1.00")
            .com_filho(No::Elemento(inf));
        let evento = assinar_elemento(Documento::new(raiz), &id, assinante)?;
        Ok(format!(
            "<envEvento xmlns=\"{}\" versao=\"1.00\"><idLote>1</idLote>{evento}</envEvento>",
            crate::modelo::NS_NFE
        ))
    }
}

impl Inutilizacao {
    /// Monta o `inutNFe` assinado.
    #[cfg(feature = "nfe-sha1")]
    pub fn assinar(&self, assinante: &dyn carimbar_dsig::Signer) -> Result<String, ErroNfe> {
        let id = format!(
            "ID{}{:02}{:03}{:09}{:09}",
            self.cnpj.as_str(),
            self.ano,
            self.serie,
            self.numero_inicial,
            self.numero_final
        );
        let inf = el("infInut")
            .com_atributo_texto("Id", &id)
            .com_filho(No::Elemento(txt("tpAmb", self.ambiente.codigo())))
            .com_filho(No::Elemento(txt("xServ", "INUTILIZAR")))
            .com_filho(No::Elemento(txt("cUF", self.uf.codigo())))
            .com_filho(No::Elemento(txt("ano", self.ano)))
            .com_filho(No::Elemento(txt("CNPJ", self.cnpj.as_str())))
            .com_filho(No::Elemento(txt("mod", 55)))
            .com_filho(No::Elemento(txt("serie", self.serie)))
            .com_filho(No::Elemento(txt("nNFIni", self.numero_inicial)))
            .com_filho(No::Elemento(txt("nNFFin", self.numero_final)))
            .com_filho(No::Elemento(txt("xJust", &self.justificativa)));
        let raiz = el("inutNFe")
            .com_namespace(None, crate::modelo::NS_NFE)
            .com_atributo_texto("versao", "4.00")
            .com_filho(No::Elemento(inf));
        assinar_elemento(Documento::new(raiz), &id, assinante)
    }
}

#[cfg(feature = "nfe-sha1")]
fn assinar_elemento(
    documento: Documento,
    id: &str,
    assinante: &dyn carimbar_dsig::Signer,
) -> Result<String, ErroNfe> {
    let assinado = carimbar_dsig::assinar_com_perfil(
        &documento,
        id,
        assinante,
        carimbar_dsig::PerfilAssinatura::NfeSha1,
    )?;
    Ok(assinado.to_xml()?)
}

/// Resultado de evento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultadoEvento {
    /// Registrado.
    Registrado(String),
    /// Rejeitado.
    Rejeitado(String),
}

/// Interpreta `retEnvEvento`.
pub fn interpretar_ret_evento(corpo: &[u8]) -> Result<ResultadoEvento, ErroNfe> {
    let documento = Documento::parse(corpo)?;
    let ret = procurar(&documento, "retEnvEvento")
        .ok_or_else(|| ErroNfe::Resposta("retEnvEvento ausente".to_owned()))?;
    let codigo = texto(ret, "cStat").unwrap_or_default();
    let motivo = texto(ret, "xMotivo").unwrap_or_default();
    if codigo == "128" || codigo == "135" {
        return Ok(ResultadoEvento::Registrado(
            String::from_utf8_lossy(corpo).into_owned(),
        ));
    }
    Ok(ResultadoEvento::Rejeitado(format!("{codigo}: {motivo}")))
}

fn procurar<'a>(documento: &'a Documento, local: &str) -> Option<&'a Elemento> {
    documento
        .caminhos_por_local(local)
        .first()
        .and_then(|caminho| documento.elemento_em(caminho))
}

fn texto(elemento: &Elemento, local: &str) -> Option<String> {
    elemento
        .primeiro_filho(local)
        .map(Elemento::texto)
        .filter(|valor| !valor.is_empty())
}

#[cfg(feature = "nfe-sha1")]
fn el(local: &str) -> Elemento {
    Elemento::new(Nome::new(local))
}

#[cfg(feature = "nfe-sha1")]
fn txt(local: &str, valor: impl ToString) -> Elemento {
    el(local).com_texto(valor.to_string())
}
