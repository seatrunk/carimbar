use crate::erro::ErroTipo;
use crate::modulo11;
use std::fmt;
use std::str::FromStr;

const CAMPO: &str = "chave de acesso";

/// Códigos IBGE de UF aceitos em `cUF`.
const UFS: [u8; 27] = [
    11, 12, 13, 14, 15, 16, 17, 21, 22, 23, 24, 25, 26, 27, 28, 29, 31, 32, 33, 35, 41, 42, 43, 50,
    51, 52, 53,
];

/// Modelo do documento na chave (posições 21-22).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ModeloDocumento {
    /// NF-e, modelo 55.
    NFe,
    /// NFC-e, modelo 65 (fora do MVP; reconhecida para leitura).
    NFCe,
}

impl ModeloDocumento {
    /// Código de 2 dígitos.
    pub fn codigo(self) -> &'static str {
        match self {
            Self::NFe => "55",
            Self::NFCe => "65",
        }
    }
}

/// Chave de acesso NF-e/NFC-e de 44 posições, com cDV validado.
///
/// Leiaute: cUF(2) AAMM(4) CNPJ/CPF(14) mod(2) serie(3) nNF(9) tpEmis(1) cNF(8) cDV(1).
/// As posições do emitente aceitam `[0-9A-Z]` (CNPJ alfanumérico); as demais só dígitos.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChaveAcesso([u8; 44]);

impl ChaveAcesso {
    /// Valida uma chave de 44 posições (espaços são ignorados).
    pub fn parse(entrada: &str) -> Result<Self, ErroTipo> {
        let limpo: Vec<u8> = entrada.bytes().filter(|b| *b != b' ').collect();
        let bytes: [u8; 44] = limpo.as_slice().try_into().map_err(|_| ErroTipo::Tamanho {
            campo: CAMPO,
            esperado: 44,
            recebido: limpo.len(),
        })?;
        for (i, &b) in bytes.iter().enumerate() {
            let ok = b.is_ascii_digit() || ((6..20).contains(&i) && b.is_ascii_uppercase());
            if !ok {
                return Err(ErroTipo::Caractere {
                    campo: CAMPO,
                    caractere: char::from(b),
                    posicao: i,
                });
            }
        }
        let chave = Self(bytes);
        chave.validar_campos()?;
        if bytes[43] != b'0' + modulo11::dv(&bytes[..43]) {
            return Err(ErroTipo::DigitoVerificador { campo: CAMPO });
        }
        Ok(chave)
    }

    /// Monta uma chave a partir das 43 primeiras posições, calculando o cDV.
    pub fn com_dv(sem_dv: &str) -> Result<Self, ErroTipo> {
        if sem_dv.len() != 43 {
            return Err(ErroTipo::Tamanho {
                campo: CAMPO,
                esperado: 43,
                recebido: sem_dv.len(),
            });
        }
        let dv = if sem_dv.bytes().all(|b| b.is_ascii_alphanumeric()) {
            modulo11::dv(sem_dv.as_bytes())
        } else {
            0
        };
        Self::parse(&format!("{sem_dv}{dv}"))
    }

    fn validar_campos(&self) -> Result<(), ErroTipo> {
        let n = |r: std::ops::Range<usize>| -> u32 {
            self.0[r]
                .iter()
                .fold(0, |acc, &b| acc * 10 + modulo11::valor(b))
        };
        let uf = n(0..2);
        if !UFS.iter().any(|&u| u32::from(u) == uf) {
            return Err(ErroTipo::Dominio {
                campo: "cUF",
                valor: uf.to_string(),
            });
        }
        let mes = n(4..6);
        if !(1..=12).contains(&mes) {
            return Err(ErroTipo::Dominio {
                campo: "AAMM",
                valor: self.campo(2..6).into(),
            });
        }
        if self.modelo().is_none() {
            return Err(ErroTipo::Dominio {
                campo: "mod",
                valor: self.campo(20..22).into(),
            });
        }
        Ok(())
    }

    fn campo(&self, r: std::ops::Range<usize>) -> &str {
        &self.as_str()[r]
    }

    /// As 44 posições.
    pub fn as_str(&self) -> &str {
        // Construído só com ASCII validado em `parse`.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }

    /// Código IBGE da UF (`cUF`).
    pub fn c_uf(&self) -> &str {
        self.campo(0..2)
    }
    /// Ano e mês de emissão (`AAMM`).
    pub fn aamm(&self) -> &str {
        self.campo(2..6)
    }
    /// CNPJ ou CPF (com zeros à esquerda) do emitente.
    pub fn emitente(&self) -> &str {
        self.campo(6..20)
    }
    /// Modelo do documento.
    pub fn modelo(&self) -> Option<ModeloDocumento> {
        match &self.0[20..22] {
            b"55" => Some(ModeloDocumento::NFe),
            b"65" => Some(ModeloDocumento::NFCe),
            _ => None,
        }
    }
    /// Série (3 posições).
    pub fn serie(&self) -> &str {
        self.campo(22..25)
    }
    /// Número do documento (9 posições).
    pub fn numero(&self) -> &str {
        self.campo(25..34)
    }
    /// Forma de emissão (`tpEmis`).
    pub fn tp_emis(&self) -> char {
        char::from(self.0[34])
    }
    /// Código numérico (`cNF`).
    pub fn c_nf(&self) -> &str {
        self.campo(35..43)
    }
    /// Dígito verificador (`cDV`).
    pub fn c_dv(&self) -> char {
        char::from(self.0[43])
    }
}

impl FromStr for ChaveAcesso {
    type Err = ErroTipo;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for ChaveAcesso {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for ChaveAcesso {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChaveAcesso({})", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Chaves sintéticas montadas aqui, com emitente sintético. Não são documentos reais.
    const SEM_DV: &str = "4326110000000100013655001000000123100000042";

    #[test]
    fn calcula_e_valida_dv() {
        let c = ChaveAcesso::com_dv(SEM_DV).expect("válida");
        assert_eq!(c.as_str().len(), 44);
        assert_eq!(ChaveAcesso::parse(c.as_str()), Ok(c));
        assert_eq!(c.c_uf(), "43");
        assert_eq!(c.aamm(), "2611");
        assert_eq!(c.emitente(), "00000001000136");
        assert_eq!(c.modelo(), Some(ModeloDocumento::NFe));
        assert_eq!(c.serie(), "001");
        assert_eq!(c.numero(), "000000123");
        assert_eq!(c.tp_emis(), '1');
        assert_eq!(c.c_nf(), "00000042");
    }

    #[test]
    fn dv_conhecido_calculado_a_mao() {
        // soma ponderada de SEM_DV = 287; 287 % 11 = 1; resto < 2, então cDV = 0.
        assert_eq!(ChaveAcesso::com_dv(SEM_DV).map(|c| c.c_dv()), Ok('0'));
    }

    #[test]
    fn rejeita_qualquer_dv_diferente() {
        let ok = ChaveAcesso::com_dv(SEM_DV).expect("válida");
        for d in b'0'..=b'9' {
            let s = format!("{SEM_DV}{}", char::from(d));
            assert_eq!(
                ChaveAcesso::parse(&s).is_ok(),
                char::from(d) == ok.c_dv(),
                "{s}"
            );
        }
    }

    #[test]
    fn aceita_emitente_alfanumerico() {
        let mut s = SEM_DV.to_owned();
        s.replace_range(6..20, "ABCDEFGH000195");
        let c = ChaveAcesso::com_dv(&s).expect("válida");
        assert_eq!(c.emitente(), "ABCDEFGH000195");
    }

    #[test]
    fn rejeita_campos_invalidos() {
        let troca = |pos: usize, v: &str| {
            let mut s = SEM_DV.to_owned();
            s.replace_range(pos..pos + v.len(), v);
            ChaveAcesso::com_dv(&s)
        };
        assert!(matches!(
            troca(0, "99"),
            Err(ErroTipo::Dominio { campo: "cUF", .. })
        ));
        assert!(matches!(
            troca(4, "13"),
            Err(ErroTipo::Dominio { campo: "AAMM", .. })
        ));
        assert!(matches!(
            troca(20, "57"),
            Err(ErroTipo::Dominio { campo: "mod", .. })
        ));
        assert!(matches!(
            troca(22, "A"),
            Err(ErroTipo::Caractere { posicao: 22, .. })
        ));
        assert!(matches!(
            ChaveAcesso::parse("123"),
            Err(ErroTipo::Tamanho { .. })
        ));
    }
}
