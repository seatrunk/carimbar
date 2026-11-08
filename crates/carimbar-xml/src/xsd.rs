//! Validação XSD opcional (feature `xsd`).
//!
//! Os XSD oficiais **não são embutidos** no pacote publicado. O chamador aponta
//! para um diretório de schemas vendorizado; `xs:include` e `xs:import` são
//! resolvidos a partir do caminho informado. O repositório mantém os pacotes em
//! `schemas/` com `SHA256SUMS` e origem em `docs/contratos-fiscais.md`.

use crate::erro::ErroXml;
use std::path::Path;
use xsd_schema::validation::{
    CollectingValidationSink, SchemaValidator, SchemaValidity, ValidationError, ValidationFlags,
    ValidationWarning, drive_quick_xml,
};
use xsd_schema::{SchemaSet, SchemaSetBuilder};

/// Validador XSD compilado, reutilizável entre documentos.
#[derive(Debug)]
pub struct Validador {
    schema_set: SchemaSet,
}

impl Validador {
    /// Compila o XSD principal, carregando includes/imports relativos ao arquivo.
    pub fn de_esquema(caminho: impl AsRef<Path>) -> Result<Self, ErroXml> {
        let caminho = caminho.as_ref();
        let location = caminho.to_str().ok_or_else(|| {
            ErroXml::Estrutura(format!(
                "caminho de schema não é UTF-8: {}",
                caminho.display()
            ))
        })?;
        let mut construtor = SchemaSetBuilder::new();
        let carregado = construtor
            .try_add(location)
            .map_err(|e| ErroXml::Xsd(format!("falha ao ler {location}: {e}")))?;
        if !carregado {
            return Err(ErroXml::Xsd(format!("esquema não carregado: {location}")));
        }
        let compilado = construtor
            .compile()
            .map_err(|e| ErroXml::Xsd(format!("falha ao compilar {location}: {e}")))?;
        let falhas: Vec<String> = compilado
            .directive_failures()
            .iter()
            .map(|falha| format!("{falha:?}"))
            .collect();
        if !falhas.is_empty() {
            return Err(ErroXml::Xsd(format!(
                "diretivas XSD não resolvidas: {}",
                falhas.join("; ")
            )));
        }
        Ok(Self {
            schema_set: compilado.into_schema_set(),
        })
    }

    /// Valida um documento XML já serializado em UTF-8.
    pub fn validar(&self, xml: &[u8]) -> Result<(), ErroXml> {
        let validador = SchemaValidator::new(&self.schema_set, ValidationFlags::default());
        let mut erros: Vec<ValidationError> = Vec::new();
        let mut avisos: Vec<ValidationWarning> = Vec::new();
        {
            let mut execucao = validador.start_run(CollectingValidationSink {
                errors: &mut erros,
                warnings: &mut avisos,
            });
            let resultado = drive_quick_xml(xml, &mut execucao, &self.schema_set)
                .map_err(|e| ErroXml::Xsd(format!("falha ao percorrer o XML: {e}")))?;
            if resultado.root_validity != Some(SchemaValidity::Valid) && erros.is_empty() {
                return Err(ErroXml::Xsd(format!(
                    "raiz não validada (estado: {:?})",
                    resultado.root_validity
                )));
            }
        }
        if !erros.is_empty() {
            return Err(ErroXml::Xsd(formatar_erros(&erros)));
        }
        Ok(())
    }
}

/// Compila o XSD e valida um documento em uma chamada.
pub fn validar(xml: &[u8], esquema: impl AsRef<Path>) -> Result<(), ErroXml> {
    Validador::de_esquema(esquema)?.validar(xml)
}

fn formatar_erros(erros: &[ValidationError]) -> String {
    let mut saida = format!("{} erro(s)", erros.len());
    for erro in erros.iter().take(10) {
        saida.push_str("; ");
        saida.push_str(&erro.to_string());
    }
    if erros.len() > 10 {
        saida.push_str("; ...");
    }
    saida
}
