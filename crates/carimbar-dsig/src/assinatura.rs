use crate::erro::ErroDsig;
use crate::{ALG_C14N, ALG_ENVELOPED, ALG_RSA_SHA256, ALG_SHA256, NS_DSIG};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use carimbar_xml::{Caminho, Documento, Elemento, No, Nome};
use sha2::{Digest, Sha256};

/// Assinante de documentos XMLDSig.
///
/// A chave privada fica no implementador e nunca é recebida por este crate:
/// só o certificado (público) e o resultado da assinatura cruzam a fronteira.
pub trait Signer {
    /// Certificado X.509 folha em DER, incluído em `KeyInfo`.
    fn certificado_der(&self) -> &[u8];

    /// Assina `dados` com RSASSA-PKCS1-v1_5 e SHA-256.
    fn assinar_rsa_sha256(&self, dados: &[u8]) -> Result<Vec<u8>, ErroDsig>;
}

impl Signer for carimbar_cert::CertificadoA1 {
    fn certificado_der(&self) -> &[u8] {
        carimbar_cert::CertificadoA1::certificado_der(self)
    }

    fn assinar_rsa_sha256(&self, dados: &[u8]) -> Result<Vec<u8>, ErroDsig> {
        carimbar_cert::CertificadoA1::assinar_rsa_sha256(self, dados)
            .map_err(|erro| ErroDsig::Assinante(erro.to_string()))
    }
}

/// Assina enveloped o elemento com `Id = id` e devolve um novo documento com
/// a `Signature` como último filho da raiz.
///
/// O digest cobre a forma canônica C14N 1.0 inclusiva do elemento. A assinatura
/// cobre o `SignedInfo` canonicalizado. O elemento assinado não pode conter outra
/// assinatura, deve ser filho direto da raiz e o `Id` tem de ser único no documento.
/// Documento já assinado é recusado, sem alterar sua assinatura original.
pub fn assinar(
    documento: &Documento,
    id: &str,
    assinante: &dyn Signer,
) -> Result<Documento, ErroDsig> {
    // Valida também árvores construídas pela API, antes de resolver IDs/namespaces.
    documento.to_xml()?;
    if id.is_empty() {
        return Err(ErroDsig::ReferenciaInvalida(String::new()));
    }
    let assinaturas = documento
        .caminhos_por_local("Signature")
        .iter()
        .filter(|c| documento.namespace_do_elemento(c).as_deref() == Some(NS_DSIG))
        .count();
    if assinaturas != 0 {
        return Err(ErroDsig::Estrutura(
            "documento já contém assinatura XMLDSig".to_owned(),
        ));
    }
    let alvo = alvo_unico(documento, id)?;
    if alvo.len() != 1 {
        return Err(ErroDsig::Estrutura(
            "o elemento assinado deve ser filho direto da raiz".to_owned(),
        ));
    }
    if !documento
        .caminhos_por_local("Signature")
        .iter()
        .all(|c| !c.starts_with(&alvo))
    {
        return Err(ErroDsig::Estrutura(
            "o elemento assinado contém outra assinatura".to_owned(),
        ));
    }

    let digest = STANDARD.encode(Sha256::digest(documento.canonicalizar_subarvore(&alvo)?));
    let certificado = STANDARD.encode(assinante.certificado_der());

    let mut rascunho = documento.clone();
    rascunho
        .raiz_mut()
        .filhos_mut()
        .push(No::Elemento(montar_assinatura(
            id,
            &digest,
            &certificado,
            None,
        )));
    let indice = rascunho.raiz().filhos().len() - 1;
    let info = rascunho.canonicalizar_subarvore(&[indice, 0])?;
    let valor = assinante.assinar_rsa_sha256(info.as_bytes())?;

    let mut assinado = documento.clone();
    assinado
        .raiz_mut()
        .filhos_mut()
        .push(No::Elemento(montar_assinatura(
            id,
            &digest,
            &certificado,
            Some(&STANDARD.encode(valor)),
        )));
    Ok(assinado)
}

pub(crate) fn alvo_unico(documento: &Documento, id: &str) -> Result<Caminho, ErroDsig> {
    let mut caminhos = documento.caminhos_por_atributo("Id", id);
    match caminhos.len() {
        0 => Err(ErroDsig::ReferenciaNaoEncontrada(id.to_owned())),
        1 => Ok(caminhos.remove(0)),
        ocorrencias => Err(ErroDsig::ReferenciaAmbigua {
            id: id.to_owned(),
            ocorrencias,
        }),
    }
}

fn montar_assinatura(id: &str, digest: &str, certificado: &str, valor: Option<&str>) -> Elemento {
    let ds = |local: &str| Nome::com_prefixo("ds", local);
    let algoritmo =
        |local: &str, uri: &str| Elemento::new(ds(local)).com_atributo_texto("Algorithm", uri);
    let texto = |local: &str, conteudo: &str| Elemento::new(ds(local)).com_texto(conteudo);

    let transforms = Elemento::new(ds("Transforms"))
        .com_filho(No::Elemento(algoritmo("Transform", ALG_ENVELOPED)))
        .com_filho(No::Elemento(algoritmo("Transform", ALG_C14N)));
    let referencia = Elemento::new(ds("Reference"))
        .com_atributo_texto("URI", format!("#{id}"))
        .com_filho(No::Elemento(transforms))
        .com_filho(No::Elemento(algoritmo("DigestMethod", ALG_SHA256)))
        .com_filho(No::Elemento(texto("DigestValue", digest)));
    let info = Elemento::new(ds("SignedInfo"))
        .com_filho(No::Elemento(algoritmo("CanonicalizationMethod", ALG_C14N)))
        .com_filho(No::Elemento(algoritmo("SignatureMethod", ALG_RSA_SHA256)))
        .com_filho(No::Elemento(referencia));

    let valor_elemento = match valor {
        Some(valor) => texto("SignatureValue", valor),
        None => Elemento::new(ds("SignatureValue")),
    };
    let chave = Elemento::new(ds("KeyInfo")).com_filho(No::Elemento(
        Elemento::new(ds("X509Data"))
            .com_filho(No::Elemento(texto("X509Certificate", certificado))),
    ));

    Elemento::new(ds("Signature"))
        .com_namespace(Some("ds"), NS_DSIG)
        .com_filho(No::Elemento(info))
        .com_filho(No::Elemento(valor_elemento))
        .com_filho(No::Elemento(chave))
}
