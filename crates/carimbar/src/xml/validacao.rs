//! Validação estrutural da árvore antes de serializar ou canonicalizar.

use crate::xml::documento::{Atributo, Documento, Elemento, No, Nome, URI_XML, URI_XMLNS};
use crate::xml::erro::ErroXml;
use std::collections::{BTreeMap, BTreeSet};

/// Valida nomes, namespaces, atributos repetidos e conteúdo de nós.
pub(crate) fn validar(documento: &Documento) -> Result<(), ErroXml> {
    if documento.declaracao.versao != "1.0" {
        return Err(ErroXml::Codificacao(format!(
            "versão XML {} não suportada; use 1.0",
            documento.declaracao.versao
        )));
    }
    for no in documento.antes.iter().chain(documento.depois.iter()) {
        match no {
            No::Comentario(comentario) => validar_comentario(comentario)?,
            No::InstrucaoProcessamento { alvo, dados } => validar_instrucao(alvo, dados)?,
            No::Texto(_) => {
                return Err(ErroXml::Estrutura(
                    "texto não é permitido fora do elemento raiz".to_owned(),
                ));
            }
            No::Elemento(_) => {
                return Err(ErroXml::Estrutura("elemento extra fora da raiz".to_owned()));
            }
        }
    }
    let mut escopo = BTreeMap::new();
    escopo.insert("xml".to_owned(), URI_XML.to_owned());
    validar_elemento(&documento.raiz, &escopo)
}

fn validar_elemento(
    elemento: &Elemento,
    escopo_pai: &BTreeMap<String, String>,
) -> Result<(), ErroXml> {
    validar_nome(elemento.nome())?;
    let mut escopo = escopo_pai.clone();
    let mut declarados = BTreeSet::new();
    for (prefixo, uri) in elemento.declaracoes_namespace() {
        let chave = prefixo.unwrap_or("");
        if !declarados.insert(chave.to_owned()) {
            return Err(ErroXml::Namespace(format!(
                "prefixo {chave:?} declarado mais de uma vez"
            )));
        }
        match prefixo {
            None => {
                if uri == URI_XMLNS || uri == URI_XML {
                    return Err(ErroXml::Namespace(
                        "o namespace padrão não pode ser xml nem xmlns".to_owned(),
                    ));
                }
            }
            Some("xmlns") => {
                return Err(ErroXml::Namespace(
                    "o prefixo xmlns não pode ser declarado como namespace".to_owned(),
                ));
            }
            Some("xml") => {
                if uri != URI_XML {
                    return Err(ErroXml::Namespace(format!(
                        "o prefixo xml deve apontar para {URI_XML}"
                    )));
                }
            }
            Some(prefixo) => {
                if prefixo.is_empty() {
                    return Err(ErroXml::Namespace("prefixo de namespace vazio".to_owned()));
                }
                if uri.is_empty() {
                    return Err(ErroXml::Namespace(format!(
                        "o prefixo {prefixo:?} não pode ser declarado com URI vazia"
                    )));
                }
                if uri == URI_XML {
                    return Err(ErroXml::Namespace(format!(
                        "o prefixo {prefixo:?} não pode reutilizar o namespace xml"
                    )));
                }
                if uri == URI_XMLNS {
                    return Err(ErroXml::Namespace(format!(
                        "o prefixo {prefixo:?} não pode apontar para o namespace xmlns"
                    )));
                }
            }
        }
        escopo.insert(chave.to_owned(), uri.to_owned());
    }

    if let Some(prefixo) = elemento.nome().prefixo() {
        if prefixo == "xmlns" {
            return Err(ErroXml::Namespace(
                "elemento não pode usar o prefixo xmlns".to_owned(),
            ));
        }
        if escopo.get(prefixo).is_none_or(String::is_empty) {
            return Err(ErroXml::Namespace(format!(
                "prefixo {prefixo:?} do elemento {} não está declarado",
                elemento.nome().qualificado()
            )));
        }
    }

    let mut vistos = BTreeSet::new();
    for atributo in elemento.atributos() {
        if atributo.eh_declaracao_namespace() {
            continue;
        }
        validar_nome(atributo.nome())?;
        validar_caracteres(atributo.valor())?;
        let namespace = namespace_do_atributo(atributo, &escopo)?;
        let chave = (namespace, atributo.nome().local().to_owned());
        if !vistos.insert(chave) {
            return Err(ErroXml::Estrutura(format!(
                "atributo repetido: {}",
                atributo.nome().qualificado()
            )));
        }
    }

    for filho in elemento.filhos() {
        match filho {
            No::Elemento(filho) => validar_elemento(filho, &escopo)?,
            No::Texto(texto) => validar_caracteres(texto)?,
            No::Comentario(comentario) => validar_comentario(comentario)?,
            No::InstrucaoProcessamento { alvo, dados } => {
                validar_instrucao(alvo, dados)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn namespace_do_atributo(
    atributo: &Atributo,
    escopo: &BTreeMap<String, String>,
) -> Result<String, ErroXml> {
    match atributo.nome().prefixo() {
        None => Ok(String::new()),
        Some("xml") => Ok(URI_XML.to_owned()),
        Some(prefixo) => escopo
            .get(prefixo)
            .filter(|uri| !uri.is_empty())
            .cloned()
            .ok_or_else(|| {
                ErroXml::Namespace(format!(
                    "prefixo {prefixo:?} do atributo não está declarado"
                ))
            }),
    }
}

fn validar_nome(nome: &Nome) -> Result<(), ErroXml> {
    if let Some(prefixo) = nome.prefixo() {
        if prefixo.is_empty() {
            return Err(ErroXml::Estrutura("prefixo de nome vazio".to_owned()));
        }
        validar_ncname(prefixo)?;
    }
    validar_ncname(nome.local())
}

pub(crate) fn validar_ncname(nome: &str) -> Result<(), ErroXml> {
    let mut caracteres = nome.chars();
    let primeiro = caracteres
        .next()
        .ok_or_else(|| ErroXml::Estrutura("nome XML vazio".to_owned()))?;
    if !(primeiro.is_alphabetic() || primeiro == '_' || u32::from(primeiro) >= 0x80) {
        return Err(ErroXml::Estrutura(format!(
            "nome XML começa com caractere inválido: {nome:?}"
        )));
    }
    for caractere in caracteres {
        let valido = caractere.is_alphanumeric()
            || matches!(caractere, '_' | '-' | '.')
            || u32::from(caractere) >= 0x80;
        if !valido {
            return Err(ErroXml::Estrutura(format!(
                "nome XML contém caractere inválido {caractere:?}: {nome:?}"
            )));
        }
    }
    Ok(())
}

fn validar_caracteres(texto: &str) -> Result<(), ErroXml> {
    for caractere in texto.chars() {
        let codigo = u32::from(caractere);
        let valido = matches!(codigo, 0x9 | 0xA | 0xD)
            || (0x20..=0xD7FF).contains(&codigo)
            || (0xE000..=0xFFFD).contains(&codigo)
            || (0x1_0000..=0x10_FFFF).contains(&codigo);
        if !valido {
            return Err(ErroXml::Estrutura(format!(
                "caractere inválido em XML: U+{codigo:04X}"
            )));
        }
    }
    Ok(())
}

fn validar_comentario(comentario: &str) -> Result<(), ErroXml> {
    if comentario.contains("--") || comentario.ends_with('-') {
        return Err(ErroXml::Estrutura(
            "comentário contém sequência inválida".to_owned(),
        ));
    }
    validar_caracteres(comentario)
}

fn validar_instrucao(alvo: &str, dados: &str) -> Result<(), ErroXml> {
    validar_ncname(alvo)?;
    if alvo.eq_ignore_ascii_case("xml") {
        return Err(ErroXml::Estrutura(
            "alvo xml é reservado para a declaração XML".to_owned(),
        ));
    }
    if dados.contains("?>") {
        return Err(ErroXml::Estrutura(
            "instrução de processamento contém ?>".to_owned(),
        ));
    }
    validar_caracteres(dados)
}
