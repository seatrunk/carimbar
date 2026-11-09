//! Chave mestra (KEK), provedor de envelope e cifra AES-256-GCM.
//!
//! A KEK nunca é gravada: ou vem do chamador, ou é derivada de uma senha com
//! Argon2id e sal público. O que fica no arquivo é apenas o verificador
//! autenticado e as DEKs já embrulhadas.

use aes_gcm::aead::{Aead, Nonce as AeadNonce, Payload};
use aes_gcm::{Aes256Gcm, KeyInit};
use zeroize::Zeroizing;

use crate::vault::erro::ErroCofre;

/// Tamanho da KEK e da DEK, em bytes (AES-256).
pub const TAMANHO_CHAVE: usize = 32;
const TAMANHO_NONCE: usize = 12;
const TAMANHO_TAG: usize = 16;
/// Menor bloco selado válido (nonce + tag).
pub const TAMANHO_MINIMO_SELADO: usize = TAMANHO_NONCE + TAMANHO_TAG;

/// Texto fixo autenticado para reconhecer a KEK correta.
pub(crate) const SENTINELA: &[u8] = b"carimbar-vault/v1";

/// Contexto autenticado do verificador de KEK.
pub(crate) const AAD_VERIFICADOR: &[u8] = b"carimbar-vault/v1/verificador";

/// Provedor de envelope: sela e abre pequenos segredos (DEKs) com uma KEK.
///
/// A implementação local ([`ChaveMestra`]) usa AES-256-GCM. Um provedor de KMS
/// pode implementar o mesmo contrato delegando `Encrypt`/`Decrypt` ao serviço,
/// desde que o `aad` seja autenticado ou vinculado ao contexto.
///
/// Implementações não devem registrar `texto` nem `selado` em log ou erro.
pub trait ProvedorKek: Send + Sync {
    /// Sela `texto`, vinculando-o ao contexto `aad`.
    fn selar(&self, texto: &[u8], aad: &[u8]) -> Result<Vec<u8>, ErroCofre>;

    /// Abre um bloco selado e confere o contexto `aad`.
    ///
    /// # Errors
    ///
    /// Retorna [`ErroCofre::ChaveIncorreta`] quando a chave não autentica o
    /// bloco e [`ErroCofre::RegistroAdulterado`] quando o bloco é curto demais.
    fn abrir(&self, selado: &[u8], aad: &[u8]) -> Result<Zeroizing<Vec<u8>>, ErroCofre>;
}

/// Chave mestra local de 32 bytes.
///
/// A chave é zerada ao sair de escopo e nunca aparece em `Debug`. Ela não é
/// derivada nem armazenada pelo cofre quando o chamador já a possui.
pub struct ChaveMestra(Zeroizing<[u8; TAMANHO_CHAVE]>);

impl std::fmt::Debug for ChaveMestra {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChaveMestra")
            .field("chave", &"<oculta>")
            .finish()
    }
}

impl ChaveMestra {
    /// Usa uma chave já provisionada pelo chamador (keyring, env, KMS local).
    #[must_use]
    pub fn de_bytes(bytes: [u8; TAMANHO_CHAVE]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    /// Gera uma KEK aleatória com o RNG do sistema.
    ///
    /// # Errors
    ///
    /// Retorna [`ErroCofre::Aleatorio`] se o sistema não fornecer entropia.
    pub fn gerar() -> Result<Self, ErroCofre> {
        Ok(Self(gerar_chave()?))
    }
}

impl ProvedorKek for ChaveMestra {
    fn selar(&self, texto: &[u8], aad: &[u8]) -> Result<Vec<u8>, ErroCofre> {
        selar_com_chave(&self.0, texto, aad)
    }

    fn abrir(&self, selado: &[u8], aad: &[u8]) -> Result<Zeroizing<Vec<u8>>, ErroCofre> {
        abrir_com_chave(&self.0, selado, aad)
    }
}

/// Gera 32 bytes aleatórios para uma DEK ou KEK.
pub(crate) fn gerar_chave() -> Result<Zeroizing<[u8; TAMANHO_CHAVE]>, ErroCofre> {
    let mut chave = Zeroizing::new([0_u8; TAMANHO_CHAVE]);
    getrandom::fill(&mut chave[..]).map_err(|erro| ErroCofre::Aleatorio(erro.to_string()))?;
    Ok(chave)
}

pub(crate) fn selar_com_chave(
    chave: &[u8; TAMANHO_CHAVE],
    texto: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, ErroCofre> {
    let cifra = Aes256Gcm::new_from_slice(&chave[..]).map_err(|_| ErroCofre::Cripto)?;
    let mut nonce = [0_u8; TAMANHO_NONCE];
    getrandom::fill(&mut nonce).map_err(|erro| ErroCofre::Aleatorio(erro.to_string()))?;
    let nonce_ref = AeadNonce::<Aes256Gcm>::try_from(&nonce[..]).map_err(|_| ErroCofre::Cripto)?;

    let mut saida = Vec::with_capacity(TAMANHO_NONCE + texto.len() + TAMANHO_TAG);
    saida.extend_from_slice(&nonce);
    saida.extend_from_slice(
        &cifra
            .encrypt(&nonce_ref, Payload { msg: texto, aad })
            .map_err(|_| ErroCofre::Cripto)?,
    );
    Ok(saida)
}

pub(crate) fn abrir_com_chave(
    chave: &[u8; TAMANHO_CHAVE],
    selado: &[u8],
    aad: &[u8],
) -> Result<Zeroizing<Vec<u8>>, ErroCofre> {
    if selado.len() < TAMANHO_MINIMO_SELADO {
        return Err(ErroCofre::RegistroAdulterado);
    }
    let cifra = Aes256Gcm::new_from_slice(&chave[..]).map_err(|_| ErroCofre::Cripto)?;
    let (nonce, resto) = selado.split_at(TAMANHO_NONCE);
    let nonce_ref =
        AeadNonce::<Aes256Gcm>::try_from(nonce).map_err(|_| ErroCofre::RegistroAdulterado)?;
    let claro = cifra
        .decrypt(&nonce_ref, Payload { msg: resto, aad })
        .map_err(|_| ErroCofre::ChaveIncorreta)?;
    Ok(Zeroizing::new(claro))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITOS: &[u8; 16] = b"0123456789abcdef";
    let mut saida = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        saida.push(char::from(DIGITOS[usize::from(byte >> 4)]));
        saida.push(char::from(DIGITOS[usize::from(byte & 0x0f)]));
    }
    saida
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sela_e_abre_com_o_mesmo_contexto() {
        let chave = ChaveMestra::de_bytes([7_u8; TAMANHO_CHAVE]);
        let selado = chave.selar(b"segredo", b"ctx").expect("sela");
        assert_ne!(selado, b"segredo");
        let aberto = chave.abrir(&selado, b"ctx").expect("abre");
        assert_eq!(&*aberto, b"segredo");
    }

    #[test]
    fn outro_contexto_nao_abre() {
        let chave = ChaveMestra::de_bytes([7_u8; TAMANHO_CHAVE]);
        let selado = chave.selar(b"segredo", b"ctx").expect("sela");
        assert!(matches!(
            chave.abrir(&selado, b"outro"),
            Err(ErroCofre::ChaveIncorreta)
        ));
    }

    #[test]
    fn chave_errada_nao_abre() {
        let chave = ChaveMestra::de_bytes([7_u8; TAMANHO_CHAVE]);
        let outra = ChaveMestra::de_bytes([8_u8; TAMANHO_CHAVE]);
        let selado = chave.selar(b"segredo", b"ctx").expect("sela");
        assert!(outra.abrir(&selado, b"ctx").is_err());
    }

    #[test]
    fn nonce_e_aleatorio_por_operacao() {
        let chave = ChaveMestra::de_bytes([7_u8; TAMANHO_CHAVE]);
        let primeiro = chave.selar(b"segredo", b"ctx").expect("sela");
        let segundo = chave.selar(b"segredo", b"ctx").expect("sela");
        assert_ne!(primeiro, segundo);
    }

    #[test]
    fn bloco_curto_e_recusado() {
        let chave = ChaveMestra::de_bytes([7_u8; TAMANHO_CHAVE]);
        assert!(matches!(
            chave.abrir(&[0_u8; 4], b"ctx"),
            Err(ErroCofre::RegistroAdulterado)
        ));
    }

    #[test]
    fn debug_nao_mostra_a_chave() {
        let chave = ChaveMestra::de_bytes([42_u8; TAMANHO_CHAVE]);
        let texto = format!("{chave:?}");
        assert!(texto.contains("oculta"));
        assert!(!texto.contains("42"));
    }

    #[test]
    fn hex_serializa_minusculo() {
        assert_eq!(hex(&[0x00, 0x0f, 0xab, 0xff]), "000fabff");
    }
}
