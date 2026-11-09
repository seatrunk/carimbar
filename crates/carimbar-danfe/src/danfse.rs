//! Extração de dados e montagem do PDF do DANFSe.

use crate::erro::ErroDanfe;
use carimbar_xml::{Documento, Elemento};
use printpdf::{
    BuiltinFont, Color, Mm, Op, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt,
    Rect, Rgb, TextItem,
};
use qrcode::{Color as QrColor, QrCode};

/// Pessoa exibida no DANFSe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PessoaDanfse {
    /// CNPJ ou CPF.
    pub inscricao: String,
    /// Inscrição municipal.
    pub inscricao_municipal: Option<String>,
    /// Nome ou razão social.
    pub nome: String,
    /// Endereço em uma linha.
    pub endereco: Option<String>,
}

/// Dados extraídos da NFS-e autorizada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DadosDanfse {
    /// Chave de acesso de 50 posições.
    pub chave: String,
    /// Número da NFS-e.
    pub numero: String,
    /// Data/hora de processamento.
    pub processamento: String,
    /// Município emissor.
    pub municipio_emissor: String,
    /// Local da prestação.
    pub local_prestacao: String,
    /// `1` produção, `2` homologação.
    pub ambiente: String,
    /// Emitente.
    pub emitente: PessoaDanfse,
    /// Tomador, quando informado.
    pub tomador: Option<PessoaDanfse>,
    /// Descrição do serviço.
    pub descricao_servico: String,
    /// Código de tributação nacional.
    pub codigo_tributacao: Option<String>,
    /// Valor do serviço.
    pub valor_servico: String,
    /// Base de cálculo.
    pub base_calculo: Option<String>,
    /// Alíquota aplicada.
    pub aliquota: Option<String>,
    /// Valor do ISSQN.
    pub valor_issqn: Option<String>,
    /// Valor líquido.
    pub valor_liquido: String,
    /// Código de tributação do ISSQN.
    pub tributacao_issqn: Option<String>,
    /// `true` quando a NFS-e está cancelada.
    pub cancelada: bool,
}

/// Opções de geração.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpcoesDanfse {
    /// Marca o documento como cancelado.
    pub cancelada: bool,
}

/// Extrai os dados da NFS-e autorizada.
pub fn extrair_dados(xml: &str) -> Result<DadosDanfse, ErroDanfe> {
    let documento = Documento::parse(xml.as_bytes())?;
    if documento.raiz().nome().local() != "NFSe" {
        return Err(ErroDanfe::NaoAutorizada(
            "raiz diferente de NFSe (DPS isolada?)".to_owned(),
        ));
    }
    let inf = documento
        .raiz()
        .primeiro_filho("infNFSe")
        .ok_or_else(|| ErroDanfe::NaoAutorizada("infNFSe ausente".to_owned()))?;
    let cstat = texto(inf, "cStat").unwrap_or_default();
    if !matches!(cstat.as_str(), "100" | "101") {
        return Err(ErroDanfe::NaoAutorizada(format!(
            "cStat {cstat} não indica NFS-e autorizada"
        )));
    }
    let chave = inf
        .atributo("Id")
        .and_then(|id| id.strip_prefix("NFS").or(Some(id)))
        .ok_or(ErroDanfe::CampoAusente("infNFSe/@Id"))?
        .to_owned();
    let dps = inf
        .primeiro_filho("DPS")
        .ok_or(ErroDanfe::CampoAusente("DPS"))?;
    let inf_dps = dps
        .primeiro_filho("infDPS")
        .ok_or(ErroDanfe::CampoAusente("DPS/infDPS"))?;
    let emit = inf
        .primeiro_filho("emit")
        .ok_or(ErroDanfe::CampoAusente("emit"))?;
    let valores = inf
        .primeiro_filho("valores")
        .ok_or(ErroDanfe::CampoAusente("valores"))?;
    let cancelada = cstat == "101"
        || texto(inf, "xOutInf").is_some_and(|texto| texto.to_ascii_uppercase().contains("CANCEL"));

    Ok(DadosDanfse {
        chave,
        numero: obrigatorio(inf, "nNFSe")?,
        processamento: obrigatorio(inf, "dhProc")?,
        municipio_emissor: obrigatorio(inf, "xLocEmi")?,
        local_prestacao: obrigatorio(inf, "xLocPrestacao")?,
        ambiente: inf_dps
            .primeiro_filho("tpAmb")
            .map(Elemento::texto)
            .unwrap_or_else(|| "2".to_owned()),
        emitente: pessoa(emit, true)?,
        tomador: inf_dps
            .primeiro_filho("toma")
            .map(|toma| pessoa(toma, false))
            .transpose()?,
        descricao_servico: caminho_texto(inf, &["DPS", "infDPS", "serv", "cServ", "xDescServ"])
            .ok_or(ErroDanfe::CampoAusente("DPS/infDPS/serv/cServ/xDescServ"))?,
        codigo_tributacao: caminho_texto(inf, &["DPS", "infDPS", "serv", "cServ", "cTribNac"]),
        valor_servico: caminho_texto(inf, &["DPS", "infDPS", "valores", "vServPrest", "vServ"])
            .ok_or(ErroDanfe::CampoAusente(
                "DPS/infDPS/valores/vServPrest/vServ",
            ))?,
        base_calculo: texto(valores, "vBC"),
        aliquota: texto(valores, "pAliqAplic"),
        valor_issqn: texto(valores, "vISSQN"),
        valor_liquido: obrigatorio(valores, "vLiq")?,
        tributacao_issqn: caminho_texto(
            inf,
            &["DPS", "infDPS", "valores", "trib", "tribMun", "tribISSQN"],
        ),
        cancelada,
    })
}

/// `true` quando o XML de evento é de cancelamento.
pub fn evento_cancela(xml: &str) -> bool {
    let Ok(documento) = Documento::parse(xml.as_bytes()) else {
        return false;
    };
    !documento.caminhos_por_local("e101101").is_empty()
        || !documento.caminhos_por_local("e105102").is_empty()
}

/// Gera o PDF.
pub fn gerar(xml: &str, opcoes: &OpcoesDanfse) -> Result<Vec<u8>, ErroDanfe> {
    let mut dados = extrair_dados(xml)?;
    dados.cancelada |= opcoes.cancelada;
    let mut desenhista = Desenhista::novo(&dados);
    desenhista.secoes(&dados)?;
    let paginas = desenhista.finalizar();
    let mut documento = PdfDocument::new("DANFSe");
    for ops in paginas {
        documento
            .pages
            .push(PdfPage::new(Mm(210.0), Mm(297.0), ops));
    }
    let mut avisos = Vec::new();
    Ok(documento.save(&PdfSaveOptions::default(), &mut avisos))
}

fn pessoa(elemento: &Elemento, emitente: bool) -> Result<PessoaDanfse, ErroDanfe> {
    let inscricao = texto(elemento, "CNPJ")
        .or_else(|| texto(elemento, "CPF"))
        .ok_or(ErroDanfe::CampoAusente(if emitente {
            "emit/CNPJ"
        } else {
            "toma/CNPJ"
        }))?;
    let endereco = if emitente {
        elemento.primeiro_filho("enderNac").map(|endereco| {
            [
                texto(endereco, "xLgr"),
                texto(endereco, "nro"),
                texto(endereco, "xBairro"),
                texto(endereco, "cMun"),
                texto(endereco, "UF"),
                texto(endereco, "CEP"),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ")
        })
    } else {
        None
    };
    Ok(PessoaDanfse {
        inscricao,
        inscricao_municipal: texto(elemento, "IM"),
        nome: texto(elemento, "xNome").unwrap_or_default(),
        endereco,
    })
}

fn caminho_texto(elemento: &Elemento, caminho: &[&str]) -> Option<String> {
    let mut atual = elemento;
    for nome in caminho {
        atual = atual.primeiro_filho(nome)?;
    }
    let valor = atual.texto();
    (!valor.is_empty()).then_some(valor)
}

fn obrigatorio(elemento: &Elemento, nome: &'static str) -> Result<String, ErroDanfe> {
    texto(elemento, nome).ok_or(ErroDanfe::CampoAusente(nome))
}

fn texto(elemento: &Elemento, nome: &str) -> Option<String> {
    elemento
        .primeiro_filho(nome)
        .map(Elemento::texto)
        .filter(|valor| !valor.is_empty())
}

struct Desenhista {
    paginas: Vec<Vec<Op>>,
    ops: Vec<Op>,
    y: f32,
}

impl Desenhista {
    fn novo(_dados: &DadosDanfse) -> Self {
        let mut desenhista = Self {
            paginas: Vec::new(),
            ops: Vec::new(),
            y: 0.0,
        };
        desenhista.cabecalho();
        desenhista
    }

    fn cabecalho(&mut self) {
        self.ops.extend(op_texto(20.0, 282.0, 14.0, true, "DANFSe"));
        self.ops.extend(op_texto(
            20.0,
            276.0,
            8.0,
            false,
            "Documento Auxiliar da NFS-e",
        ));
        self.ops.extend(op_linha(20.0, 272.0, 170.0));
        self.y = 262.0;
    }

    fn nova_pagina(&mut self) {
        let anterior = std::mem::take(&mut self.ops);
        self.paginas.push(anterior);
        self.cabecalho();
    }

    fn secoes(&mut self, dados: &DadosDanfse) -> Result<(), ErroDanfe> {
        if dados.ambiente == "2" {
            self.ops
                .extend(op_texto(20.0, 266.0, 8.0, true, "AMBIENTE DE HOMOLOGAÇÃO"));
        }
        if dados.cancelada {
            self.ops
                .extend(op_texto(150.0, 266.0, 10.0, true, "CANCELADA"));
        }
        self.titulo("NFS-e");
        self.campo("Número", &dados.numero);
        self.campo("Chave de acesso", &dados.chave);
        self.campo("Processamento", &dados.processamento);
        self.campo("Município emissor", &dados.municipio_emissor);
        self.campo("Local da prestação", &dados.local_prestacao);

        self.titulo("Prestador de serviços");
        self.pessoa(&dados.emitente);
        if let Some(tomador) = &dados.tomador {
            self.titulo("Tomador de serviços");
            self.pessoa(tomador);
        }

        self.titulo("Serviço prestado");
        if let Some(codigo) = &dados.codigo_tributacao {
            self.campo("Código de tributação", codigo);
        }
        for linha in quebrar_texto(&dados.descricao_servico, 88) {
            self.linha("Descrição", &linha, false);
        }

        self.titulo("Valores");
        self.campo("Valor do serviço", &dados.valor_servico);
        if let Some(base) = &dados.base_calculo {
            self.campo("Base de cálculo", base);
        }
        if let Some(aliquota) = &dados.aliquota {
            self.campo("Alíquota", &format!("{aliquota}%"));
        }
        if let Some(issqn) = &dados.valor_issqn {
            self.campo("ISSQN", issqn);
        }
        if let Some(tributacao) = &dados.tributacao_issqn {
            self.campo("Tributação do ISSQN", tributacao);
        }
        self.campo("Valor líquido", &dados.valor_liquido);

        self.titulo("Consulta");
        self.qr(&dados.chave)?;
        Ok(())
    }

    fn titulo(&mut self, titulo: &str) {
        self.garantir_espaco(14.0);
        self.y -= 4.0;
        self.ops.extend(op_texto(20.0, self.y, 9.0, true, titulo));
        self.y -= 5.0;
    }

    fn campo(&mut self, rotulo: &str, valor: &str) {
        self.linha(rotulo, valor, true);
    }

    fn pessoa(&mut self, pessoa: &PessoaDanfse) {
        self.campo("Inscrição", &pessoa.inscricao);
        if let Some(im) = &pessoa.inscricao_municipal {
            self.campo("Inscrição municipal", im);
        }
        if !pessoa.nome.is_empty() {
            self.campo("Nome", &pessoa.nome);
        }
        if let Some(endereco) = &pessoa.endereco {
            for linha in quebrar_texto(endereco, 88) {
                self.linha("Endereço", &linha, false);
            }
        }
    }

    fn linha(&mut self, rotulo: &str, valor: &str, negrito: bool) {
        self.garantir_espaco(6.0);
        self.ops.extend(op_texto(20.0, self.y, 7.5, true, rotulo));
        self.ops.extend(op_texto(62.0, self.y, 7.5, negrito, valor));
        self.y -= 5.5;
    }

    fn garantir_espaco(&mut self, necessario: f32) {
        if self.y - necessario < 20.0 {
            self.nova_pagina();
        }
    }

    fn qr(&mut self, chave: &str) -> Result<(), ErroDanfe> {
        let url = format!("https://www.nfse.gov.br/consultapublica?chave={chave}");
        let codigo = QrCode::new(url.as_bytes()).map_err(|erro| ErroDanfe::Qr(erro.to_string()))?;
        let largura = codigo.width();
        let cores = codigo.to_colors();
        let tamanho = 34.0 / f32::from(u16::try_from(largura).unwrap_or(u16::MAX));
        let x0 = 20.0;
        let y0 = self.y - 35.0;
        self.ops.push(Op::SetFillColor {
            col: Color::Rgb(Rgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                icc_profile: None,
            }),
        });
        for (indice, cor) in cores.iter().enumerate() {
            if *cor == QrColor::Dark {
                let coluna = f32::from(u16::try_from(indice % largura).unwrap_or(u16::MAX));
                let linha = f32::from(u16::try_from(indice / largura).unwrap_or(u16::MAX));
                let x = x0 + coluna * tamanho;
                let y = y0 + 34.0 - (linha + 1.0) * tamanho;
                self.ops.push(Op::DrawRectangle {
                    rectangle: Rect::from_xywh(
                        Pt::from(Mm(x)),
                        Pt::from(Mm(y)),
                        Pt::from(Mm(tamanho)),
                        Pt::from(Mm(tamanho)),
                    ),
                });
            }
        }
        self.ops.extend(op_texto(
            60.0,
            self.y - 4.0,
            7.0,
            false,
            "Consulte pela chave de acesso",
        ));
        self.ops
            .extend(op_texto(60.0, self.y - 10.0, 7.0, true, chave));
        self.y -= 40.0;
        Ok(())
    }

    fn finalizar(mut self) -> Vec<Vec<Op>> {
        self.paginas.push(self.ops);
        let total = self.paginas.len();
        for (indice, ops) in self.paginas.iter_mut().enumerate() {
            ops.extend(op_linha(20.0, 16.0, 170.0));
            ops.extend(op_texto(
                20.0,
                12.0,
                6.5,
                false,
                "Gerado localmente a partir da NFS-e autorizada.",
            ));
            ops.extend(op_texto(
                150.0,
                12.0,
                6.5,
                true,
                &format!("Página {}/{}", indice + 1, total),
            ));
        }
        self.paginas
    }
}

fn op_texto(x: f32, y: f32, tamanho: f32, negrito: bool, texto: &str) -> Vec<Op> {
    vec![
        Op::StartTextSection,
        Op::SetTextCursor {
            pos: Point::new(Mm(x), Mm(y)),
        },
        Op::SetFont {
            font: PdfFontHandle::Builtin(if negrito {
                BuiltinFont::HelveticaBold
            } else {
                BuiltinFont::Helvetica
            }),
            size: Pt(tamanho),
        },
        Op::SetFillColor {
            col: Color::Rgb(Rgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                icc_profile: None,
            }),
        },
        Op::ShowText {
            items: vec![TextItem::Text(texto.to_owned())],
        },
        Op::EndTextSection,
    ]
}

fn op_linha(x: f32, y: f32, largura: f32) -> Vec<Op> {
    vec![
        Op::SetOutlineColor {
            col: Color::Rgb(Rgb {
                r: 0.6,
                g: 0.6,
                b: 0.6,
                icc_profile: None,
            }),
        },
        Op::SetOutlineThickness { pt: Pt(0.4) },
        Op::DrawLine {
            line: printpdf::Line {
                points: vec![
                    printpdf::LinePoint {
                        p: Point::new(Mm(x), Mm(y)),
                        bezier: false,
                    },
                    printpdf::LinePoint {
                        p: Point::new(Mm(x + largura), Mm(y)),
                        bezier: false,
                    },
                ],
                is_closed: false,
            },
        },
    ]
}

fn quebrar_texto(texto: &str, limite: usize) -> Vec<String> {
    let mut linhas = Vec::new();
    let mut atual = String::new();
    for palavra in texto.split_whitespace() {
        if !atual.is_empty() && atual.chars().count() + 1 + palavra.chars().count() > limite {
            linhas.push(std::mem::take(&mut atual));
        }
        if !atual.is_empty() {
            atual.push(' ');
        }
        atual.push_str(palavra);
    }
    if !atual.is_empty() {
        linhas.push(atual);
    }
    if linhas.is_empty() {
        linhas.push(String::new());
    }
    linhas
}
