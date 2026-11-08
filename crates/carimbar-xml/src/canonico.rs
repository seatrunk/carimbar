//! Canonicalização XML C14N 1.0 inclusiva, sem comentários.

use crate::documento::{Documento, Elemento, No, URI_XML};
use crate::erro::ErroXml;
use crate::escrita::{escrever_atributo, escrever_texto};
use crate::validacao::namespace_do_atributo;
use std::collections::BTreeMap;

/// Gera a forma canônica de um documento.
///
/// Implementa o perfil inclusivo sem comentários usado por XMLDSig
/// (`http://www.w3.org/TR/2001/REC-xml-c14n-20010315`).
pub(crate) fn canonicalizar(documento: &Documento) -> Result<String, ErroXml> {
    crate::validacao::validar(documento)?;
    let mut saida = String::new();
    for no in &documento.antes {
        if let No::InstrucaoProcessamento { alvo, dados } = no {
            escrever_instrucao(alvo, dados, &mut saida);
            saida.push('\n');
        }
    }

    let mut escopo = BTreeMap::new();
    escopo.insert("xml".to_owned(), URI_XML.to_owned());
    escrever_elemento(&documento.raiz, &escopo, &BTreeMap::new(), &mut saida)?;

    for no in &documento.depois {
        if let No::InstrucaoProcessamento { alvo, dados } = no {
            saida.push('\n');
            escrever_instrucao(alvo, dados, &mut saida);
        }
    }
    Ok(saida)
}

fn escrever_elemento(
    elemento: &Elemento,
    escopo_pai: &BTreeMap<String, String>,
    renderizado_pai: &BTreeMap<String, String>,
    saida: &mut String,
) -> Result<(), ErroXml> {
    let mut escopo = escopo_pai.clone();
    for (prefixo, uri) in elemento.declaracoes_namespace() {
        escopo.insert(prefixo.unwrap_or("").to_owned(), uri.to_owned());
    }

    let mut declaracoes = Vec::new();
    for (prefixo, uri) in &escopo {
        if prefixo == "xml" {
            continue;
        }
        if prefixo.is_empty() {
            let atual = renderizado_pai
                .get(prefixo)
                .map(String::as_str)
                .unwrap_or("");
            if uri != atual {
                declaracoes.push((String::new(), uri.clone()));
            }
        } else if renderizado_pai.get(prefixo) != Some(uri) {
            declaracoes.push((prefixo.clone(), uri.clone()));
        }
    }
    declaracoes.sort_by(|a, b| a.0.cmp(&b.0));

    saida.push('<');
    saida.push_str(&elemento.nome().qualificado());
    for (prefixo, uri) in &declaracoes {
        if prefixo.is_empty() {
            saida.push_str(" xmlns=\"");
        } else {
            saida.push_str(" xmlns:");
            saida.push_str(prefixo);
            saida.push_str("=\"");
        }
        escrever_atributo(uri, saida);
        saida.push('"');
    }

    let mut atributos = Vec::new();
    for atributo in elemento.atributos() {
        if atributo.eh_declaracao_namespace() {
            continue;
        }
        let namespace = namespace_do_atributo(atributo, &escopo)?;
        atributos.push((atributo, namespace, atributo.nome().local().to_owned()));
    }
    atributos.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2)));
    for (atributo, _, _) in atributos {
        saida.push(' ');
        saida.push_str(&atributo.nome().qualificado());
        saida.push_str("=\"");
        escrever_atributo(atributo.valor(), saida);
        saida.push('"');
    }
    saida.push('>');

    for filho in elemento.filhos() {
        match filho {
            No::Elemento(filho) => escrever_elemento(filho, &escopo, &escopo, saida)?,
            No::Texto(texto) => escrever_texto(texto, saida),
            No::Comentario(_) => {}
            No::InstrucaoProcessamento { alvo, dados } => {
                escrever_instrucao(alvo, dados, saida);
            }
        }
    }

    saida.push_str("</");
    saida.push_str(&elemento.nome().qualificado());
    saida.push('>');
    Ok(())
}

fn escrever_instrucao(alvo: &str, dados: &str, saida: &mut String) {
    saida.push_str("<?");
    saida.push_str(alvo);
    if !dados.is_empty() {
        saida.push(' ');
        saida.push_str(dados);
    }
    saida.push_str("?>");
}

/// Canonicaliza a subárvore do elemento em `caminho` (índices em `filhos()` a partir
/// da raiz), no perfil inclusivo do C14N 1.0.
///
/// O elemento recebe os namespaces em escopo e os atributos no namespace XML
/// do ancestral mais próximo. C14N 1.0 herda inclusive `xml:base` e `xml:id`;
/// as regras especiais desses atributos pertencem ao C14N 1.1.
pub(crate) fn canonicalizar_subarvore(
    documento: &Documento,
    caminho: &[usize],
) -> Result<String, ErroXml> {
    crate::validacao::validar(documento)?;
    let mut ancestrais: Vec<&Elemento> = Vec::with_capacity(caminho.len());
    let mut atual = &documento.raiz;
    for indice in caminho {
        ancestrais.push(atual);
        atual = match atual.filhos().get(*indice) {
            Some(No::Elemento(filho)) => filho,
            _ => {
                return Err(ErroXml::Estrutura(
                    "caminho até o elemento é inválido".to_owned(),
                ));
            }
        };
    }

    let mut escopo = BTreeMap::new();
    escopo.insert("xml".to_owned(), URI_XML.to_owned());
    for ancestral in &ancestrais {
        for (prefixo, uri) in ancestral.declaracoes_namespace() {
            escopo.insert(prefixo.unwrap_or("").to_owned(), uri.to_owned());
        }
    }

    let mut apice = atual.clone();
    for ancestral in ancestrais.iter().rev() {
        for atributo in ancestral.atributos() {
            let local = atributo.nome().local();
            if eh_xml(atributo, local) && !apice.atributos().iter().any(|a| eh_xml(a, local)) {
                apice = apice.com_atributo(atributo.nome().clone(), atributo.valor());
            }
        }
    }

    let mut saida = String::new();
    escrever_elemento(&apice, &escopo, &BTreeMap::new(), &mut saida)?;
    Ok(saida)
}

fn eh_xml(atributo: &crate::documento::Atributo, local: &str) -> bool {
    atributo.nome().prefixo() == Some("xml") && atributo.nome().local() == local
}
