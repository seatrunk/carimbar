//! Árvore XML com nomes qualificados e declarações de namespace preservados.

use crate::erro::ErroXml;

/// Prefixo reservado para o namespace XML.
pub const PREFIXO_XML: &str = "xml";

/// URI do namespace XML.
pub const URI_XML: &str = "http://www.w3.org/XML/1998/namespace";

/// URI do namespace `xmlns`.
pub const URI_XMLNS: &str = "http://www.w3.org/2000/xmlns/";

/// Nome qualificado (`prefixo:local` ou apenas `local`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Nome {
    prefixo: Option<String>,
    local: String,
}

impl Nome {
    /// Cria um nome sem prefixo.
    pub fn new(local: impl Into<String>) -> Self {
        Self {
            prefixo: None,
            local: local.into(),
        }
    }

    /// Cria um nome com prefixo.
    pub fn com_prefixo(prefixo: impl Into<String>, local: impl Into<String>) -> Self {
        Self {
            prefixo: Some(prefixo.into()),
            local: local.into(),
        }
    }

    /// Interpreta um nome qualificado como aparece no XML.
    pub fn parse(entrada: &str) -> Result<Self, ErroXml> {
        let mut partes = entrada.split(':');
        let primeira = partes
            .next()
            .filter(|p| !p.is_empty())
            .ok_or_else(|| ErroXml::Estrutura("nome XML vazio".to_owned()))?;
        let segunda = partes.next();
        if partes.next().is_some() {
            return Err(ErroXml::Estrutura(format!(
                "nome XML com mais de um dois-pontos: {entrada:?}"
            )));
        }
        match segunda {
            Some(local) => {
                crate::validacao::validar_ncname(primeira)?;
                crate::validacao::validar_ncname(local)?;
                Ok(Self::com_prefixo(primeira, local))
            }
            None => {
                crate::validacao::validar_ncname(primeira)?;
                Ok(Self::new(primeira))
            }
        }
    }

    /// Prefixo, se houver.
    pub fn prefixo(&self) -> Option<&str> {
        self.prefixo.as_deref()
    }

    /// Parte local.
    pub fn local(&self) -> &str {
        &self.local
    }

    /// Representação `prefixo:local` ou `local`.
    pub fn qualificado(&self) -> String {
        match &self.prefixo {
            Some(prefixo) => format!("{prefixo}:{}", self.local),
            None => self.local.clone(),
        }
    }
}

/// Atributo com nome qualificado e valor já normalizado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atributo {
    nome: Nome,
    valor: String,
}

impl Atributo {
    /// Cria um atributo.
    pub fn new(nome: Nome, valor: impl Into<String>) -> Self {
        Self {
            nome,
            valor: valor.into(),
        }
    }

    /// Nome do atributo.
    pub fn nome(&self) -> &Nome {
        &self.nome
    }

    /// Valor do atributo.
    pub fn valor(&self) -> &str {
        &self.valor
    }

    pub(crate) fn eh_declaracao_namespace(&self) -> bool {
        (self.nome.prefixo().is_none() && self.nome.local() == "xmlns")
            || self.nome.prefixo() == Some("xmlns")
    }

    /// Prefixo declarado: `None` para o namespace padrão.
    pub(crate) fn prefixo_declarado(&self) -> Option<&str> {
        match self.nome.prefixo() {
            None => None,
            Some("xmlns") => Some(self.nome.local()),
            Some(_) => Some(self.nome.local()),
        }
    }
}

/// Nó filho de um elemento ou do documento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum No {
    /// Elemento.
    Elemento(Elemento),
    /// Texto já decodificado.
    Texto(String),
    /// Comentário, sem os delimitadores.
    Comentario(String),
    /// Instrução de processamento.
    InstrucaoProcessamento {
        /// Alvo da instrução.
        alvo: String,
        /// Dados da instrução, sem o espaço separador.
        dados: String,
    },
}

/// Elemento XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elemento {
    nome: Nome,
    atributos: Vec<Atributo>,
    filhos: Vec<No>,
}

impl Elemento {
    /// Cria um elemento vazio.
    pub fn new(nome: Nome) -> Self {
        Self {
            nome,
            atributos: Vec::new(),
            filhos: Vec::new(),
        }
    }

    /// Nome do elemento.
    pub fn nome(&self) -> &Nome {
        &self.nome
    }

    /// Atributos, na ordem original, incluindo declarações `xmlns`.
    pub fn atributos(&self) -> &[Atributo] {
        &self.atributos
    }

    /// Filhos, na ordem do documento.
    pub fn filhos(&self) -> &[No] {
        &self.filhos
    }

    /// Filhos mutáveis.
    pub fn filhos_mut(&mut self) -> &mut Vec<No> {
        &mut self.filhos
    }

    /// Acrescenta um atributo e devolve o elemento.
    #[must_use]
    pub fn com_atributo(mut self, nome: Nome, valor: impl Into<String>) -> Self {
        self.atributos.push(Atributo::new(nome, valor));
        self
    }

    /// Acrescenta um atributo sem namespace e devolve o elemento.
    #[must_use]
    pub fn com_atributo_texto(self, nome: &str, valor: impl Into<String>) -> Self {
        self.com_atributo(Nome::new(nome), valor)
    }

    /// Declara um namespace (`None` para o padrão) e devolve o elemento.
    #[must_use]
    pub fn com_namespace(mut self, prefixo: Option<&str>, uri: impl Into<String>) -> Self {
        let nome = match prefixo {
            Some(prefixo) => Nome::com_prefixo("xmlns", prefixo),
            None => Nome::new("xmlns"),
        };
        self.atributos.push(Atributo::new(nome, uri));
        self
    }

    /// Acrescenta um filho e devolve o elemento.
    #[must_use]
    pub fn com_filho(mut self, filho: No) -> Self {
        self.filhos.push(filho);
        self
    }

    /// Acrescenta texto e devolve o elemento.
    #[must_use]
    pub fn com_texto(mut self, texto: impl Into<String>) -> Self {
        self.filhos.push(No::Texto(texto.into()));
        self
    }

    /// Valor de um atributo sem namespace, se existir.
    pub fn atributo(&self, local: &str) -> Option<&str> {
        self.atributos
            .iter()
            .find(|a| a.nome.prefixo().is_none() && a.nome.local() == local)
            .map(Atributo::valor)
    }

    /// Concatenação dos textos diretamente filhos deste elemento.
    pub fn texto(&self) -> String {
        let mut saida = String::new();
        for filho in &self.filhos {
            if let No::Texto(texto) = filho {
                saida.push_str(texto);
            }
        }
        saida
    }

    /// Busca o primeiro elemento filho com a parte local informada.
    pub fn primeiro_filho(&self, local: &str) -> Option<&Elemento> {
        self.filhos.iter().find_map(|filho| match filho {
            No::Elemento(elemento) if elemento.nome.local() == local => Some(elemento),
            _ => None,
        })
    }

    /// Declarações de namespace presentes neste elemento.
    pub(crate) fn declaracoes_namespace(&self) -> impl Iterator<Item = (Option<&str>, &str)> + '_ {
        self.atributos
            .iter()
            .filter(|a| a.eh_declaracao_namespace())
            .map(|a| (a.prefixo_declarado(), a.valor()))
    }
}

/// Versão e modo standalone da declaração XML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaracao {
    /// Versão declarada (o MVP aceita apenas `1.0`).
    pub versao: String,
    /// `standalone` declarado, se houver.
    pub standalone: Option<bool>,
}

impl Default for Declaracao {
    fn default() -> Self {
        Self {
            versao: "1.0".to_owned(),
            standalone: None,
        }
    }
}

/// Documento XML com exatamente um elemento raiz.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Documento {
    pub(crate) declaracao: Declaracao,
    pub(crate) antes: Vec<No>,
    pub(crate) raiz: Elemento,
    pub(crate) depois: Vec<No>,
}

impl Documento {
    /// Cria um documento com a raiz informada.
    pub fn new(raiz: Elemento) -> Self {
        Self {
            declaracao: Declaracao::default(),
            antes: Vec::new(),
            raiz,
            depois: Vec::new(),
        }
    }

    /// Lê um documento UTF-8 com os limites padrão.
    pub fn parse(xml: &[u8]) -> Result<Self, ErroXml> {
        crate::leitura::ler(xml)
    }

    /// Lê um documento UTF-8 com limites explícitos.
    pub fn parse_com_limites(xml: &[u8], limites: &Limites) -> Result<Self, ErroXml> {
        crate::leitura::ler_com_limites(xml, limites)
    }

    /// Declaração XML normalizada.
    pub fn declaracao(&self) -> &Declaracao {
        &self.declaracao
    }

    /// Elemento raiz.
    pub fn raiz(&self) -> &Elemento {
        &self.raiz
    }

    /// Elemento raiz mutável.
    pub fn raiz_mut(&mut self) -> &mut Elemento {
        &mut self.raiz
    }

    /// Nós antes da raiz (PI e comentários).
    pub fn antes_da_raiz(&self) -> &[No] {
        &self.antes
    }

    /// Nós depois da raiz (PI e comentários).
    pub fn depois_da_raiz(&self) -> &[No] {
        &self.depois
    }

    /// Serializa em UTF-8 de forma determinística.
    pub fn to_xml(&self) -> Result<String, ErroXml> {
        crate::escrita::escrever(self)
    }

    /// Gera a forma canônica C14N 1.0 inclusiva sem comentários.
    pub fn canonicalizar(&self) -> Result<String, ErroXml> {
        crate::canonico::canonicalizar(self)
    }

    /// Valida contra um XSD, resolvendo `xs:include`/`xs:import` no diretório.
    #[cfg(feature = "xsd")]
    pub fn validar_xsd(&self, esquema: impl AsRef<std::path::Path>) -> Result<(), ErroXml> {
        let xml = self.to_xml()?;
        crate::xsd::validar(xml.as_bytes(), esquema)
    }
}

/// Limites de consumo aplicados durante a leitura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limites {
    /// Tamanho máximo da entrada em bytes.
    pub bytes_max: usize,
    /// Profundidade máxima de elementos aninhados.
    pub profundidade_max: usize,
    /// Quantidade máxima de elementos no documento.
    pub elementos_max: usize,
    /// Quantidade máxima de atributos por elemento.
    pub atributos_por_elemento_max: usize,
}

impl Default for Limites {
    fn default() -> Self {
        Self {
            bytes_max: 8 * 1024 * 1024,
            profundidade_max: 128,
            elementos_max: 100_000,
            atributos_por_elemento_max: 1_000,
        }
    }
}

impl Limites {
    /// Limites padrão.
    pub fn padrao() -> Self {
        Self::default()
    }
}
