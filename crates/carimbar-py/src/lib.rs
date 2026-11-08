//! Binding Python (PyO3 + maturin, abi3-py39). API síncrona no 0.1; async na 0.2 (D6).
//!
//! Status: esqueleto. Expõe só validações de `carimbar-types` para provar o
//! build do wheel. A API completa (`Cofre`, `Emissor`) chega na Etapa 6
//! (`docs/plano-mvp.md`, issue #20).

use pyo3::prelude::*;

/// Retorna `True` se o CNPJ (com ou sem máscara) tem dígitos verificadores válidos.
#[pyfunction]
fn validar_cnpj(cnpj: &str) -> bool {
    carimbar_types::Cnpj::parse(cnpj).is_ok()
}

/// Retorna `True` se a chave de acesso de 44 posições tem DV válido.
#[pyfunction]
fn validar_chave_acesso(chave: &str) -> bool {
    carimbar_types::ChaveAcesso::parse(chave).is_ok()
}

#[pymodule]
fn _carimbar(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_function(wrap_pyfunction!(validar_cnpj, m)?)?;
    m.add_function(wrap_pyfunction!(validar_chave_acesso, m)?)?;
    Ok(())
}
