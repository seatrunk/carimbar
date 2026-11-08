//! Metadados públicos de certificados armazenados.

use std::time::SystemTime;

use carimbar_types::Cnpj;

/// Metadados de um certificado importado.
///
/// Não há PFX, senha, DEK ou impressão digital da chave privada aqui: apenas o
/// necessário para listar e escolher o certificado. `sha256` é o hash do
/// certificado folha em DER, não do PKCS#12.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadadosCertificado {
    /// Empresa dona do certificado.
    pub empresa: Cnpj,
    /// Identificador escolhido na importação.
    pub id: String,
    /// CNPJ do titular extraído do certificado.
    pub titular: Cnpj,
    /// Início da validade da folha.
    pub valido_de: SystemTime,
    /// Fim da validade da folha.
    pub valido_ate: SystemTime,
    /// SHA-256 do certificado folha em DER, em hexadecimal minúsculo.
    pub sha256: String,
    /// Instante da importação.
    pub importado_em: SystemTime,
}
