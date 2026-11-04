use crate::erro::ErroTipo;
use rust_decimal::Decimal;
use std::fmt;
use std::str::FromStr;

/// Valor monetário em reais: `Decimal`, não negativo, até 2 casas.
///
/// Não há conversão de `f64`. Construa a partir de texto (`"10.50"`) ou de
/// centavos inteiros. A serialização sempre sai com 2 casas (formato TDec_1302).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Dinheiro(Decimal);

impl Dinheiro {
    /// Zero.
    pub const ZERO: Self = Self(Decimal::ZERO);

    /// Valida um `Decimal` existente.
    pub fn new(valor: Decimal) -> Result<Self, ErroTipo> {
        if valor.is_sign_negative() && !valor.is_zero() {
            return Err(ErroTipo::Dinheiro(format!("negativo: {valor}")));
        }
        if valor.normalize().scale() > 2 {
            return Err(ErroTipo::Dinheiro(format!("mais de 2 casas: {valor}")));
        }
        Ok(Self(valor.round_dp(2)))
    }

    /// A partir de centavos.
    pub fn from_centavos(centavos: u64) -> Self {
        Self(Decimal::from(centavos) / Decimal::ONE_HUNDRED).normalizado()
    }

    fn normalizado(mut self) -> Self {
        self.0.rescale(2);
        self
    }

    /// Valor interno.
    pub fn valor(&self) -> Decimal {
        self.0
    }

    /// Soma sem estouro.
    pub fn checked_add(self, outro: Self) -> Option<Self> {
        self.0.checked_add(outro.0).map(|v| Self(v).normalizado())
    }

    /// Texto para o XML: ponto decimal e exatamente 2 casas.
    pub fn para_xml(&self) -> String {
        format!("{:.2}", self.0)
    }
}

impl FromStr for Dinheiro {
    type Err = ErroTipo;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            return Err(ErroTipo::Dinheiro(format!("formato: {s:?}")));
        }
        let d = Decimal::from_str_exact(s).map_err(|e| ErroTipo::Dinheiro(e.to_string()))?;
        Self::new(d).map(Self::normalizado)
    }
}

impl fmt::Display for Dinheiro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.para_xml())
    }
}

impl fmt::Debug for Dinheiro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Dinheiro({})", self.para_xml())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Dinheiro {
        s.parse().expect("valor válido")
    }

    #[test]
    fn soma_exata_sem_erro_de_ponto_flutuante() {
        // Em f64, 0.1 + 0.2 != 0.3.
        assert_eq!(d("0.10").checked_add(d("0.20")), Some(d("0.30")));
        assert_eq!(d("0.1").para_xml(), "0.10");
    }

    #[test]
    fn serializa_com_duas_casas() {
        assert_eq!(d("10").para_xml(), "10.00");
        assert_eq!(d("1234.5").to_string(), "1234.50");
        assert_eq!(Dinheiro::from_centavos(199).para_xml(), "1.99");
        assert_eq!(Dinheiro::ZERO.para_xml(), "0.00");
    }

    #[test]
    fn rejeita_formatos_invalidos() {
        for s in ["", "-1.00", "1,00", "1.001", "1e3", "abc", "1.2.3"] {
            assert!(s.parse::<Dinheiro>().is_err(), "{s:?}");
        }
        assert!(Dinheiro::new(Decimal::new(-1, 0)).is_err());
        assert!(
            d("1.100").para_xml() == "1.10",
            "zeros à direita além de 2 casas são aceitos"
        );
    }
}
