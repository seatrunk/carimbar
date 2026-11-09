//! Identificadores fiscais da NFS-e Nacional: município, inscrição federal,
//! Id da DPS e chave de acesso.
//!
//! As regras mudam entre produção e produção restrita. O contrato completo
//! está em `docs/contratos-fiscais.md`, seção 2.2. Resumindo:
//!
//! - produção: `DPS` + 42 dígitos e chave com 50 dígitos;
//! - produção restrita: `DPS` + município (7) + tipo (1) + inscrição (14) +
//!   série (5) + número (15), aceitando CNPJ alfanumérico na inscrição.

use crate::erro::ErroNfse;
use carimbar_types::{Cnpj, TpAmb};
use std::fmt;

/// Código de município do IBGE, com 7 dígitos.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CodigoMunicipio(String);

impl CodigoMunicipio {
    /// Valida o código de 7 dígitos.
    pub fn new(valor: impl Into<String>) -> Result<Self, ErroNfse> {
        let valor = valor.into();
        if valor.len() != 7 || !valor.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ErroNfse::campo(
                "cMun",
                format!("esperado código IBGE de 7 dígitos: {valor:?}"),
            ));
        }
        Ok(Self(valor))
    }

    /// Texto do código.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CodigoMunicipio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Tipo de inscrição federal usado no Id da DPS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoInscricao {
    /// CNPJ, código `1`.
    Cnpj,
    /// CPF, código `2`.
    Cpf,
}

impl TipoInscricao {
    /// Código no Id e no XML.
    pub fn codigo(self) -> char {
        match self {
            Self::Cnpj => '1',
            Self::Cpf => '2',
        }
    }
}

/// Inscrição federal do prestador, tomador, intermediário ou autor do evento.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InscricaoFederal {
    /// CNPJ numérico ou alfanumérico com DV válido.
    Cnpj(Cnpj),
    /// CPF com 11 dígitos e DV válido.
    Cpf(String),
}

impl InscricaoFederal {
    /// Lê CNPJ (14 posições) ou CPF (11 dígitos), removendo máscara.
    pub fn parse(entrada: &str) -> Result<Self, ErroNfse> {
        let limpo: String = entrada
            .trim()
            .chars()
            .filter(|c| !matches!(c, '.' | '/' | '-'))
            .collect();
        match limpo.len() {
            14 => Cnpj::parse(&limpo)
                .map(Self::Cnpj)
                .map_err(|erro| ErroNfse::campo("inscrição federal", erro.to_string())),
            11 => {
                if !cpf_valido(&limpo) {
                    return Err(ErroNfse::campo(
                        "inscrição federal",
                        format!("CPF inválido: {limpo:?}"),
                    ));
                }
                Ok(Self::Cpf(limpo))
            }
            _ => Err(ErroNfse::campo(
                "inscrição federal",
                format!("esperado CNPJ (14) ou CPF (11): {entrada:?}"),
            )),
        }
    }

    /// Tipo da inscrição.
    pub fn tipo(&self) -> TipoInscricao {
        match self {
            Self::Cnpj(_) => TipoInscricao::Cnpj,
            Self::Cpf(_) => TipoInscricao::Cpf,
        }
    }

    /// CNPJ, se houver.
    pub fn cnpj(&self) -> Option<&Cnpj> {
        match self {
            Self::Cnpj(cnpj) => Some(cnpj),
            Self::Cpf(_) => None,
        }
    }

    /// Texto com 14 posições; CPF é completado com três zeros à esquerda.
    pub fn para_id(&self) -> String {
        match self {
            Self::Cnpj(cnpj) => cnpj.as_str().to_owned(),
            Self::Cpf(cpf) => format!("000{cpf}"),
        }
    }

    /// Texto do CNPJ ou CPF sem máscara.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Cnpj(cnpj) => cnpj.as_str(),
            Self::Cpf(cpf) => cpf,
        }
    }

    /// `true` quando a inscrição contém letras.
    pub fn alfanumerica(&self) -> bool {
        self.as_str().bytes().any(|b| b.is_ascii_alphabetic())
    }
}

impl fmt::Display for InscricaoFederal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Série da DPS, com até 5 posições e teto de `89999`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SerieDps(u32);

impl SerieDps {
    /// Maior série aceita pelo schema (`TSSerieDPS`).
    pub const MAXIMA: u32 = 89_999;

    /// Valida a série.
    pub fn new(valor: u32) -> Result<Self, ErroNfse> {
        if valor > Self::MAXIMA {
            return Err(ErroNfse::campo(
                "serie",
                format!("série acima de {}: {valor}", Self::MAXIMA),
            ));
        }
        Ok(Self(valor))
    }

    /// Valor numérico.
    pub fn get(self) -> u32 {
        self.0
    }

    /// Texto sem zeros à esquerda, como no XML.
    pub fn para_xml(self) -> String {
        self.0.to_string()
    }

    /// Texto com 5 posições, como no Id.
    pub fn para_id(self) -> String {
        format!("{:05}", self.0)
    }
}

/// Número da DPS, de 1 a 999.999.999.999.999.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NumeroDps(u64);

impl NumeroDps {
    /// Maior número aceito por `TSNumDPS`.
    pub const MAXIMO: u64 = 999_999_999_999_999;

    /// Valida o número.
    pub fn new(valor: u64) -> Result<Self, ErroNfse> {
        if !(1..=Self::MAXIMO).contains(&valor) {
            return Err(ErroNfse::campo(
                "nDPS",
                format!("número fora de 1..={}: {valor}", Self::MAXIMO),
            ));
        }
        Ok(Self(valor))
    }

    /// Valor numérico.
    pub fn get(self) -> u64 {
        self.0
    }

    /// Texto sem zeros à esquerda.
    pub fn para_xml(self) -> String {
        self.0.to_string()
    }

    /// Texto com 15 posições, como no Id.
    pub fn para_id(self) -> String {
        format!("{:015}", self.0)
    }
}

/// Identificador da DPS (`Id` de `infDPS`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdDps(String);

impl IdDps {
    /// Gera o Id a partir dos campos que o compõem.
    ///
    /// Em produção, CNPJ alfanumérico é recusado porque o padrão publicado
    /// aceita apenas dígitos.
    pub fn gerar(
        municipio: &CodigoMunicipio,
        inscricao: &InscricaoFederal,
        serie: SerieDps,
        numero: NumeroDps,
        ambiente: TpAmb,
    ) -> Result<Self, ErroNfse> {
        if ambiente == TpAmb::Producao && inscricao.alfanumerica() {
            return Err(ErroNfse::campo(
                "IdDPS",
                "CNPJ alfanumérico não é aceito na produção",
            ));
        }
        let valor = format!(
            "DPS{}{}{}{}{}",
            municipio,
            inscricao.tipo().codigo(),
            inscricao.para_id(),
            serie.para_id(),
            numero.para_id()
        );
        Self::parse(&valor, ambiente)
    }

    /// Valida um Id já formado para o ambiente.
    pub fn parse(entrada: &str, ambiente: TpAmb) -> Result<Self, ErroNfse> {
        let corpo = entrada
            .strip_prefix("DPS")
            .ok_or_else(|| ErroNfse::campo("IdDPS", "deve começar com DPS"))?;
        if entrada.len() != 45 || corpo.len() != 42 {
            return Err(ErroNfse::campo(
                "IdDPS",
                format!("esperado DPS + 42 posições: {entrada:?}"),
            ));
        }
        let bytes = corpo.as_bytes();
        if !bytes[0..7].iter().all(u8::is_ascii_digit) {
            return Err(ErroNfse::campo("IdDPS", "município deve ter 7 dígitos"));
        }
        if !matches!(bytes[7], b'1' | b'2') {
            return Err(ErroNfse::campo(
                "IdDPS",
                "tipo de inscrição deve ser 1 ou 2",
            ));
        }
        let inscricao = &corpo[8..22];
        let inscricao_ok = if ambiente == TpAmb::Producao {
            inscricao.bytes().all(|b| b.is_ascii_digit())
        } else {
            inscricao
                .bytes()
                .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
        };
        if !inscricao_ok {
            return Err(ErroNfse::campo(
                "IdDPS",
                "inscrição federal inválida para o ambiente",
            ));
        }
        if !corpo[22..42].bytes().all(|b| b.is_ascii_digit()) {
            return Err(ErroNfse::campo("IdDPS", "série e número devem ser dígitos"));
        }
        Ok(Self(entrada.to_owned()))
    }

    /// Texto do Id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdDps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Chave de acesso da NFS-e, com 50 posições.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChaveNfse(String);

impl ChaveNfse {
    /// Valida a chave conforme o ambiente.
    ///
    /// Produção aceita somente dígitos. Produção restrita aceita letras
    /// maiúsculas na posição da inscrição federal.
    pub fn parse(entrada: &str, ambiente: TpAmb) -> Result<Self, ErroNfse> {
        let valor = entrada.trim();
        if valor.len() != 50 {
            return Err(ErroNfse::campo(
                "chave NFS-e",
                format!("esperado 50 posições: {valor:?}"),
            ));
        }
        let bytes = valor.as_bytes();
        let alfanumerica = bytes
            .iter()
            .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase());
        let valida = if ambiente == TpAmb::Producao {
            alfanumerica && bytes.iter().all(u8::is_ascii_digit)
        } else {
            alfanumerica
                && bytes[0..6].iter().all(u8::is_ascii_digit)
                && bytes[20..50].iter().all(u8::is_ascii_digit)
        };
        if !valida {
            return Err(ErroNfse::campo(
                "chave NFS-e",
                format!("formato inválido para o ambiente: {valor:?}"),
            ));
        }
        Ok(Self(valor.to_owned()))
    }

    /// Texto da chave.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ChaveNfse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

fn cpf_valido(cpf: &str) -> bool {
    if cpf.len() != 11 || !cpf.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if cpf.bytes().all(|b| b == cpf.as_bytes()[0]) {
        return false;
    }
    let digitos: Vec<u32> = cpf.bytes().map(|b| u32::from(b - b'0')).collect();
    for tamanho in [9_usize, 10] {
        let mut soma = 0_u32;
        for (indice, digito) in digitos[..tamanho].iter().enumerate() {
            let peso = u32::try_from(tamanho + 1 - indice).expect("peso cabe em u32");
            soma += digito * peso;
        }
        let resto = soma % 11;
        let esperado = if resto < 2 { 0 } else { 11 - resto };
        if digitos[tamanho] != esperado {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn municipio() -> CodigoMunicipio {
        CodigoMunicipio::new("3550308").expect("município")
    }

    fn cnpj() -> InscricaoFederal {
        InscricaoFederal::parse("00000001000136").expect("CNPJ sintético")
    }

    #[test]
    fn gera_id_de_homologacao_e_producao() {
        let id = IdDps::gerar(
            &municipio(),
            &cnpj(),
            SerieDps::new(1).expect("série"),
            NumeroDps::new(1).expect("número"),
            TpAmb::Homologacao,
        )
        .expect("Id");
        assert_eq!(id.as_str(), "DPS355030810000000100013600001000000000000001");
        assert!(IdDps::parse(id.as_str(), TpAmb::Producao).is_ok());
    }

    #[test]
    fn cnpj_alfanumerico_so_vale_na_producao_restrita() {
        let cnpj = InscricaoFederal::parse("00000001000A40").expect("CNPJ alfanumérico");
        let id = IdDps::gerar(
            &municipio(),
            &cnpj,
            SerieDps::new(1).expect("série"),
            NumeroDps::new(1).expect("número"),
            TpAmb::Homologacao,
        )
        .expect("Id restrito");
        assert!(id.as_str().contains("00000001000A40"));
        assert!(
            IdDps::gerar(
                &municipio(),
                &cnpj,
                SerieDps::new(1).expect("série"),
                NumeroDps::new(1).expect("número"),
                TpAmb::Producao,
            )
            .is_err()
        );
    }

    #[test]
    fn chave_segue_o_ambiente() {
        let chave = format!("{}{}{}", "355030", "00000001000136", "0".repeat(30));
        assert_eq!(chave.len(), 50);
        assert!(ChaveNfse::parse(&chave, TpAmb::Producao).is_ok());
        let alfanumerica = chave.replace("00000001000136", "00000001000A40");
        assert_eq!(alfanumerica.len(), 50);
        assert!(ChaveNfse::parse(&alfanumerica, TpAmb::Producao).is_err());
    }

    #[test]
    fn cpf_tem_dv_validado() {
        assert!(InscricaoFederal::parse("52998224725").is_ok());
        assert!(InscricaoFederal::parse("52998224724").is_err());
        assert!(InscricaoFederal::parse("11111111111").is_err());
    }
}
