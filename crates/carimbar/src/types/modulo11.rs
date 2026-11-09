//! Módulo 11 com pesos 2..=9 da direita para a esquerda, como usado no CNPJ
//! e no cDV da chave de acesso. O valor de cada posição é `ascii - 48`, o que
//! cobre dígitos e as letras do CNPJ alfanumérico (A=17 ... Z=42).

pub(crate) fn valor(c: u8) -> u32 {
    u32::from(c) - 48
}

/// Resto 0 ou 1 → 0; senão 11 - resto.
pub(crate) fn dv(posicoes: &[u8]) -> u8 {
    let soma: u32 = posicoes
        .iter()
        .rev()
        .zip((2..=9).cycle())
        .map(|(&c, peso)| valor(c) * peso)
        .sum();
    let resto = soma % 11;
    if resto < 2 { 0 } else { (11 - resto) as u8 }
}
