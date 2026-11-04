use crate::cnpj::Cnpj;
use crate::erro::ErroTipo;

/// Prova de que alguém pediu produção para um CNPJ específico.
///
/// Só existe via [`ConfirmacaoProducao::new`], com o CNPJ explícito. Não tem
/// `Default`, nem `Clone`: cada uso de produção precisa ser construído de propósito.
#[derive(Debug, PartialEq, Eq)]
pub struct ConfirmacaoProducao {
    cnpj: Cnpj,
}

impl ConfirmacaoProducao {
    /// Confirma produção para `cnpj`.
    pub fn new(cnpj: Cnpj) -> Self {
        Self { cnpj }
    }

    /// CNPJ confirmado.
    pub fn cnpj(&self) -> &Cnpj {
        &self.cnpj
    }
}

/// Ambiente de emissão. Sem `Default` de propósito: o chamador escolhe sempre.
///
/// `tpAmb` no XML e a URL do endpoint saem deste mesmo valor (fonte única).
///
/// ```compile_fail
/// let _a: carimbar_types::Ambiente = Default::default();
/// ```
///
/// ```compile_fail
/// // ConfirmacaoProducao não tem construtor sem CNPJ.
/// let _c = carimbar_types::ConfirmacaoProducao { cnpj: todo!() };
/// ```
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Ambiente {
    /// Homologação / produção restrita (`tpAmb = 2`).
    Homologacao,
    /// Produção (`tpAmb = 1`). Só construível com [`ConfirmacaoProducao`].
    Producao(ConfirmacaoProducao),
}

impl Ambiente {
    /// Produção para o CNPJ confirmado.
    pub fn producao(confirmacao: ConfirmacaoProducao) -> Self {
        Self::Producao(confirmacao)
    }

    /// Valor de `tpAmb` correspondente.
    pub fn tp_amb(&self) -> TpAmb {
        match self {
            Self::Homologacao => TpAmb::Homologacao,
            Self::Producao(_) => TpAmb::Producao,
        }
    }

    /// `true` se é produção.
    pub fn eh_producao(&self) -> bool {
        matches!(self, Self::Producao(_))
    }

    /// Garante que a confirmação de produção é do CNPJ emitente.
    pub fn exigir_emitente(&self, emitente: &Cnpj) -> Result<(), ErroTipo> {
        match self {
            Self::Producao(c) if c.cnpj() != emitente => Err(ErroTipo::Dominio {
                campo: "ConfirmacaoProducao",
                valor: format!("confirmada para {}, emitente {}", c.cnpj(), emitente),
            }),
            _ => Ok(()),
        }
    }
}

/// Código `tpAmb` lido ou escrito no XML. Não autoriza nada: para emitir use [`Ambiente`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TpAmb {
    /// `1`
    Producao,
    /// `2`
    Homologacao,
}

impl TpAmb {
    /// Código no XML.
    pub fn codigo(self) -> char {
        match self {
            Self::Producao => '1',
            Self::Homologacao => '2',
        }
    }

    /// Lê o código do XML.
    pub fn from_codigo(c: &str) -> Result<Self, ErroTipo> {
        match c {
            "1" => Ok(Self::Producao),
            "2" => Ok(Self::Homologacao),
            _ => Err(ErroTipo::Dominio {
                campo: "tpAmb",
                valor: c.into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cnpj(s: &str) -> Cnpj {
        Cnpj::parse(s).expect("CNPJ sintético válido")
    }

    #[test]
    fn homologacao_e_tpamb_2() {
        assert_eq!(Ambiente::Homologacao.tp_amb().codigo(), '2');
        assert!(!Ambiente::Homologacao.eh_producao());
    }

    #[test]
    fn producao_exige_confirmacao_do_emitente() {
        let emitente = cnpj("00000001000136");
        let outro = cnpj("12345678000195");
        let amb = Ambiente::producao(ConfirmacaoProducao::new(emitente));
        assert_eq!(amb.tp_amb(), TpAmb::Producao);
        assert!(amb.exigir_emitente(&emitente).is_ok());
        assert!(amb.exigir_emitente(&outro).is_err());
        assert!(Ambiente::Homologacao.exigir_emitente(&outro).is_ok());
    }

    #[test]
    fn tpamb_roundtrip() {
        for t in [TpAmb::Producao, TpAmb::Homologacao] {
            assert_eq!(TpAmb::from_codigo(&t.codigo().to_string()), Ok(t));
        }
        assert!(TpAmb::from_codigo("3").is_err());
    }
}
