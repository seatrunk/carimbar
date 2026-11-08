//! Escrita e leitura de XML fiscal, canonicalização C14N 1.0 e validação XSD.
//!
//! A leitura é deliberadamente restrita:
//!
//! - aceita apenas UTF-8;
//! - rejeita DTD e entidades não predefinidas;
//! - impõe limites de bytes, profundidade, elementos e atributos;
//! - valida nomes, namespaces e atributos repetidos.
//!
//! A escrita é determinística e preserva prefixos e declarações de namespace. A
//! canonicalização implementa o perfil inclusivo sem comentários usado por
//! XMLDSig (`http://www.w3.org/TR/2001/REC-xml-c14n-20010315`).
//!
//! A validação XSD fica atrás da feature `xsd`. Os schemas oficiais não são
//! embutidos no pacote: o chamador informa o caminho do XSD principal e o
//! validador resolve `xs:include`/`xs:import` relativos a ele. Veja
//! `docs/contratos-fiscais.md` para os pacotes e seus SHA-256.
//!
//! Status: implementação da Etapa 1 do plano (`docs/plano-mvp.md`, issue #7).

mod canonico;
mod documento;
mod erro;
mod escrita;
mod leitura;
mod validacao;

#[cfg(feature = "xsd")]
pub mod xsd;

pub use documento::{
    Atributo, Declaracao, Documento, Elemento, Limites, No, Nome, PREFIXO_XML, URI_XML, URI_XMLNS,
};
pub use erro::ErroXml;
pub use leitura::{ler, ler_com_limites};
