//! Leitura de certificados A1 PKCS#12 ICP-Brasil: senha, validade, vínculo do
//! titular ao CNPJ e chave privada protegida em memória.
//!
//! A chave privada nunca sai do crate em forma de texto: [`CertificadoA1`]
//! só expõe assinatura RSA-SHA256 e a identidade para mTLS, e não tem
//! `Clone`. `Debug` oculta a chave e a senha nunca é guardada.
//! A feature opcional `nfe-sha1` acrescenta assinatura RSA-SHA1 pelo backend
//! OpenSSL para o contrato legado da NF-e. O grafo padrão não depende de OpenSSL.
//!
//! Limite conhecido: o `p12-keystore` mantém sua própria cópia da chave ao
//! abrir o arquivo, e essa cópia não é zerada. Só a cópia de [`CertificadoA1`]
//! é zerada ao sair de escopo.
//!
//! Status: implementação da Etapa 1 do plano (`docs/plano-mvp.md`, issue #8).

mod certificado;
mod erro;

pub use certificado::{CertificadoA1, IdentidadeTls, OID_CNPJ_ICP_BRASIL};
pub use erro::ErroCertificado;
