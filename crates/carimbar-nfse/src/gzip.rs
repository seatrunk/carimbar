//! Compressão GZip e base64 usadas pelas APIs JSON da Sefin Nacional.
//!
//! O XML fiscal vai em `dpsXmlGZipB64`/`pedidoRegistroEventoXmlGZipB64` e volta
//! em `nfseXmlGZipB64`/`eventoXmlGZipB64`. A descompressão limita o tamanho
//! descompactado para não permitir bomba de compressão.

use crate::erro::ErroNfse;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use std::io::{Read, Write};

/// Limite do XML descompactado, em bytes.
pub const LIMITE_XML: usize = 8 * 1024 * 1024;

/// Comprime o XML e devolve base64 padrão.
pub fn comprimir_b64(xml: &str) -> Result<String, ErroNfse> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(xml.as_bytes())
        .map_err(|erro| ErroNfse::Compressao(format!("escrita gzip: {erro}")))?;
    let comprimido = encoder
        .finish()
        .map_err(|erro| ErroNfse::Compressao(format!("fim gzip: {erro}")))?;
    Ok(STANDARD.encode(comprimido))
}

/// Decodifica base64, descomprime e exige UTF-8 dentro do limite.
pub fn descomprimir_b64(codificado: &str) -> Result<String, ErroNfse> {
    let comprimido = STANDARD
        .decode(codificado.trim())
        .map_err(|erro| ErroNfse::Compressao(format!("base64: {erro}")))?;
    let mut leitor = GzDecoder::new(comprimido.as_slice()).take((LIMITE_XML + 1) as u64);
    let mut xml = Vec::new();
    leitor
        .read_to_end(&mut xml)
        .map_err(|erro| ErroNfse::Compressao(format!("leitura gzip: {erro}")))?;
    if xml.len() > LIMITE_XML {
        return Err(ErroNfse::Compressao(format!(
            "XML descompactado acima de {LIMITE_XML} bytes"
        )));
    }
    String::from_utf8(xml)
        .map_err(|_| ErroNfse::Compressao("XML descompactado não é UTF-8".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comprime_e_descomprime_ida_e_volta() {
        let xml = "<DPS>serviço sintético</DPS>";
        let b64 = comprimir_b64(xml).expect("comprime");
        assert_ne!(b64, xml);
        assert_eq!(descomprimir_b64(&b64).expect("descomprime"), xml);
    }

    #[test]
    fn recusa_base64_invalido() {
        assert!(descomprimir_b64("não é base64").is_err());
    }
}
