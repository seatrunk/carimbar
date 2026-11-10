//! DPS v1.01, eventos de cancelamento e cliente das APIs JSON da NFS-e
//! Nacional.
//!
//! # Escopo
//!
//! - modelo da DPS com os campos exigidos pelo leiaute e pelo grupo IBS/CBS;
//! - geração e validação do `Id` da DPS e da chave da NFS-e, respeitando as
//!   diferenças entre produção e produção restrita;
//! - serialização na ordem do XSD oficial e assinatura XMLDSig SHA-256;
//! - pedido de registro do evento de cancelamento;
//! - cliente Sefin para emissão, consulta de NFS-e, consulta de DPS e registro
//!   de evento, com GZip/base64 e interpretação de autorização, rejeição,
//!   duplicidade e resultado desconhecido.
//!
//! # Limites
//!
//! O crate não embute raízes de confiança, certificados nem schemas oficiais.
//! O chamador fornece o `ClienteMtls` de `carimbar::transport` e, quando quiser
//! validar antes do envio, o caminho do XSD oficial (veja `schemas/` e
//! `docs/contratos-fiscais.md`).
//!
//! A DPS cobre os grupos obrigatórios e os campos opcionais mais usados de
//! prestador, tomador, intermediário, serviço, valores e IBS/CBS. Endereço,
//! telefone, comércio exterior, obra, evento, benefício municipal e os grupos
//! de crédito presumido/diferimento do IBS/CBS ficam para incremento.
//!
//! # Exemplo
//!
//! ```no_run
//! # use carimbar::nfse::{CodigoMunicipio, DataFiscal, DataHoraFiscal, Dps, InscricaoFederal, LocalPrestacao, NumeroDps, Prestador, RegimeTributario, SerieDps, Servico, TotalTributos, TribIssqn, TributoMunicipal, TipoRetencaoIssqn, Valores};
//! # use carimbar::types::{Dinheiro, TpAmb};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let dps = Dps {
//!     ambiente: TpAmb::Homologacao,
//!     dh_emi: DataHoraFiscal::parse("2026-11-08T10:00:00-03:00")?,
//!     ver_aplic: "carimbar-0.0.3".to_owned(),
//!     serie: SerieDps::new(1)?,
//!     numero: NumeroDps::new(1)?,
//!     d_compet: DataFiscal::parse("2026-11-08")?,
//!     tp_emit: carimbar::nfse::TipoEmitente::Prestador,
//!     c_motivo_emis_ti: None,
//!     ch_nfse_rej: None,
//!     c_loc_emi: CodigoMunicipio::new("3550308")?,
//!     substituicao: None,
//!     prestador: Prestador {
//!         inscricao: InscricaoFederal::parse("00000001000136")?,
//!         inscricao_municipal: None,
//!         nome: None,
//!         email: None,
//!         regime: RegimeTributario::nao_optante(),
//!     },
//!     tomador: None,
//!     intermediario: None,
//!     servico: Servico {
//!         local_prestacao: LocalPrestacao::Municipio(CodigoMunicipio::new("3550308")?),
//!         codigo_tributacao_nacional: "010101".to_owned(),
//!         codigo_tributacao_municipal: None,
//!         codigo_nbs: None,
//!         codigo_interno: None,
//!         descricao: "Serviço sintético".to_owned(),
//!         informacoes_complementares: None,
//!     },
//!     valores: Valores {
//!         valor_recebido: None,
//!         valor_servico: "10.00".parse::<Dinheiro>()?,
//!         desconto_incondicional: None,
//!         desconto_condicional: None,
//!         tributo_municipal: TributoMunicipal {
//!             trib_issqn: TribIssqn::Tributavel,
//!             tipo_retencao: TipoRetencaoIssqn::NaoRetido,
//!             aliquota: None,
//!         },
//!         total_tributos: TotalTributos::IndicadorZero,
//!     },
//!     ibs_cbs: None,
//! };
//! let xml = dps.para_xml()?;
//! assert!(xml.contains("infDPS"));
//! # Ok(())
//! # }
//! ```
//!
//! Status: implementação da Etapa 3 do plano (`docs/plano-mvp.md`, issue #11).

#![deny(missing_docs)]

mod adn;
mod api;
mod dps;
mod erro;
mod evento;
mod gzip;
mod identificadores;
mod parametros;
mod tempo;

pub use adn::{
    ClienteAdn, DocumentoDistribuido, LoteDistribuicao, MensagemAdn, StatusDistribuicao,
    TipoDocumento,
};
pub use api::{
    ClienteSefin, DpsLocalizada, EmissaoAutorizada, ErroFiscal, EventoRegistrado, Executor,
    NfseConsultada, Rejeicao, ResultadoEmissao, ResultadoEvento,
};
pub use dps::{
    Destinatario, Dps, IbsCbs, LocalPrestacao, MotivoEmissaoTi, MotivoSubstituicao, NS_NFSE,
    OpSimplesNacional, Pessoa, Prestador, RegimeApuracaoSimples, RegimeEspecialTributacao,
    RegimeTributario, Servico, Substituicao, TipoEmitente, TipoRetencaoIssqn, TotalTributos,
    TribIssqn, TributoMunicipal, VERSAO_LEIAUTE, Valores,
};
pub use erro::ErroNfse;
pub use evento::{CODIGO_CANCELAMENTO, EventoCancelamento, MotivoCancelamento};
pub use gzip::{LIMITE_XML, comprimir_b64, descomprimir_b64};
pub use identificadores::{
    ChaveNfse, CodigoMunicipio, IdDps, InscricaoFederal, NumeroDps, SerieDps, TipoInscricao,
};
pub use parametros::{
    Aliquota, CacheParametros, ClienteParametros, ConsultaAliquotas, ParametrosConvenio,
};
pub use tempo::{DataFiscal, DataHoraFiscal};
