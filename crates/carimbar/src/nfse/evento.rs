//! Pedido de registro de evento de cancelamento da NFS-e.
//!
//! O leiaute aceita uma escolha entre vários eventos. O MVP implementa o
//! cancelamento (`e101101`), que é a operação exigida para corrigir uma nota
//! emitida. Outros eventos podem ser acrescentados sem mudar o contrato do
//! cliente.

use crate::dsig::{Signer, assinar};
use crate::nfse::dps::{NS_NFSE, VERSAO_LEIAUTE, el, exigir_texto, txt};
use crate::nfse::erro::ErroNfse;
use crate::nfse::identificadores::{ChaveNfse, InscricaoFederal};
use crate::nfse::tempo::DataHoraFiscal;
use crate::types::TpAmb;
use crate::xml::{Documento, No};

/// Código do evento de cancelamento.
pub const CODIGO_CANCELAMENTO: &str = "101101";

/// Motivo do cancelamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotivoCancelamento {
    /// `1` — erro na emissão.
    ErroEmissao,
    /// `2` — serviço não prestado.
    ServicoNaoPrestado,
    /// `9` — outros.
    Outros,
}

impl MotivoCancelamento {
    /// Código no XML.
    pub fn codigo(self) -> u8 {
        match self {
            Self::ErroEmissao => 1,
            Self::ServicoNaoPrestado => 2,
            Self::Outros => 9,
        }
    }
}

/// Pedido de registro do evento de cancelamento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventoCancelamento {
    /// Ambiente do XML e do endpoint.
    pub ambiente: TpAmb,
    /// Chave da NFS-e a cancelar.
    pub chave: ChaveNfse,
    /// CNPJ ou CPF do autor do evento.
    pub autor: InscricaoFederal,
    /// Versão do aplicativo emissor, até 20 posições.
    pub ver_aplic: String,
    /// Data e hora do evento.
    pub dh_evento: DataHoraFiscal,
    /// Motivo.
    pub motivo: MotivoCancelamento,
    /// Descrição do motivo, de 15 a 255 caracteres.
    pub descricao: String,
}

impl EventoCancelamento {
    /// Identificador do pedido: `PRE` + chave + código do evento.
    ///
    /// O número sequencial do evento não entra neste Id; ele integra o Id do
    /// evento gerado pelo ambiente (`EVT...`).
    pub fn id(&self) -> String {
        format!("PRE{}{CODIGO_CANCELAMENTO}", self.chave.as_str())
    }

    /// Valida os campos.
    pub fn validar(&self) -> Result<(), ErroNfse> {
        ChaveNfse::parse(self.chave.as_str(), self.ambiente)?;
        if self.ambiente == TpAmb::Producao && self.autor.alfanumerica() {
            return Err(ErroNfse::campo(
                "CNPJAutor",
                "CNPJ alfanumérico não é aceito na produção",
            ));
        }
        exigir_texto("verAplic", &self.ver_aplic, 1, 20)?;
        exigir_texto("xMotivo", &self.descricao, 15, 255)?;
        Ok(())
    }

    /// Monta o XML do pedido (sem assinatura).
    pub fn documento(&self) -> Result<Documento, ErroNfse> {
        self.validar()?;
        let autor = match &self.autor {
            InscricaoFederal::Cnpj(cnpj) => txt("CNPJAutor", cnpj.as_str()),
            InscricaoFederal::Cpf(cpf) => txt("CPFAutor", cpf),
        };
        let evento = el("e101101")
            .com_filho(No::Elemento(txt("xDesc", "Cancelamento de NFS-e")))
            .com_filho(No::Elemento(txt("cMotivo", self.motivo.codigo())))
            .com_filho(No::Elemento(txt("xMotivo", &self.descricao)));
        let inf = el("infPedReg")
            .com_atributo_texto("Id", self.id())
            .com_filho(No::Elemento(txt("tpAmb", self.ambiente.codigo())))
            .com_filho(No::Elemento(txt("verAplic", &self.ver_aplic)))
            .com_filho(No::Elemento(txt("dhEvento", self.dh_evento.to_string())))
            .com_filho(No::Elemento(autor))
            .com_filho(No::Elemento(txt("chNFSe", self.chave.as_str())))
            .com_filho(No::Elemento(evento));
        let raiz = el("pedRegEvento")
            .com_namespace(None, NS_NFSE)
            .com_atributo_texto("versao", VERSAO_LEIAUTE)
            .com_filho(No::Elemento(inf));
        Ok(Documento::new(raiz))
    }

    /// Serializa o pedido sem assinatura.
    pub fn para_xml(&self) -> Result<String, ErroNfse> {
        Ok(self.documento()?.to_xml()?)
    }

    /// Assina o pedido com XMLDSig sobre `infPedReg`.
    pub fn assinar(&self, assinante: &dyn Signer) -> Result<Documento, ErroNfse> {
        let documento = self.documento()?;
        Ok(assinar(&documento, &self.id(), assinante)?)
    }

    /// Serializa o pedido assinado.
    pub fn para_xml_assinado(&self, assinante: &dyn Signer) -> Result<String, ErroNfse> {
        Ok(self.assinar(assinante)?.to_xml()?)
    }

    /// Valida o pedido contra um XSD informado pelo chamador.
    #[cfg(feature = "xsd")]
    pub fn validar_xsd(&self, esquema: impl AsRef<std::path::Path>) -> Result<(), ErroNfse> {
        let xml = self.para_xml()?;
        crate::xml::xsd::validar(xml.as_bytes(), esquema)
            .map_err(|erro| ErroNfse::Xsd(erro.to_string()))
    }
}
