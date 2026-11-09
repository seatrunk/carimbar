//! Derivação da KEK a partir de senha (Argon2id) e sal público.

use std::fmt;

use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

use crate::vault::crypto::{ChaveMestra, TAMANHO_CHAVE, hex};
use crate::vault::erro::ErroCofre;

/// Tamanho do sal, em bytes.
pub const TAMANHO_SAL: usize = 16;

/// Sal público usado na derivação da KEK.
///
/// O sal não é segredo: fica gravado no cofre para permitir reabrir com a mesma
/// senha. Ele não precisa ser ocultado em `Debug`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Sal([u8; TAMANHO_SAL]);

impl fmt::Debug for Sal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Sal").field(&hex(&self.0)).finish()
    }
}

impl Sal {
    /// Gera um sal aleatório com o RNG do sistema.
    ///
    /// # Errors
    ///
    /// Retorna [`ErroCofre::Aleatorio`] se o sistema não fornecer entropia.
    pub fn novo() -> Result<Self, ErroCofre> {
        let mut bytes = [0_u8; TAMANHO_SAL];
        getrandom::fill(&mut bytes).map_err(|erro| ErroCofre::Aleatorio(erro.to_string()))?;
        Ok(Self(bytes))
    }

    /// Reconstrói um sal já gravado.
    #[must_use]
    pub fn de_bytes(bytes: [u8; TAMANHO_SAL]) -> Self {
        Self(bytes)
    }

    /// Bytes do sal.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Custo do Argon2id usado ao derivar a KEK de uma senha.
///
/// O padrão segue a recomendação prática do OWASP (19 MiB, 2 iterações,
/// paralelismo 1). Cofres criados guardam o custo usado e o reaplicam ao abrir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustoKdf {
    /// Memória em KiB.
    pub memoria_kib: u32,
    /// Número de iterações.
    pub iteracoes: u32,
    /// Grau de paralelismo.
    pub paralelismo: u32,
}

impl Default for CustoKdf {
    fn default() -> Self {
        Self {
            memoria_kib: Params::DEFAULT_M_COST,
            iteracoes: Params::DEFAULT_T_COST,
            paralelismo: Params::DEFAULT_P_COST,
        }
    }
}

/// Limite de memória aceito ao reabrir um cofre (1 GiB).
const MEMORIA_MAXIMA_KIB: u32 = 1024 * 1024;

impl CustoKdf {
    /// Custo reduzido para testes e máquinas muito limitadas.
    ///
    /// Não use em produção: 4 MiB é insuficiente contra ataque de dicionário.
    #[must_use]
    pub fn rapido() -> Self {
        Self {
            memoria_kib: 4096,
            iteracoes: 1,
            paralelismo: 1,
        }
    }

    fn parametros(self) -> Result<Params, ErroCofre> {
        if self.paralelismo == 0 {
            return Err(ErroCofre::Parametro(
                "paralelismo do Argon2 deve ser pelo menos 1".to_owned(),
            ));
        }
        if self.iteracoes == 0 {
            return Err(ErroCofre::Parametro(
                "iterações do Argon2 devem ser pelo menos 1".to_owned(),
            ));
        }
        if self.memoria_kib > MEMORIA_MAXIMA_KIB {
            return Err(ErroCofre::Parametro(
                "memória do Argon2 acima do limite de 1 GiB".to_owned(),
            ));
        }
        Params::new(
            self.memoria_kib,
            self.iteracoes,
            self.paralelismo,
            Some(TAMANHO_CHAVE),
        )
        .map_err(|erro| ErroCofre::Parametro(format!("parâmetros do Argon2: {erro}")))
    }
}

impl ChaveMestra {
    /// Deriva uma KEK de `senha` com Argon2id, usando o sal e o custo informados.
    ///
    /// # Errors
    ///
    /// Retorna [`ErroCofre::Parametro`] para custos fora dos limites aceitos.
    pub fn derivar(senha: &str, sal: &Sal, custo: CustoKdf) -> Result<Self, ErroCofre> {
        let params = custo.parametros()?;
        let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut bytes = Zeroizing::new([0_u8; TAMANHO_CHAVE]);
        argon
            .hash_password_into(senha.as_bytes(), sal.as_bytes(), &mut bytes[..])
            .map_err(|erro| ErroCofre::Parametro(format!("derivação Argon2id: {erro}")))?;
        Ok(Self::de_bytes(*bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::crypto::ProvedorKek;

    #[test]
    fn mesma_senha_e_sal_geram_a_mesma_chave() {
        let sal = Sal::de_bytes([1_u8; TAMANHO_SAL]);
        let custo = CustoKdf::rapido();
        let primeira = ChaveMestra::derivar("senha", &sal, custo).expect("deriva");
        let segunda = ChaveMestra::derivar("senha", &sal, custo).expect("deriva");
        let selado = primeira.selar(b"x", b"").expect("sela");
        assert_eq!(segunda.abrir(&selado, b"").expect("abre").as_slice(), b"x");
        assert_ne!(segunda.selar(b"x", b"").expect("sela"), selado);
    }

    #[test]
    fn sal_diferente_muda_a_chave() {
        let custo = CustoKdf::rapido();
        let primeira = ChaveMestra::derivar("senha", &Sal::de_bytes([1; 16]), custo).expect("1");
        let segunda = ChaveMestra::derivar("senha", &Sal::de_bytes([2; 16]), custo).expect("2");
        let selado = primeira.selar(b"x", b"").expect("sela");
        assert!(segunda.abrir(&selado, b"").is_err());
    }

    #[test]
    fn custo_invalido_e_recusado() {
        let sal = Sal::de_bytes([1_u8; TAMANHO_SAL]);
        assert!(
            ChaveMestra::derivar(
                "senha",
                &sal,
                CustoKdf {
                    memoria_kib: 1024 * 1024 + 1,
                    ..CustoKdf::rapido()
                }
            )
            .is_err()
        );
        assert!(
            ChaveMestra::derivar(
                "senha",
                &sal,
                CustoKdf {
                    iteracoes: 0,
                    ..CustoKdf::rapido()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn sal_novo_tem_16_bytes() {
        let sal = Sal::novo().expect("sal");
        assert_eq!(sal.as_bytes().len(), TAMANHO_SAL);
    }
}
