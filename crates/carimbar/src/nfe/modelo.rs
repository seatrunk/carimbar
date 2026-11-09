//! Modelo mínimo da NF-e 4.00 e serialização na ordem do XSD.
//!
//! O modelo cobre os grupos obrigatórios de uma operação de venda simples e o
//! grupo IBS/CBS. Grupos opcionais de comércio exterior, combustíveis,
//! importação, exportação, transporte detalhado e agricultura ficam fora do MVP.

use crate::endpoints::Uf;
use crate::nfe::erro::ErroNfe;
use crate::types::{ChaveAcesso, Cnpj, Decimal, Dinheiro, TpAmb};
use crate::xml::{Documento, Elemento, No, Nome};

/// Namespace da NF-e.
pub const NS_NFE: &str = "http://www.portalfiscal.inf.br/nfe";
/// Versão do leiaute.
pub const VERSAO: &str = "4.00";

/// Endereço nacional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endereco {
    /// Logradouro.
    pub logradouro: String,
    /// Número.
    pub numero: String,
    /// Complemento.
    pub complemento: Option<String>,
    /// Bairro.
    pub bairro: String,
    /// Código IBGE do município.
    pub codigo_municipio: String,
    /// Nome do município.
    pub municipio: String,
    /// UF.
    pub uf: Uf,
    /// CEP com 8 dígitos.
    pub cep: String,
}

/// Emitente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emitente {
    /// CNPJ.
    pub cnpj: Cnpj,
    /// Razão social.
    pub nome: String,
    /// Nome fantasia.
    pub fantasia: Option<String>,
    /// Endereço.
    pub endereco: Endereco,
    /// Inscrição estadual.
    pub ie: String,
    /// Código de regime tributário (1, 2, 3 ou 4).
    pub crt: u8,
}

/// Destinatário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destinatario {
    /// CNPJ ou CPF, só dígitos.
    pub inscricao: String,
    /// Nome.
    pub nome: String,
    /// Endereço.
    pub endereco: Endereco,
    /// Indicador da IE (1, 2 ou 9).
    pub ind_ie_dest: u8,
    /// Inscrição estadual, quando contribuinte.
    pub ie: Option<String>,
}

/// Grupo IBS/CBS do item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IbsCbsItem {
    /// CST do IBS/CBS.
    pub cst: String,
    /// Classificação tributária.
    pub c_class_trib: String,
}

/// Item da NF-e.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Código do produto.
    pub codigo: String,
    /// GTIN ou `SEM GTIN`.
    pub ean: String,
    /// Descrição.
    pub descricao: String,
    /// NCM.
    pub ncm: String,
    /// CFOP.
    pub cfop: String,
    /// Unidade comercial.
    pub unidade: String,
    /// Quantidade.
    pub quantidade: Decimal,
    /// Valor unitário.
    pub valor_unitario: Decimal,
    /// Valor total.
    pub valor_total: Dinheiro,
    /// Grupo IBS/CBS.
    pub ibs_cbs: Option<IbsCbsItem>,
}

/// Total de IBS/CBS da nota.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TotalIbsCbs {
    /// Base de cálculo.
    pub v_bc: Dinheiro,
    /// Valor do IBS.
    pub v_ibs: Dinheiro,
    /// Valor da CBS.
    pub v_cbs: Dinheiro,
}

/// Pagamento único.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pagamento {
    /// Código da forma de pagamento (2 dígitos).
    pub tipo: String,
    /// Valor pago.
    pub valor: Dinheiro,
}

/// Nota Fiscal Eletrônica modelo 55.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotaFiscal {
    /// Ambiente.
    pub ambiente: TpAmb,
    /// UF do emitente.
    pub uf: Uf,
    /// Série.
    pub serie: u32,
    /// Número.
    pub numero: u64,
    /// Código numérico de 8 dígitos.
    pub c_nf: String,
    /// Natureza da operação.
    pub nat_op: String,
    /// Data/hora de emissão.
    pub dh_emi: String,
    /// Tipo (0 entrada, 1 saída).
    pub tp_nf: u8,
    /// Destino (1 interna, 2 interestadual, 3 exterior).
    pub id_dest: u8,
    /// Município do fato gerador.
    pub c_mun_fg: String,
    /// Formato de impressão.
    pub tp_imp: u8,
    /// Forma de emissão (1 normal, 6 SVC-AN, 7 SVC-RS...).
    pub tp_emis: u8,
    /// Finalidade (1 normal...).
    pub fin_nfe: u8,
    /// Consumidor final (0/1).
    pub ind_final: u8,
    /// Presença (0,1,2,3,4,5,9).
    pub ind_pres: u8,
    /// Versão do aplicativo.
    pub ver_proc: String,
    /// Data/hora de contingência.
    pub dh_cont: Option<String>,
    /// Justificativa de contingência.
    pub x_just: Option<String>,
    /// Emitente.
    pub emitente: Emitente,
    /// Destinatário.
    pub destinatario: Option<Destinatario>,
    /// Itens.
    pub itens: Vec<Item>,
    /// Modalidade do frete.
    pub mod_frete: u8,
    /// Pagamento.
    pub pagamento: Pagamento,
    /// Informações complementares.
    pub inf_cpl: Option<String>,
    /// Totais de IBS/CBS.
    pub total_ibs_cbs: Option<TotalIbsCbs>,
}

impl NotaFiscal {
    /// Calcula a chave de acesso com cDV.
    pub fn chave(&self) -> Result<ChaveAcesso, ErroNfe> {
        let aamm = format!(
            "{}{}",
            self.dh_emi.get(2..4).unwrap_or("00"),
            self.dh_emi.get(5..7).unwrap_or("01")
        );
        let sem_dv = format!(
            "{}{}{}{:02}{:03}{:09}{}{}",
            self.uf.codigo(),
            aamm,
            self.emitente.cnpj.as_str(),
            55,
            self.serie,
            self.numero,
            self.tp_emis,
            self.c_nf
        );
        ChaveAcesso::com_dv(&sem_dv).map_err(ErroNfe::from)
    }

    /// Valida os campos que não são garantidos pelos tipos.
    pub fn validar(&self) -> Result<(), ErroNfe> {
        let chave = self.chave()?;
        if chave.tp_emis() != char::from(b'0' + self.tp_emis) {
            return Err(ErroNfe::campo("tpEmis", "divergente da chave"));
        }
        exigir_texto("natOp", &self.nat_op, 1, 60)?;
        exigir_texto("verProc", &self.ver_proc, 1, 20)?;
        exigir_digitos("cNF", &self.c_nf, 8)?;
        exigir_digitos("cMunFG", &self.c_mun_fg, 7)?;
        if !(0..=999).contains(&self.serie) {
            return Err(ErroNfe::campo("serie", "fora de 0..=999"));
        }
        if !(1..=999_999_999).contains(&self.numero) {
            return Err(ErroNfe::campo("nNF", "fora de 1..=999999999"));
        }
        if !matches!(self.tp_nf, 0 | 1) {
            return Err(ErroNfe::campo("tpNF", "esperado 0 ou 1"));
        }
        if !matches!(self.id_dest, 1..=3) {
            return Err(ErroNfe::campo("idDest", "esperado 1..=3"));
        }
        if !matches!(self.tp_emis, 1..=7 | 9) {
            return Err(ErroNfe::campo("tpEmis", "esperado 1..=7 ou 9"));
        }
        if self.tp_emis != 1 && (self.dh_cont.is_none() || self.x_just.is_none()) {
            return Err(ErroNfe::campo(
                "contingência",
                "dhCont e xJust são obrigatórios",
            ));
        }
        if let Some(justificativa) = &self.x_just {
            exigir_texto("xJust", justificativa, 15, 256)?;
        }
        validar_endereco("enderEmit", &self.emitente.endereco)?;
        exigir_texto("emit/xNome", &self.emitente.nome, 2, 60)?;
        exigir_texto("emit/IE", &self.emitente.ie, 2, 14)?;
        if !(1..=4).contains(&self.emitente.crt) {
            return Err(ErroNfe::campo("emit/CRT", "esperado 1..=4"));
        }
        if let Some(dest) = &self.destinatario {
            exigir_texto("dest/xNome", &dest.nome, 2, 60)?;
            validar_endereco("enderDest", &dest.endereco)?;
            if !matches!(dest.ind_ie_dest, 1 | 2 | 9) {
                return Err(ErroNfe::campo("dest/indIEDest", "esperado 1, 2 ou 9"));
            }
        }
        if self.itens.is_empty() {
            return Err(ErroNfe::campo("det", "ao menos um item"));
        }
        let mut total = Dinheiro::ZERO;
        for item in &self.itens {
            exigir_texto("det/prod/xProd", &item.descricao, 1, 120)?;
            exigir_digitos("det/prod/NCM", &item.ncm, 8)?;
            exigir_digitos("det/prod/CFOP", &item.cfop, 4)?;
            if item.quantidade <= Decimal::ZERO {
                return Err(ErroNfe::campo("det/prod/qCom", "precisa ser positiva"));
            }
            if item.valor_unitario.is_sign_negative() {
                return Err(ErroNfe::campo("det/prod/vUnCom", "não pode ser negativo"));
            }
            total = total
                .checked_add(item.valor_total)
                .ok_or_else(|| ErroNfe::campo("total", "soma excedida"))?;
            if let Some(ibs) = &item.ibs_cbs {
                exigir_digitos("det/imposto/IBSCBS/CST", &ibs.cst, 3)?;
                exigir_digitos("det/imposto/IBSCBS/cClassTrib", &ibs.c_class_trib, 6)?;
            }
        }
        if self.pagamento.valor != total {
            return Err(ErroNfe::campo(
                "pag/detPag/vPag",
                "precisa ser igual à soma dos itens",
            ));
        }
        if let Some(ibs) = &self.total_ibs_cbs
            && ibs.v_bc != total
        {
            return Err(ErroNfe::campo(
                "total/IBSCBSTot/vBCIBSCBS",
                "precisa ser igual à soma dos itens",
            ));
        }
        Ok(())
    }

    /// Monta o documento XML sem assinatura.
    pub fn documento(&self) -> Result<Documento, ErroNfe> {
        self.validar()?;
        let chave = self.chave()?;
        let id = format!("NFe{}", chave.as_str());
        let c_dv = chave.as_str().chars().last().unwrap_or('0');
        let mut inf = el("infNFe")
            .com_atributo_texto("versao", VERSAO)
            .com_atributo_texto("Id", &id);

        inf = inf.com_filho(No::Elemento(montar_ide(self, c_dv)));
        inf = inf.com_filho(No::Elemento(montar_emitente(&self.emitente)));
        if let Some(dest) = &self.destinatario {
            inf = inf.com_filho(No::Elemento(montar_destinatario(dest)));
        }
        for (indice, item) in self.itens.iter().enumerate() {
            inf = inf.com_filho(No::Elemento(montar_item(indice + 1, item)));
        }
        inf = inf.com_filho(No::Elemento(montar_total(self)));
        inf = inf.com_filho(No::Elemento(
            el("transp").com_filho(No::Elemento(txt("modFrete", self.mod_frete))),
        ));
        inf = inf.com_filho(No::Elemento(
            el("pag").com_filho(No::Elemento(
                el("detPag")
                    .com_filho(No::Elemento(txt("tPag", &self.pagamento.tipo)))
                    .com_filho(No::Elemento(txt("vPag", self.pagamento.valor.para_xml()))),
            )),
        ));
        if let Some(inf_cpl) = &self.inf_cpl {
            inf = inf.com_filho(No::Elemento(
                el("infAdic").com_filho(No::Elemento(txt("infCpl", inf_cpl))),
            ));
        }

        let raiz = el("NFe")
            .com_namespace(None, NS_NFE)
            .com_filho(No::Elemento(inf));
        Ok(Documento::new(raiz))
    }

    /// Serializa sem assinatura.
    pub fn para_xml(&self) -> Result<String, ErroNfe> {
        Ok(self.documento()?.to_xml()?)
    }

    /// Assina com o perfil legado da NF-e (RSA-SHA1/SHA-1).
    #[cfg(feature = "nfe-sha1")]
    pub fn assinar(&self, assinante: &dyn crate::dsig::Signer) -> Result<Documento, ErroNfe> {
        let documento = self.documento()?;
        let chave = self.chave()?;
        Ok(crate::dsig::assinar_com_perfil(
            &documento,
            &format!("NFe{}", chave.as_str()),
            assinante,
            crate::dsig::PerfilAssinatura::NfeSha1,
        )?)
    }

    /// Serializa assinada.
    #[cfg(feature = "nfe-sha1")]
    pub fn para_xml_assinado(
        &self,
        assinante: &dyn crate::dsig::Signer,
    ) -> Result<String, ErroNfe> {
        Ok(self.assinar(assinante)?.to_xml()?)
    }

    /// Valida contra um XSD informado pelo chamador.
    #[cfg(feature = "xsd")]
    pub fn validar_xsd(&self, esquema: impl AsRef<std::path::Path>) -> Result<(), ErroNfe> {
        let xml = self.para_xml()?;
        crate::xml::xsd::validar(xml.as_bytes(), esquema)
            .map_err(|erro| ErroNfe::Xsd(erro.to_string()))
    }
}

fn montar_ide(nota: &NotaFiscal, c_dv: char) -> Elemento {
    let mut ide = el("ide")
        .com_filho(No::Elemento(txt("cUF", nota.uf.codigo())))
        .com_filho(No::Elemento(txt("cNF", &nota.c_nf)))
        .com_filho(No::Elemento(txt("natOp", &nota.nat_op)))
        .com_filho(No::Elemento(txt("mod", 55)))
        .com_filho(No::Elemento(txt("serie", nota.serie)))
        .com_filho(No::Elemento(txt("nNF", nota.numero)))
        .com_filho(No::Elemento(txt("dhEmi", &nota.dh_emi)))
        .com_filho(No::Elemento(txt("tpNF", nota.tp_nf)))
        .com_filho(No::Elemento(txt("idDest", nota.id_dest)))
        .com_filho(No::Elemento(txt("cMunFG", &nota.c_mun_fg)))
        .com_filho(No::Elemento(txt("tpImp", nota.tp_imp)))
        .com_filho(No::Elemento(txt("tpEmis", nota.tp_emis)))
        .com_filho(No::Elemento(txt("cDV", c_dv)))
        .com_filho(No::Elemento(txt("tpAmb", nota.ambiente.codigo())))
        .com_filho(No::Elemento(txt("finNFe", nota.fin_nfe)))
        .com_filho(No::Elemento(txt("indFinal", nota.ind_final)))
        .com_filho(No::Elemento(txt("indPres", nota.ind_pres)))
        .com_filho(No::Elemento(txt("procEmi", 0)))
        .com_filho(No::Elemento(txt("verProc", &nota.ver_proc)));
    if let Some(dh_cont) = &nota.dh_cont {
        ide = ide.com_filho(No::Elemento(txt("dhCont", dh_cont)));
    }
    if let Some(x_just) = &nota.x_just {
        ide = ide.com_filho(No::Elemento(txt("xJust", x_just)));
    }
    ide
}

fn montar_emitente(emitente: &Emitente) -> Elemento {
    let mut emit = el("emit")
        .com_filho(No::Elemento(txt("CNPJ", emitente.cnpj.as_str())))
        .com_filho(No::Elemento(txt("xNome", &emitente.nome)));
    if let Some(fantasia) = &emitente.fantasia {
        emit = emit.com_filho(No::Elemento(txt("xFant", fantasia)));
    }
    emit = emit
        .com_filho(No::Elemento(montar_endereco(
            "enderEmit",
            &emitente.endereco,
            true,
        )))
        .com_filho(No::Elemento(txt("IE", &emitente.ie)))
        .com_filho(No::Elemento(txt("CRT", emitente.crt)));
    emit
}

fn montar_destinatario(dest: &Destinatario) -> Elemento {
    let identificacao = if dest.inscricao.len() == 14 {
        txt("CNPJ", &dest.inscricao)
    } else {
        txt("CPF", &dest.inscricao)
    };
    let mut elemento = el("dest")
        .com_filho(No::Elemento(identificacao))
        .com_filho(No::Elemento(txt("xNome", &dest.nome)))
        .com_filho(No::Elemento(montar_endereco(
            "enderDest",
            &dest.endereco,
            false,
        )))
        .com_filho(No::Elemento(txt("indIEDest", dest.ind_ie_dest)));
    if let Some(ie) = &dest.ie {
        elemento = elemento.com_filho(No::Elemento(txt("IE", ie)));
    }
    elemento
}

fn montar_endereco(local: &str, endereco: &Endereco, emitente: bool) -> Elemento {
    let mut elemento = el(local)
        .com_filho(No::Elemento(txt("xLgr", &endereco.logradouro)))
        .com_filho(No::Elemento(txt("nro", &endereco.numero)));
    if let Some(complemento) = &endereco.complemento {
        elemento = elemento.com_filho(No::Elemento(txt("xCpl", complemento)));
    }
    elemento = elemento
        .com_filho(No::Elemento(txt("xBairro", &endereco.bairro)))
        .com_filho(No::Elemento(txt("cMun", &endereco.codigo_municipio)))
        .com_filho(No::Elemento(txt("xMun", &endereco.municipio)))
        .com_filho(No::Elemento(txt("UF", endereco.uf.sigla())))
        .com_filho(No::Elemento(txt("CEP", &endereco.cep)));
    if emitente {
        elemento = elemento
            .com_filho(No::Elemento(txt("cPais", "1058")))
            .com_filho(No::Elemento(txt("xPais", "Brasil")));
    }
    elemento
}

fn montar_item(numero: usize, item: &Item) -> Elemento {
    let prod = el("prod")
        .com_filho(No::Elemento(txt("cProd", &item.codigo)))
        .com_filho(No::Elemento(txt("cEAN", &item.ean)))
        .com_filho(No::Elemento(txt("xProd", &item.descricao)))
        .com_filho(No::Elemento(txt("NCM", &item.ncm)))
        .com_filho(No::Elemento(txt("CFOP", &item.cfop)))
        .com_filho(No::Elemento(txt("uCom", &item.unidade)))
        .com_filho(No::Elemento(txt(
            "qCom",
            formatar_decimal(item.quantidade, 4),
        )))
        .com_filho(No::Elemento(txt(
            "vUnCom",
            formatar_decimal(item.valor_unitario, 10),
        )))
        .com_filho(No::Elemento(txt("vProd", item.valor_total.para_xml())))
        .com_filho(No::Elemento(txt("cEANTrib", &item.ean)))
        .com_filho(No::Elemento(txt("uTrib", &item.unidade)))
        .com_filho(No::Elemento(txt(
            "qTrib",
            formatar_decimal(item.quantidade, 4),
        )))
        .com_filho(No::Elemento(txt(
            "vUnTrib",
            formatar_decimal(item.valor_unitario, 10),
        )))
        .com_filho(No::Elemento(txt("indTot", 1)));
    if let Some(ibs) = &item.ibs_cbs {
        let imposto = el("imposto").com_filho(No::Elemento(
            el("IBSCBS")
                .com_filho(No::Elemento(txt("CST", &ibs.cst)))
                .com_filho(No::Elemento(txt("cClassTrib", &ibs.c_class_trib))),
        ));
        return el("det")
            .com_atributo_texto("nItem", numero.to_string())
            .com_filho(No::Elemento(prod))
            .com_filho(No::Elemento(imposto));
    }
    el("det")
        .com_atributo_texto("nItem", numero.to_string())
        .com_filho(No::Elemento(prod))
        .com_filho(No::Elemento(el("imposto")))
}

fn montar_total(nota: &NotaFiscal) -> Elemento {
    let total_itens = nota.itens.iter().fold(Dinheiro::ZERO, |acc, item| {
        acc.checked_add(item.valor_total).unwrap_or(acc)
    });
    let mut icms = el("ICMSTot");
    for campo in [
        "vBC",
        "vICMS",
        "vICMSDeson",
        "vFCP",
        "vBCST",
        "vST",
        "vFCPST",
        "vFCPSTRet",
    ] {
        icms = icms.com_filho(No::Elemento(txt(campo, "0.00")));
    }
    icms = icms
        .com_filho(No::Elemento(txt("vProd", total_itens.para_xml())))
        .com_filho(No::Elemento(txt("vFrete", "0.00")))
        .com_filho(No::Elemento(txt("vSeg", "0.00")))
        .com_filho(No::Elemento(txt("vDesc", "0.00")))
        .com_filho(No::Elemento(txt("vII", "0.00")))
        .com_filho(No::Elemento(txt("vIPI", "0.00")))
        .com_filho(No::Elemento(txt("vIPIDevol", "0.00")))
        .com_filho(No::Elemento(txt("vPIS", "0.00")))
        .com_filho(No::Elemento(txt("vCOFINS", "0.00")))
        .com_filho(No::Elemento(txt("vOutro", "0.00")))
        .com_filho(No::Elemento(txt("vNF", total_itens.para_xml())));
    let mut total = el("total").com_filho(No::Elemento(icms));
    if let Some(ibs) = &nota.total_ibs_cbs {
        let g_ibs = el("gIBS")
            .com_filho(No::Elemento(
                el("gIBSUF")
                    .com_filho(No::Elemento(txt("vDif", "0.00")))
                    .com_filho(No::Elemento(txt("vDevTrib", "0.00")))
                    .com_filho(No::Elemento(txt("vIBSUF", "0.00"))),
            ))
            .com_filho(No::Elemento(
                el("gIBSMun")
                    .com_filho(No::Elemento(txt("vDif", "0.00")))
                    .com_filho(No::Elemento(txt("vDevTrib", "0.00")))
                    .com_filho(No::Elemento(txt("vIBSMun", "0.00"))),
            ))
            .com_filho(No::Elemento(txt("vIBS", ibs.v_ibs.para_xml())))
            .com_filho(No::Elemento(txt("vCredPres", "0.00")))
            .com_filho(No::Elemento(txt("vCredPresCondSus", "0.00")));
        let g_cbs = el("gCBS")
            .com_filho(No::Elemento(txt("vDif", "0.00")))
            .com_filho(No::Elemento(txt("vDevTrib", "0.00")))
            .com_filho(No::Elemento(txt("vCBS", ibs.v_cbs.para_xml())))
            .com_filho(No::Elemento(txt("vCredPres", "0.00")))
            .com_filho(No::Elemento(txt("vCredPresCondSus", "0.00")));
        total = total.com_filho(No::Elemento(
            el("IBSCBSTot")
                .com_filho(No::Elemento(txt("vBCIBSCBS", ibs.v_bc.para_xml())))
                .com_filho(No::Elemento(g_ibs))
                .com_filho(No::Elemento(g_cbs)),
        ));
    }
    total
}

fn validar_endereco(campo: &'static str, endereco: &Endereco) -> Result<(), ErroNfe> {
    exigir_texto(campo, &endereco.logradouro, 2, 60)?;
    exigir_texto(campo, &endereco.bairro, 2, 60)?;
    exigir_digitos(campo, &endereco.codigo_municipio, 7)?;
    exigir_digitos(campo, &endereco.cep, 8)?;
    Ok(())
}

fn el(local: &str) -> Elemento {
    Elemento::new(Nome::new(local))
}

fn txt(local: &str, valor: impl ToString) -> Elemento {
    el(local).com_texto(valor.to_string())
}

fn exigir_texto(campo: &'static str, valor: &str, min: usize, max: usize) -> Result<(), ErroNfe> {
    let tamanho = valor.chars().count();
    if tamanho < min || tamanho > max {
        return Err(ErroNfe::campo(
            campo,
            format!("esperado {min}..={max} caracteres, recebido {tamanho}"),
        ));
    }
    Ok(())
}

fn exigir_digitos(campo: &'static str, valor: &str, tamanho: usize) -> Result<(), ErroNfe> {
    if valor.len() != tamanho || !valor.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ErroNfe::campo(
            campo,
            format!("esperado {tamanho} dígitos: {valor:?}"),
        ));
    }
    Ok(())
}

fn formatar_decimal(valor: Decimal, casas: u32) -> String {
    format!("{valor:.precisao$}", precisao = casas as usize)
}
