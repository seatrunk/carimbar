//! Serialização determinística de XML.

use crate::documento::{Documento, Elemento, No};
use crate::erro::ErroXml;

pub(crate) fn escrever(documento: &Documento) -> Result<String, ErroXml> {
    crate::validacao::validar(documento)?;
    let mut saida = String::new();
    saida.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"");
    if let Some(standalone) = documento.declaracao.standalone {
        saida.push_str(if standalone {
            " standalone=\"yes\""
        } else {
            " standalone=\"no\""
        });
    }
    saida.push_str("?>");
    for no in &documento.antes {
        escrever_no(no, &mut saida);
    }
    escrever_elemento(&documento.raiz, &mut saida);
    for no in &documento.depois {
        escrever_no(no, &mut saida);
    }
    Ok(saida)
}

fn escrever_no(no: &No, saida: &mut String) {
    match no {
        No::Elemento(elemento) => escrever_elemento(elemento, saida),
        No::Texto(texto) => escrever_texto(texto, saida),
        No::Comentario(comentario) => {
            saida.push_str("<!--");
            saida.push_str(comentario);
            saida.push_str("-->");
        }
        No::InstrucaoProcessamento { alvo, dados } => escrever_instrucao(alvo, dados, saida),
    }
}

fn escrever_elemento(elemento: &Elemento, saida: &mut String) {
    saida.push('<');
    saida.push_str(&elemento.nome().qualificado());
    for atributo in elemento.atributos() {
        saida.push(' ');
        saida.push_str(&atributo.nome().qualificado());
        saida.push_str("=\"");
        escrever_atributo(atributo.valor(), saida);
        saida.push('"');
    }
    if elemento.filhos().is_empty() {
        saida.push_str("/>");
        return;
    }
    saida.push('>');
    for filho in elemento.filhos() {
        escrever_no(filho, saida);
    }
    saida.push_str("</");
    saida.push_str(&elemento.nome().qualificado());
    saida.push('>');
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

pub(crate) fn escrever_texto(texto: &str, saida: &mut String) {
    for caractere in texto.chars() {
        match caractere {
            '&' => saida.push_str("&amp;"),
            '<' => saida.push_str("&lt;"),
            '>' => saida.push_str("&gt;"),
            '\r' => saida.push_str("&#xD;"),
            outro => saida.push(outro),
        }
    }
}

pub(crate) fn escrever_atributo(valor: &str, saida: &mut String) {
    for caractere in valor.chars() {
        match caractere {
            '&' => saida.push_str("&amp;"),
            '<' => saida.push_str("&lt;"),
            '"' => saida.push_str("&quot;"),
            '\t' => saida.push_str("&#x9;"),
            '\n' => saida.push_str("&#xA;"),
            '\r' => saida.push_str("&#xD;"),
            outro => saida.push(outro),
        }
    }
}
