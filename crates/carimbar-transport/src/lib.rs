//! Cliente HTTP/1.1 sobre TLS mútuo para SEFAZ e Sefin/ADN.
//!
//! O transporte é a camada D3 do `docs/contratos-fiscais.md`:
//!
//! - handshake mTLS com `rustls`, usando a cadeia ICP-Brasil fornecida pelo
//!   chamador em DER (nenhuma raiz é embutida no crate);
//! - HTTP/1.1 mínimo e estrito, sem redirecionamento e sem reuso de conexão;
//! - corpo SOAP 1.2 ou JSON já serializado;
//! - classificação de falhas entre "ainda não enviei" e "resultado
//!   desconhecido", para que envios de emissão nunca sejam repetidos às cegas.
//!
//! # Exemplo
//!
//! ```no_run
//! use carimbar_transport::{ClienteMtls, Opcoes, Requisicao};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let identidade = carimbar_transport::Identidade::de_der(
//!     vec![/* certificado folha em DER */],
//!     vec![/* chave PKCS#8 em DER */],
//! )?;
//! let raizes = vec![/* raiz de confiança em DER */];
//! let cliente = ClienteMtls::novo(identidade, &raizes, Opcoes::default())?;
//!
//! let requisicao = Requisicao::post_soap12(
//!     "https://homologacao.exemplo.invalid/nfe",
//!     "http://www.portalfiscal.inf.br/nfe/wsdl/NFeStatusServico4/nfeStatusServicoNF",
//!     "<nfeDadosMsg/>",
//! )?;
//! let resposta = cliente.executar_seguro(&requisicao)?;
//! # let _ = resposta;
//! # Ok(())
//! # }
//! ```
//!
//! # Segurança
//!
//! `executar` faz uma única tentativa e distingue:
//!
//! - [`ErroEnvio::AntesDoEnvio`]: a requisição comprovadamente não chegou a ser
//!   escrita (URL, DNS, TCP, handshake TLS ou montagem do cabeçalho);
//! - [`ErroEnvio::ResultadoDesconhecido`]: a escrita começou, então o serviço
//!   pode ter processado a operação.
//!
//! Para consultas e demais operações idempotentes, [`ClienteMtls::executar_seguro`]
//! repete falhas transitórias e respostas 429/5xx.

#![deny(missing_docs)]

mod erro;
mod http;
mod requisicao;

pub use erro::{ErroEnvio, ErroTransporte};
pub use http::{ClienteMtls, Identidade, LIMITE_PADRAO, Opcoes, Resposta};
pub use requisicao::{Metodo, Requisicao, envelope_soap12};
