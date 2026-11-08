//! Leitura de XML com limites de recursos e rejeição de construções inseguras.

use crate::documento::{Atributo, Declaracao, Documento, Elemento, Limites, No, Nome};
use crate::erro::ErroXml;
use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

/// Lê um documento XML UTF-8 com os limites padrão.
pub fn ler(xml: &[u8]) -> Result<Documento, ErroXml> {
    ler_com_limites(xml, &Limites::padrao())
}

/// Lê um documento XML UTF-8 com limites explícitos.
///
/// Rejeita DTD, entidades não predefinidas, codificação diferente de UTF-8,
/// profundidade excessiva e quantidade excessiva de elementos/atributos.
pub fn ler_com_limites(xml: &[u8], limites: &Limites) -> Result<Documento, ErroXml> {
    if xml.len() > limites.bytes_max {
        return Err(ErroXml::limite("bytes", xml.len(), limites.bytes_max));
    }
    let texto = std::str::from_utf8(xml)
        .map_err(|e| ErroXml::Codificacao(format!("bytes inválidos em UTF-8: {e}")))?;
    let texto = texto.strip_prefix('\u{feff}').unwrap_or(texto);

    let mut leitor = Reader::from_str(texto);
    leitor.config_mut().enable_all_checks(true);
    leitor.config_mut().trim_text(false);

    let mut estado = Estado::novo(limites);
    loop {
        let posicao = leitor.buffer_position();
        let evento = leitor
            .read_event()
            .map_err(|e| ErroXml::sintaxe(posicao, e.to_string()))?;
        match evento {
            Event::Start(inicio) => estado.abrir(&inicio, posicao)?,
            Event::Empty(inicio) => estado.vazio(&inicio, posicao)?,
            Event::End(_) => estado.fechar(posicao)?,
            Event::Text(texto) => {
                let conteudo = texto.xml10_content();
                let decodificado = quick_xml::escape::unescape(&conteudo)
                    .map_err(|e| ErroXml::sintaxe(posicao, e.to_string()))?;
                estado.adicionar_texto(&decodificado, posicao)?;
            }
            Event::CData(dados) => {
                let normalizado = normalizar_fim_de_linha(dados.into_inner().as_ref());
                estado.adicionar_texto(&normalizado, posicao)?;
            }
            Event::GeneralRef(referencia) => {
                estado.adicionar_referencia(&referencia, posicao)?;
            }
            Event::Comment(comentario) => {
                let texto = comentario.into_inner().into_owned();
                estado.adicionar_no(No::Comentario(texto), posicao)?;
            }
            Event::PI(instrucao) => {
                let alvo = instrucao.target().to_owned();
                let dados = instrucao.content().trim_start().to_owned();
                estado.adicionar_no(No::InstrucaoProcessamento { alvo, dados }, posicao)?;
            }
            Event::Decl(declaracao) => estado.declarar(&declaracao, posicao)?,
            Event::DocType(_) => return Err(ErroXml::DtdProibida { posicao }),
            Event::Eof => break,
        }
    }

    estado.concluir()
}

struct Estado<'a> {
    limites: &'a Limites,
    pilha: Vec<Elemento>,
    antes: Vec<No>,
    depois: Vec<No>,
    raiz: Option<Elemento>,
    declaracao: Declaracao,
    declaracao_vista: bool,
    elementos: usize,
}

impl<'a> Estado<'a> {
    fn novo(limites: &'a Limites) -> Self {
        Self {
            limites,
            pilha: Vec::new(),
            antes: Vec::new(),
            depois: Vec::new(),
            raiz: None,
            declaracao: Declaracao::default(),
            declaracao_vista: false,
            elementos: 0,
        }
    }

    fn abrir(&mut self, inicio: &BytesStart<'_>, posicao: u64) -> Result<(), ErroXml> {
        self.verificar_profundidade()?;
        let elemento = self.construir(inicio, posicao)?;
        self.pilha.push(elemento);
        Ok(())
    }

    fn vazio(&mut self, inicio: &BytesStart<'_>, posicao: u64) -> Result<(), ErroXml> {
        self.verificar_profundidade()?;
        let elemento = self.construir(inicio, posicao)?;
        self.anexar(No::Elemento(elemento), posicao)
    }

    fn verificar_profundidade(&self) -> Result<(), ErroXml> {
        let profundidade = self.pilha.len() + 1;
        if profundidade > self.limites.profundidade_max {
            return Err(ErroXml::limite(
                "profundidade",
                profundidade,
                self.limites.profundidade_max,
            ));
        }
        Ok(())
    }

    fn construir(&mut self, inicio: &BytesStart<'_>, posicao: u64) -> Result<Elemento, ErroXml> {
        self.elementos += 1;
        if self.elementos > self.limites.elementos_max {
            return Err(ErroXml::limite(
                "elementos",
                self.elementos,
                self.limites.elementos_max,
            ));
        }
        let nome = Nome::parse(inicio.name().as_ref())
            .map_err(|e| ErroXml::sintaxe(posicao, e.to_string()))?;
        let mut atributos = Vec::new();
        for atributo in inicio.attributes() {
            let atributo = atributo
                .map_err(|e| ErroXml::sintaxe(posicao, format!("atributo inválido: {e}")))?;
            if atributos.len() >= self.limites.atributos_por_elemento_max {
                return Err(ErroXml::limite(
                    "atributos por elemento",
                    atributos.len() + 1,
                    self.limites.atributos_por_elemento_max,
                ));
            }
            let nome = Nome::parse(atributo.key.as_ref())
                .map_err(|e| ErroXml::sintaxe(posicao, e.to_string()))?;
            let valor = atributo
                .normalized_value(XmlVersion::Explicit1_0)
                .map_err(|e| ErroXml::sintaxe(posicao, format!("valor inválido: {e}")))?;
            atributos.push(Atributo::new(nome, valor.into_owned()));
        }
        let mut elemento = Elemento::new(nome);
        for atributo in atributos {
            elemento = elemento.com_atributo(atributo.nome().clone(), atributo.valor());
        }
        Ok(elemento)
    }

    fn fechar(&mut self, posicao: u64) -> Result<(), ErroXml> {
        let elemento = self
            .pilha
            .pop()
            .ok_or_else(|| ErroXml::sintaxe(posicao, "fechamento sem elemento aberto"))?;
        self.anexar(No::Elemento(elemento), posicao)
    }

    fn anexar(&mut self, no: No, posicao: u64) -> Result<(), ErroXml> {
        if let Some(atual) = self.pilha.last_mut() {
            atual.filhos_mut().push(no);
            return Ok(());
        }
        match no {
            No::Elemento(elemento) => {
                if self.raiz.is_some() {
                    return Err(ErroXml::sintaxe(
                        posicao,
                        "documento com mais de um elemento raiz",
                    ));
                }
                self.raiz = Some(elemento);
            }
            No::Texto(texto) => {
                if !texto.trim().is_empty() {
                    return Err(ErroXml::sintaxe(posicao, "texto fora do elemento raiz"));
                }
            }
            outro => {
                if self.raiz.is_some() {
                    self.depois.push(outro);
                } else {
                    self.antes.push(outro);
                }
            }
        }
        Ok(())
    }

    fn adicionar_texto(&mut self, texto: &str, posicao: u64) -> Result<(), ErroXml> {
        if texto.is_empty() {
            return Ok(());
        }
        if self.pilha.is_empty() {
            if texto.trim().is_empty() {
                return Ok(());
            }
            return Err(ErroXml::sintaxe(posicao, "texto fora do elemento raiz"));
        }
        let atual = self
            .pilha
            .last_mut()
            .ok_or_else(|| ErroXml::sintaxe(posicao, "texto sem elemento aberto"))?;
        match atual.filhos_mut().last_mut() {
            Some(No::Texto(anterior)) => anterior.push_str(texto),
            _ => atual.filhos_mut().push(No::Texto(texto.to_owned())),
        }
        Ok(())
    }

    fn adicionar_referencia(
        &mut self,
        referencia: &quick_xml::events::BytesRef<'_>,
        posicao: u64,
    ) -> Result<(), ErroXml> {
        if let Some(caractere) = referencia
            .resolve_char_ref()
            .map_err(|e| ErroXml::sintaxe(posicao, e.to_string()))?
        {
            let mut buffer = [0_u8; 4];
            self.adicionar_texto(caractere.encode_utf8(&mut buffer), posicao)?;
            return Ok(());
        }
        let nome = referencia.as_ref();
        match quick_xml::escape::resolve_predefined_entity(nome) {
            Some(valor) => self.adicionar_texto(valor, posicao),
            None => Err(ErroXml::Entidade {
                nome: nome.to_owned(),
                posicao,
            }),
        }
    }

    fn adicionar_no(&mut self, no: No, posicao: u64) -> Result<(), ErroXml> {
        self.anexar(no, posicao)
    }

    fn declarar(
        &mut self,
        declaracao: &quick_xml::events::BytesDecl<'_>,
        posicao: u64,
    ) -> Result<(), ErroXml> {
        if self.declaracao_vista
            || !self.antes.is_empty()
            || !self.pilha.is_empty()
            || self.raiz.is_some()
        {
            return Err(ErroXml::sintaxe(
                posicao,
                "declaração XML fora do início do documento",
            ));
        }
        self.declaracao_vista = true;
        let versao = declaracao
            .version()
            .map_err(|e| ErroXml::sintaxe(posicao, e.to_string()))?;
        if versao != "1.0" {
            return Err(ErroXml::Codificacao(format!(
                "versão XML {versao} não suportada; use 1.0"
            )));
        }
        if let Some(codificacao) = declaracao.encoding() {
            let codificacao = codificacao.map_err(|e| ErroXml::sintaxe(posicao, e.to_string()))?;
            if !codificacao.eq_ignore_ascii_case("utf-8") {
                return Err(ErroXml::Codificacao(codificacao.into_owned()));
            }
        }
        let standalone = match declaracao.standalone() {
            Some(resultado) => {
                let valor = resultado.map_err(|e| ErroXml::sintaxe(posicao, e.to_string()))?;
                match valor.as_ref() {
                    "yes" => Some(true),
                    "no" => Some(false),
                    outro => {
                        return Err(ErroXml::sintaxe(
                            posicao,
                            format!("standalone inválido: {outro:?}"),
                        ));
                    }
                }
            }
            None => None,
        };
        self.declaracao = Declaracao {
            versao: versao.into_owned(),
            standalone,
        };
        Ok(())
    }

    fn concluir(self) -> Result<Documento, ErroXml> {
        if !self.pilha.is_empty() {
            return Err(ErroXml::Estrutura(format!(
                "{} elemento(s) sem fechamento",
                self.pilha.len()
            )));
        }
        let raiz = self
            .raiz
            .ok_or_else(|| ErroXml::Estrutura("documento sem elemento raiz".to_owned()))?;
        let documento = Documento {
            declaracao: self.declaracao,
            antes: self.antes,
            raiz,
            depois: self.depois,
        };
        crate::validacao::validar(&documento)?;
        Ok(documento)
    }
}

fn normalizar_fim_de_linha(texto: &str) -> String {
    let mut saida = String::with_capacity(texto.len());
    let mut caracteres = texto.chars().peekable();
    while let Some(caractere) = caracteres.next() {
        if caractere == '\r' {
            if caracteres.peek() == Some(&'\n') {
                caracteres.next();
            }
            saida.push('\n');
        } else {
            saida.push(caractere);
        }
    }
    saida
}
