use crate::endpoints::erro::ErroEndpoints;

macro_rules! ufs {
    ($($variante:ident = ($sigla:literal, $codigo:literal, $nome:literal)),+ $(,)?) => {
        /// Unidade da Federação, com sigla e código IBGE.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum Uf {
            $(
                #[doc = $nome]
                $variante,
            )+
        }

        impl Uf {
            /// Todas as 27 unidades da Federação.
            pub const TODAS: &'static [Uf] = &[$(Uf::$variante),+];

            /// Sigla de duas letras.
            pub fn sigla(self) -> &'static str {
                match self {
                    $(Uf::$variante => $sigla),+
                }
            }

            /// Código IBGE de duas posições.
            pub fn codigo(self) -> &'static str {
                match self {
                    $(Uf::$variante => $codigo),+
                }
            }

            /// Nome por extenso.
            pub fn nome(self) -> &'static str {
                match self {
                    $(Uf::$variante => $nome),+
                }
            }
        }
    };
}

ufs! {
    Acre = ("AC", "12", "Acre"),
    Alagoas = ("AL", "27", "Alagoas"),
    Amapa = ("AP", "16", "Amapá"),
    Amazonas = ("AM", "13", "Amazonas"),
    Bahia = ("BA", "29", "Bahia"),
    Ceara = ("CE", "23", "Ceará"),
    DistritoFederal = ("DF", "53", "Distrito Federal"),
    EspiritoSanto = ("ES", "32", "Espírito Santo"),
    Goias = ("GO", "52", "Goiás"),
    Maranhao = ("MA", "21", "Maranhão"),
    MatoGrosso = ("MT", "51", "Mato Grosso"),
    MatoGrossoDoSul = ("MS", "50", "Mato Grosso do Sul"),
    MinasGerais = ("MG", "31", "Minas Gerais"),
    Para = ("PA", "15", "Pará"),
    Paraiba = ("PB", "25", "Paraíba"),
    Parana = ("PR", "41", "Paraná"),
    Pernambuco = ("PE", "26", "Pernambuco"),
    Piaui = ("PI", "22", "Piauí"),
    RioDeJaneiro = ("RJ", "33", "Rio de Janeiro"),
    RioGrandeDoNorte = ("RN", "24", "Rio Grande do Norte"),
    RioGrandeDoSul = ("RS", "43", "Rio Grande do Sul"),
    Rondonia = ("RO", "11", "Rondônia"),
    Roraima = ("RR", "14", "Roraima"),
    SantaCatarina = ("SC", "42", "Santa Catarina"),
    SaoPaulo = ("SP", "35", "São Paulo"),
    Sergipe = ("SE", "28", "Sergipe"),
    Tocantins = ("TO", "17", "Tocantins"),
}

impl Uf {
    /// Lê a sigla, sem diferenciar maiúsculas de minúsculas.
    pub fn from_sigla(sigla: &str) -> Result<Self, ErroEndpoints> {
        let sigla = sigla.trim();
        Self::TODAS
            .iter()
            .copied()
            .find(|uf| uf.sigla().eq_ignore_ascii_case(sigla))
            .ok_or_else(|| ErroEndpoints::Uf(sigla.to_owned()))
    }

    /// Lê o código IBGE de duas posições.
    pub fn from_codigo(codigo: &str) -> Result<Self, ErroEndpoints> {
        Self::TODAS
            .iter()
            .copied()
            .find(|uf| uf.codigo() == codigo)
            .ok_or_else(|| ErroEndpoints::Uf(codigo.to_owned()))
    }
}

impl std::str::FromStr for Uf {
    type Err = ErroEndpoints;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_sigla(s)
    }
}

impl std::fmt::Display for Uf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.sigla())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cobre_as_27_ufs_sem_duplicata() {
        assert_eq!(Uf::TODAS.len(), 27);
        let mut siglas: Vec<_> = Uf::TODAS.iter().map(|u| u.sigla()).collect();
        siglas.sort_unstable();
        siglas.dedup();
        assert_eq!(siglas.len(), 27);
        let mut codigos: Vec<_> = Uf::TODAS.iter().map(|u| u.codigo()).collect();
        codigos.sort_unstable();
        codigos.dedup();
        assert_eq!(codigos.len(), 27);
    }

    #[test]
    fn le_sigla_e_codigo() {
        assert_eq!(Uf::from_sigla("sp"), Ok(Uf::SaoPaulo));
        assert_eq!(Uf::from_codigo("35"), Ok(Uf::SaoPaulo));
        assert_eq!(Uf::SaoPaulo.sigla(), "SP");
        assert_eq!(Uf::SaoPaulo.codigo(), "35");
        assert!(Uf::from_sigla("XX").is_err());
        assert!(Uf::from_codigo("99").is_err());
    }
}
