use crate::erro::ErroTipo;
use crate::modulo11;
use std::fmt;
use std::str::FromStr;

const CAMPO: &str = "CNPJ";

/// CNPJ validado: 12 posições `[0-9A-Z]` + 2 DV numéricos.
///
/// Aceita o formato alfanumérico (IN RFB 2.229/2024, em vigor desde 07/2026).
/// A máscara (`.`, `/`, `-`) é removida; letras minúsculas são rejeitadas.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cnpj([u8; 14]);

impl Cnpj {
    /// Valida e normaliza um CNPJ, com ou sem máscara.
    pub fn parse(entrada: &str) -> Result<Self, ErroTipo> {
        let limpo: Vec<u8> = entrada
            .bytes()
            .filter(|b| !matches!(b, b'.' | b'/' | b'-'))
            .collect();
        let bytes: [u8; 14] = limpo.as_slice().try_into().map_err(|_| ErroTipo::Tamanho {
            campo: CAMPO,
            esperado: 14,
            recebido: limpo.len(),
        })?;
        for (i, &b) in bytes.iter().enumerate() {
            let ok = if i < 12 {
                b.is_ascii_digit() || b.is_ascii_uppercase()
            } else {
                b.is_ascii_digit()
            };
            if !ok {
                return Err(ErroTipo::Caractere {
                    campo: CAMPO,
                    caractere: char::from(b),
                    posicao: i,
                });
            }
        }
        if bytes.iter().all(|&b| b == bytes[0]) {
            return Err(ErroTipo::Repetido { campo: CAMPO });
        }
        let (d1, d2) = Self::calcular_dv(&bytes[..12]);
        if bytes[12] != b'0' + d1 || bytes[13] != b'0' + d2 {
            return Err(ErroTipo::DigitoVerificador { campo: CAMPO });
        }
        Ok(Self(bytes))
    }

    /// Calcula os 2 DV para uma base de 12 posições já validada.
    fn calcular_dv(base: &[u8]) -> (u8, u8) {
        let d1 = modulo11::dv(base);
        let mut com_d1 = base.to_vec();
        com_d1.push(b'0' + d1);
        (d1, modulo11::dv(&com_d1))
    }

    /// As 14 posições sem máscara.
    pub fn as_str(&self) -> &str {
        // Construído só com ASCII validado em `parse`.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }

    /// Raiz (8 primeiras posições), usada para casar matriz e filial com o certificado.
    pub fn raiz(&self) -> &str {
        &self.as_str()[..8]
    }

    /// `true` se alguma posição da base é letra (CNPJ alfanumérico).
    pub fn alfanumerico(&self) -> bool {
        self.0[..12].iter().any(u8::is_ascii_uppercase)
    }

    /// Formato com máscara `XX.XXX.XXX/XXXX-XX`.
    pub fn formatado(&self) -> String {
        let s = self.as_str();
        format!(
            "{}.{}.{}/{}-{}",
            &s[..2],
            &s[2..5],
            &s[5..8],
            &s[8..12],
            &s[12..]
        )
    }
}

impl FromStr for Cnpj {
    type Err = ErroTipo;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for Cnpj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Cnpj {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cnpj({})", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // CNPJs sintéticos: bases arbitrárias com DV calculado. Não são empresas reais.
    fn sintetico(base: &str) -> String {
        let (d1, d2) = Cnpj::calcular_dv(base.as_bytes());
        format!("{base}{d1}{d2}")
    }

    #[test]
    fn aceita_numerico_com_e_sem_mascara() {
        let c = Cnpj::parse("00.000.001/0001-36").expect("válido");
        assert_eq!(c.as_str(), "00000001000136");
        assert_eq!(c.formatado(), "00.000.001/0001-36");
        assert_eq!(c.raiz(), "00000001");
        assert_eq!(Cnpj::parse("00000001000136"), Ok(c));
        assert!(!c.alfanumerico());
    }

    #[test]
    fn aceita_alfanumerico_exemplo_rfb() {
        // Exemplo publicado pela RFB para o CNPJ alfanumérico.
        let c = Cnpj::parse("12.ABC.345/01DE-35").expect("válido");
        assert!(c.alfanumerico());
        assert_eq!(c.as_str(), "12ABC34501DE35");
    }

    #[test]
    fn rejeita_dv_errado() {
        assert_eq!(
            Cnpj::parse("00000001000137"),
            Err(ErroTipo::DigitoVerificador { campo: "CNPJ" })
        );
    }

    #[test]
    fn rejeita_tamanho_caractere_e_repetido() {
        assert!(matches!(
            Cnpj::parse("0000000100013"),
            Err(ErroTipo::Tamanho { .. })
        ));
        assert!(matches!(
            Cnpj::parse("12abc34501de35"),
            Err(ErroTipo::Caractere { .. })
        ));
        assert!(matches!(
            Cnpj::parse("12ABC34501DEA5"),
            Err(ErroTipo::Caractere { posicao: 12, .. })
        ));
        assert_eq!(
            Cnpj::parse("00000000000000"),
            Err(ErroTipo::Repetido { campo: "CNPJ" })
        );
    }

    #[test]
    fn todo_dv_calculado_e_aceito_e_vizinhos_rejeitados() {
        for base in [
            "000000010001",
            "987654320001",
            "ZZZZZZZZ0001",
            "A1B2C3D4E5F6",
        ] {
            let ok = sintetico(base);
            assert!(Cnpj::parse(&ok).is_ok(), "{ok}");
            let ultimo = ok.as_bytes()[13];
            let errado = format!(
                "{}{}",
                &ok[..13],
                char::from(b'0' + (ultimo - b'0' + 1) % 10)
            );
            assert!(Cnpj::parse(&errado).is_err(), "{errado}");
        }
    }
}
