//! Lê XML da entrada padrão e escreve a forma canônica C14N 1.0 na saída.

use std::io::{Read, Write};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut entrada = Vec::new();
    std::io::stdin().read_to_end(&mut entrada)?;
    let documento = carimbar_xml::Documento::parse(&entrada)?;
    let canonico = documento.canonicalizar()?;
    std::io::stdout().write_all(canonico.as_bytes())?;
    Ok(())
}
