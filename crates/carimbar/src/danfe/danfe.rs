//! Extração de dados e montagem do DANFE.

use crate::danfe::danfse::{PessoaDanfse, evento_cancela};
use crate::danfe::erro::ErroDanfe;
use crate::xml::{Documento, Elemento};
use printpdf::{
    BuiltinFont, Color, Mm, Op, PdfDocument, PdfFontHandle, PdfPage, PdfSaveOptions, Point, Pt,
    Rect, Rgb, TextItem,
};

/// Item exibido no DANFE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDanfe {
    /// Código.
    pub codigo: String,
    /// Descrição.
    pub descricao: String,
    /// NCM.
    pub ncm: String,
    /// CFOP.
    pub cfop: String,
    /// Unidade.
    pub unidade: String,
    /// Quantidade.
    pub quantidade: String,
    /// Valor unitário.
    pub valor_unitario: String,
    /// Valor total.
    pub valor_total: String,
}

/// Totais da NF-e.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotaisDanfe {
    /// Base do ICMS.
    pub v_bc: String,
    /// Valor do ICMS.
    pub v_icms: String,
    /// Valor dos produtos.
    pub v_prod: String,
    /// Frete.
    pub v_frete: String,
    /// Seguro.
    pub v_seg: String,
    /// Desconto.
    pub v_desc: String,
    /// IPI.
    pub v_ipi: String,
    /// PIS.
    pub v_pis: String,
    /// COFINS.
    pub v_cofins: String,
    /// Valor total.
    pub v_nf: String,
    /// Total aproximado de tributos.
    pub v_tot_trib: Option<String>,
}

/// Dados extraídos da NF-e autorizada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DadosDanfe {
    /// Chave.
    pub chave: String,
    /// Protocolo.
    pub protocolo: String,
    /// Número.
    pub numero: String,
    /// Série.
    pub serie: String,
    /// Data/hora de emissão.
    pub dh_emi: String,
    /// Ambiente.
    pub tp_amb: String,
    /// Forma de emissão.
    pub tp_emis: String,
    /// Natureza da operação.
    pub nat_op: String,
    /// Emitente.
    pub emitente: PessoaDanfse,
    /// Destinatário.
    pub destinatario: Option<PessoaDanfse>,
    /// Itens.
    pub itens: Vec<ItemDanfe>,
    /// Totais.
    pub totais: TotaisDanfe,
    /// Modalidade do frete.
    pub transporte: String,
    /// Forma de pagamento.
    pub pagamento: String,
    /// Informações complementares.
    pub inf_cpl: Option<String>,
    /// Cancelada.
    pub cancelada: bool,
}

/// Opções de geração.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OpcoesDanfe {
    /// Marca o documento como cancelado.
    pub cancelada: bool,
}

/// Extrai os dados da NF-e autorizada (`nfeProc`).
pub fn extrair_dados_nfe(xml: &str) -> Result<DadosDanfe, ErroDanfe> {
    let documento = Documento::parse(xml.as_bytes())?;
    let raiz = documento.raiz();
    let (nfe, protocolo) = if raiz.nome().local() == "nfeProc" {
        let nfe = raiz
            .primeiro_filho("NFe")
            .ok_or_else(|| ErroDanfe::NaoAutorizada("nfeProc sem NFe".to_owned()))?;
        let prot = raiz
            .primeiro_filho("protNFe")
            .ok_or_else(|| ErroDanfe::NaoAutorizada("nfeProc sem protNFe".to_owned()))?;
        let inf_prot = prot
            .primeiro_filho("infProt")
            .ok_or_else(|| ErroDanfe::NaoAutorizada("protNFe sem infProt".to_owned()))?;
        let cstat = texto(inf_prot, "cStat").unwrap_or_default();
        if !matches!(cstat.as_str(), "100" | "101") {
            return Err(ErroDanfe::NaoAutorizada(format!(
                "cStat {cstat} não indica NF-e autorizada"
            )));
        }
        (nfe, Some(inf_prot))
    } else {
        return Err(ErroDanfe::NaoAutorizada(
            "XML sem nfeProc/protocolo de autorização".to_owned(),
        ));
    };
    let inf = nfe
        .primeiro_filho("infNFe")
        .ok_or(ErroDanfe::CampoAusente("infNFe"))?;
    let chave = inf
        .atributo("Id")
        .and_then(|id| id.strip_prefix("NFe"))
        .ok_or(ErroDanfe::CampoAusente("infNFe/@Id"))?
        .to_owned();
    let ide = inf
        .primeiro_filho("ide")
        .ok_or(ErroDanfe::CampoAusente("ide"))?;
    let emit = inf
        .primeiro_filho("emit")
        .ok_or(ErroDanfe::CampoAusente("emit"))?;
    let total = inf
        .primeiro_filho("total")
        .and_then(|total| total.primeiro_filho("ICMSTot"))
        .ok_or(ErroDanfe::CampoAusente("total/ICMSTot"))?;
    let cstat_protocolo = protocolo
        .and_then(|inf| texto(inf, "cStat"))
        .unwrap_or_default();
    let cancelada = cstat_protocolo == "101"
        || texto(inf, "xOutInf").is_some_and(|texto| texto.to_ascii_uppercase().contains("CANCEL"));
    let itens = inf
        .filhos()
        .iter()
        .filter_map(|filho| match filho {
            crate::xml::No::Elemento(elemento) if elemento.nome().local() == "det" => {
                Some(elemento)
            }
            _ => None,
        })
        .map(item)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(DadosDanfe {
        chave,
        protocolo: protocolo
            .and_then(|inf| texto(inf, "nProt"))
            .unwrap_or_default(),
        numero: obrigatorio(ide, "nNF")?,
        serie: obrigatorio(ide, "serie")?,
        dh_emi: obrigatorio(ide, "dhEmi")?,
        tp_amb: obrigatorio(ide, "tpAmb")?,
        tp_emis: obrigatorio(ide, "tpEmis")?,
        nat_op: obrigatorio(ide, "natOp")?,
        emitente: pessoa(emit, true)?,
        destinatario: inf
            .primeiro_filho("dest")
            .map(|dest| pessoa(dest, false))
            .transpose()?,
        itens,
        totais: TotaisDanfe {
            v_bc: valor(total, "vBC"),
            v_icms: valor(total, "vICMS"),
            v_prod: valor(total, "vProd"),
            v_frete: valor(total, "vFrete"),
            v_seg: valor(total, "vSeg"),
            v_desc: valor(total, "vDesc"),
            v_ipi: valor(total, "vIPI"),
            v_pis: valor(total, "vPIS"),
            v_cofins: valor(total, "vCOFINS"),
            v_nf: valor(total, "vNF"),
            v_tot_trib: texto(total, "vTotTrib"),
        },
        transporte: caminho_texto(inf, &["transp", "modFrete"]).unwrap_or_default(),
        pagamento: caminho_texto(inf, &["pag", "detPag", "tPag"]).unwrap_or_default(),
        inf_cpl: caminho_texto(inf, &["infAdic", "infCpl"]),
        cancelada,
    })
}

fn item(det: &Elemento) -> Result<ItemDanfe, ErroDanfe> {
    let prod = det
        .primeiro_filho("prod")
        .ok_or(ErroDanfe::CampoAusente("det/prod"))?;
    Ok(ItemDanfe {
        codigo: valor(prod, "cProd"),
        descricao: obrigatorio(prod, "xProd")?,
        ncm: valor(prod, "NCM"),
        cfop: valor(prod, "CFOP"),
        unidade: valor(prod, "uCom"),
        quantidade: valor(prod, "qCom"),
        valor_unitario: valor(prod, "vUnCom"),
        valor_total: valor(prod, "vProd"),
    })
}

/// Gera o PDF do DANFE.
pub fn gerar_danfe(xml: &str, opcoes: &OpcoesDanfe) -> Result<Vec<u8>, ErroDanfe> {
    let mut dados = extrair_dados_nfe(xml)?;
    dados.cancelada |= opcoes.cancelada;
    let mut desenhista = Desenhista::novo();
    desenhista.cabecalho(&dados)?;
    desenhista.secoes(&dados)?;
    let paginas = desenhista.finalizar();
    let mut documento = PdfDocument::new("DANFE");
    for ops in paginas {
        documento
            .pages
            .push(PdfPage::new(Mm(210.0), Mm(297.0), ops));
    }
    let mut avisos = Vec::new();
    Ok(documento.save(&PdfSaveOptions::default(), &mut avisos))
}

/// `true` quando o XML de evento é de cancelamento.
pub fn evento_cancela_nfe(xml: &str) -> bool {
    evento_cancela(xml)
}

fn pessoa(elemento: &Elemento, emitente: bool) -> Result<PessoaDanfse, ErroDanfe> {
    let inscricao = texto(elemento, "CNPJ")
        .or_else(|| texto(elemento, "CPF"))
        .ok_or(ErroDanfe::CampoAusente(if emitente {
            "emit/CNPJ"
        } else {
            "dest/CNPJ"
        }))?;
    let endereco = elemento
        .primeiro_filho(if emitente { "enderEmit" } else { "enderDest" })
        .map(|endereco| {
            [
                texto(endereco, "xLgr"),
                texto(endereco, "nro"),
                texto(endereco, "xBairro"),
                texto(endereco, "xMun"),
                texto(endereco, "UF"),
                texto(endereco, "CEP"),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ")
        });
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

fn valor(elemento: &Elemento, nome: &str) -> String {
    texto(elemento, nome).unwrap_or_else(|| "0.00".to_owned())
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
    fn novo() -> Self {
        Self {
            paginas: Vec::new(),
            ops: Vec::new(),
            y: 0.0,
        }
    }

    fn cabecalho(&mut self, dados: &DadosDanfe) -> Result<(), ErroDanfe> {
        self.ops.extend(op_texto(15.0, 284.0, 14.0, true, "DANFE"));
        self.ops.extend(op_texto(
            15.0,
            278.0,
            7.5,
            false,
            "Documento Auxiliar da Nota Fiscal Eletrônica",
        ));
        self.ops
            .extend(op_texto(15.0, 273.0, 7.0, false, "0 - Entrada / 1 - Saída"));
        self.ops.extend(op_texto(15.0, 267.0, 7.0, true, "Número"));
        self.ops
            .extend(op_texto(40.0, 267.0, 7.0, false, &dados.numero));
        self.ops.extend(op_texto(65.0, 267.0, 7.0, true, "Série"));
        self.ops
            .extend(op_texto(80.0, 267.0, 7.0, false, &dados.serie));
        self.ops.extend(op_texto(105.0, 267.0, 7.0, true, "Folha"));
        self.ops.extend(op_texto(120.0, 267.0, 7.0, false, "1/1"));
        self.ops
            .extend(op_texto(15.0, 261.0, 7.0, true, "Chave de acesso"));
        self.ops
            .extend(op_texto(55.0, 261.0, 7.0, false, &dados.chave));
        self.ops
            .extend(op_texto(15.0, 255.0, 7.0, true, "Protocolo de autorização"));
        self.ops
            .extend(op_texto(65.0, 255.0, 7.0, false, &dados.protocolo));
        self.ops.extend(op_linha(15.0, 252.0, 180.0));

        if dados.tp_amb == "2" {
            self.ops.extend(op_texto(
                15.0,
                246.0,
                8.0,
                true,
                "HOMOLOGAÇÃO - SEM VALOR FISCAL",
            ));
        }
        if dados.tp_emis != "1" {
            self.ops
                .extend(op_texto(110.0, 246.0, 8.0, true, "CONTINGÊNCIA"));
        }
        if dados.cancelada {
            self.ops
                .extend(op_texto(150.0, 246.0, 9.0, true, "CANCELADA"));
        }
        self.y = 238.0;
        self.codigo_barras(&dados.chave)?;
        Ok(())
    }

    fn codigo_barras(&mut self, chave: &str) -> Result<(), ErroDanfe> {
        let codigo = barcoders::sym::code128::Code128::new(format!("\u{0106}{chave}"))
            .map_err(|erro| ErroDanfe::Qr(erro.to_string()))?;
        let bits = codigo.encode();
        let largura = 0.22_f32;
        let altura = 12.0_f32;
        let x0 = 15.0_f32;
        let y0 = self.y - altura;
        self.ops.push(Op::SetFillColor {
            col: Color::Rgb(Rgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                icc_profile: None,
            }),
        });
        for (indice, bit) in bits.iter().enumerate() {
            if *bit == 1 {
                let x = x0 + indice as f32 * largura;
                self.ops.push(Op::DrawRectangle {
                    rectangle: Rect::from_xywh(
                        Pt::from(Mm(x)),
                        Pt::from(Mm(y0)),
                        Pt::from(Mm(largura)),
                        Pt::from(Mm(altura)),
                    ),
                });
            }
        }
        self.y -= altura + 4.0;
        Ok(())
    }

    fn secoes(&mut self, dados: &DadosDanfe) -> Result<(), ErroDanfe> {
        self.titulo("Emitente");
        self.pessoa(&dados.emitente);
        if let Some(destinatario) = &dados.destinatario {
            self.titulo("Destinatário / Remetente");
            self.pessoa(destinatario);
        }

        self.titulo("Dados da operação");
        self.campo("Natureza da operação", &dados.nat_op);
        self.campo("Emissão", &dados.dh_emi);
        self.campo("Transporte", &dados.transporte);
        self.campo("Pagamento", &dados.pagamento);

        self.titulo("Itens");
        self.cabecalho_itens();
        for item in &dados.itens {
            self.item(item);
        }

        self.titulo("Totais");
        self.totais(&dados.totais);
        if let Some(inf_cpl) = &dados.inf_cpl {
            self.titulo("Informações complementares");
            for linha in quebrar_texto(inf_cpl, 95) {
                self.linha("", &linha, false);
            }
        }
        Ok(())
    }

    fn cabecalho_itens(&mut self) {
        self.garantir_espaco(10.0);
        let mut ops = Vec::new();
        for (x, texto) in [
            (15.0, "Código"),
            (38.0, "Descrição"),
            (105.0, "NCM"),
            (125.0, "CFOP"),
            (140.0, "Un"),
            (150.0, "Qtd"),
            (165.0, "V.Unit"),
            (182.0, "V.Total"),
        ] {
            ops.extend(op_texto(x, self.y, 6.5, true, texto));
        }
        self.ops.extend(ops);
        self.y -= 4.0;
    }

    fn item(&mut self, item: &ItemDanfe) {
        let linhas = quebrar_texto(&item.descricao, 32);
        let altura = 4.0 + linhas.len() as f32 * 4.0;
        self.garantir_espaco(altura);
        self.ops
            .extend(op_texto(15.0, self.y, 6.2, false, &item.codigo));
        for (indice, linha) in linhas.iter().enumerate() {
            self.ops.extend(op_texto(
                38.0,
                self.y - indice as f32 * 4.0,
                6.2,
                false,
                linha,
            ));
        }
        let y = self.y;
        for (x, texto) in [
            (105.0, &item.ncm),
            (125.0, &item.cfop),
            (140.0, &item.unidade),
            (150.0, &item.quantidade),
            (165.0, &item.valor_unitario),
            (182.0, &item.valor_total),
        ] {
            self.ops.extend(op_texto(x, y, 6.2, false, texto));
        }
        self.y -= altura;
        self.ops.extend(op_linha(15.0, self.y + 1.0, 180.0));
    }

    fn totais(&mut self, totais: &TotaisDanfe) {
        let pares = [
            ("Base do ICMS", &totais.v_bc),
            ("Valor do ICMS", &totais.v_icms),
            ("Valor dos produtos", &totais.v_prod),
            ("Frete", &totais.v_frete),
            ("Seguro", &totais.v_seg),
            ("Desconto", &totais.v_desc),
            ("IPI", &totais.v_ipi),
            ("PIS", &totais.v_pis),
            ("COFINS", &totais.v_cofins),
            ("Valor total da NF-e", &totais.v_nf),
        ];
        for (rotulo, valor) in pares {
            self.campo(rotulo, valor);
        }
        if let Some(v_tot_trib) = &totais.v_tot_trib {
            self.campo("Total aproximado de tributos", v_tot_trib);
        }
    }

    fn titulo(&mut self, titulo: &str) {
        self.garantir_espaco(12.0);
        self.y -= 2.0;
        self.ops.extend(op_texto(15.0, self.y, 8.0, true, titulo));
        self.y -= 5.0;
    }

    fn campo(&mut self, rotulo: &str, valor: &str) {
        self.linha(rotulo, valor, false);
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
            for linha in quebrar_texto(endereco, 90) {
                self.linha("Endereço", &linha, false);
            }
        }
    }

    fn linha(&mut self, rotulo: &str, valor: &str, negrito: bool) {
        self.garantir_espaco(5.0);
        self.ops.extend(op_texto(15.0, self.y, 6.5, true, rotulo));
        self.ops.extend(op_texto(62.0, self.y, 6.5, negrito, valor));
        self.y -= 4.5;
    }

    fn garantir_espaco(&mut self, necessario: f32) {
        if self.y - necessario < 18.0 {
            let anterior = std::mem::take(&mut self.ops);
            self.paginas.push(anterior);
            self.ops
                .extend(op_texto(15.0, 282.0, 10.0, true, "DANFE - continuação"));
            self.ops.extend(op_linha(15.0, 278.0, 180.0));
            self.y = 272.0;
        }
    }

    fn finalizar(mut self) -> Vec<Vec<Op>> {
        self.paginas.push(self.ops);
        let total = self.paginas.len();
        for (indice, ops) in self.paginas.iter_mut().enumerate() {
            ops.extend(op_linha(15.0, 14.0, 180.0));
            ops.extend(op_texto(
                15.0,
                10.0,
                6.0,
                false,
                "Gerado localmente a partir da NF-e autorizada.",
            ));
            ops.extend(op_texto(
                175.0,
                10.0,
                6.0,
                true,
                &format!("Folha {}/{}", indice + 1, total),
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
