use crate::erro::ErroDsig;
use crate::{ALG_C14N, ALG_ENVELOPED, NS_DSIG, PerfilAssinatura};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use carimbar_xml::{Caminho, Documento};
use ring::signature::UnparsedPublicKey;
use x509_parser::prelude::{FromDer, X509Certificate};

/// Resultado de uma verificação XMLDSig bem-sucedida.
///
/// Indica apenas que o digest confere e que a assinatura foi feita pela chave
/// do certificado em `KeyInfo`. Não indica que esse certificado é confiável.
/// O chamador deve comparar [`Self::id`] com o identificador esperado e usar apenas
/// o elemento referenciado como conteúdo autenticado; irmãos não estão assinados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verificacao {
    id: String,
    certificado_der: Vec<u8>,
}

impl Verificacao {
    /// `Id` do elemento assinado.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Certificado em `KeyInfo`, em DER. Quem usa o resultado decide se ele é confiável.
    pub fn certificado_der(&self) -> &[u8] {
        &self.certificado_der
    }
}

/// Verifica a assinatura enveloped do perfil: uma `Signature` filha da raiz, uma
/// referência `#Id`, C14N 1.0 inclusiva, RSA-SHA256 e digest SHA-256.
///
/// Valida a integridade do conteúdo assinado e a assinatura criptográfica. Não
/// valida a cadeia do certificado, a validade no tempo nem a revogação. Isso
/// cabe a quem usa o resultado, com `carimbar-cert` e a política de confiança.
pub fn verificar(documento: &Documento) -> Result<Verificacao, ErroDsig> {
    verificar_com_perfil(documento, PerfilAssinatura::Sha256)
}

/// Verifica somente o perfil escolhido, sem aceitar downgrade por URIs do XML.
/// As regras de referência e confiança são as mesmas de [`verificar`].
pub fn verificar_com_perfil(
    documento: &Documento,
    perfil: PerfilAssinatura,
) -> Result<Verificacao, ErroDsig> {
    documento.to_xml()?;
    let assinaturas: Vec<Caminho> = documento
        .caminhos_por_local("Signature")
        .into_iter()
        .filter(|c| documento.namespace_do_elemento(c).as_deref() == Some(NS_DSIG))
        .collect();
    let caminho_assinatura = match assinaturas.len() {
        0 => return Err(ErroDsig::SemAssinatura),
        1 => assinaturas[0].clone(),
        n => return Err(ErroDsig::MultiplasAssinaturas(n)),
    };
    if caminho_assinatura.len() != 1 {
        return Err(ErroDsig::AssinaturaForaDaRaiz);
    }

    let caminho_info = filho_unico(documento, &caminho_assinatura, "SignedInfo")?;
    exigir_filhos(
        documento,
        &caminho_assinatura,
        &["SignedInfo", "SignatureValue", "KeyInfo"],
    )?;
    exigir_filhos(
        documento,
        &caminho_info,
        &["CanonicalizationMethod", "SignatureMethod", "Reference"],
    )?;
    let metodo_c14n = filho_unico(documento, &caminho_info, "CanonicalizationMethod")?;
    exigir_algoritmo(documento, &metodo_c14n, "CanonicalizationMethod", ALG_C14N)?;
    let metodo_assinatura = filho_unico(documento, &caminho_info, "SignatureMethod")?;
    exigir_algoritmo(
        documento,
        &metodo_assinatura,
        "SignatureMethod",
        perfil.assinatura_uri(),
    )?;

    let referencia = filho_unico(documento, &caminho_info, "Reference")?;
    let uri = atributo(documento, &referencia, "URI")?;
    let id = uri
        .strip_prefix('#')
        .filter(|id| !id.is_empty())
        .ok_or_else(|| ErroDsig::ReferenciaInvalida(uri.clone()))?;
    exigir_filhos(
        documento,
        &referencia,
        &["Transforms", "DigestMethod", "DigestValue"],
    )?;
    let transformadas = filho_unico(documento, &referencia, "Transforms")?;
    exigir_filhos(documento, &transformadas, &["Transform", "Transform"])?;
    let transforms = filhos(documento, &transformadas, "Transform")?;
    for (transform, algoritmo) in transforms.iter().zip([ALG_ENVELOPED, ALG_C14N]) {
        exigir_algoritmo(documento, transform, "Transform", algoritmo)?;
    }
    let metodo_digest = filho_unico(documento, &referencia, "DigestMethod")?;
    exigir_algoritmo(
        documento,
        &metodo_digest,
        "DigestMethod",
        perfil.digest_uri(),
    )?;

    let caminho_alvo = {
        let mut caminhos = documento.caminhos_por_atributo("Id", id);
        match caminhos.len() {
            0 => return Err(ErroDsig::ReferenciaNaoEncontrada(id.to_owned())),
            1 => caminhos.remove(0),
            ocorrencias => {
                return Err(ErroDsig::ReferenciaAmbigua {
                    id: id.to_owned(),
                    ocorrencias,
                });
            }
        }
    };
    if caminho_alvo.len() != 1 || caminho_alvo.starts_with(&caminho_assinatura) {
        return Err(ErroDsig::Estrutura(
            "a referência deve apontar para outro filho direto da raiz".to_owned(),
        ));
    }
    if documento
        .caminhos_por_local("Signature")
        .iter()
        .any(|c| c.starts_with(&caminho_alvo))
    {
        return Err(ErroDsig::Estrutura(
            "o elemento referenciado contém uma assinatura".to_owned(),
        ));
    }

    let digest_esperado = decodificar(
        &texto_do_filho(documento, &referencia, "DigestValue")?,
        "DigestValue",
    )?;
    let digest_calculado =
        perfil.digest(documento.canonicalizar_subarvore(&caminho_alvo)?.as_bytes());
    if digest_calculado.as_slice() != digest_esperado.as_slice() {
        return Err(ErroDsig::DigestDivergente);
    }

    let caminho_chave = filho_unico(documento, &caminho_assinatura, "KeyInfo")?;
    exigir_filhos(documento, &caminho_chave, &["X509Data"])?;
    let caminho_dados = filho_unico(documento, &caminho_chave, "X509Data")?;
    exigir_filhos(documento, &caminho_dados, &["X509Certificate"])?;
    let certificado_der = decodificar(
        &texto_do_filho(documento, &caminho_dados, "X509Certificate")?,
        "X509Certificate",
    )?;
    let (restante, certificado) =
        X509Certificate::from_der(&certificado_der).map_err(|_| ErroDsig::CertificadoInvalido)?;
    if !restante.is_empty()
        || certificado.public_key().algorithm.algorithm.to_id_string() != "1.2.840.113549.1.1.1"
    {
        return Err(ErroDsig::CertificadoInvalido);
    }
    let chave_publica = certificado.public_key().subject_public_key.data.to_vec();

    let valor = decodificar(
        &texto_do_filho(documento, &caminho_assinatura, "SignatureValue")?,
        "SignatureValue",
    )?;
    let info_canonico = documento.canonicalizar_subarvore(&caminho_info)?;
    UnparsedPublicKey::new(perfil.verificador(), &chave_publica)
        .verify(info_canonico.as_bytes(), &valor)
        .map_err(|_| ErroDsig::AssinaturaInvalida)?;

    Ok(Verificacao {
        id: id.to_owned(),
        certificado_der,
    })
}

fn filhos(documento: &Documento, pai: &[usize], local: &str) -> Result<Vec<Caminho>, ErroDsig> {
    let elemento = documento
        .elemento_em(pai)
        .ok_or_else(|| ErroDsig::Estrutura("caminho inválido".to_owned()))?;
    let mut encontrados = Vec::new();
    for indice in 0..elemento.filhos().len() {
        let mut caminho = pai.to_vec();
        caminho.push(indice);
        let Some(filho) = documento.elemento_em(&caminho) else {
            continue;
        };
        if filho.nome().local() == local
            && documento.namespace_do_elemento(&caminho).as_deref() == Some(NS_DSIG)
        {
            encontrados.push(caminho);
        }
    }
    Ok(encontrados)
}

fn filho_unico(documento: &Documento, pai: &[usize], local: &str) -> Result<Caminho, ErroDsig> {
    let mut encontrados = filhos(documento, pai, local)?;
    match encontrados.len() {
        1 => Ok(encontrados.remove(0)),
        0 => Err(ErroDsig::Estrutura(format!("elemento {local} ausente"))),
        _ => Err(ErroDsig::Estrutura(format!("elemento {local} repetido"))),
    }
}

fn atributo(documento: &Documento, caminho: &[usize], nome: &str) -> Result<String, ErroDsig> {
    documento
        .elemento_em(caminho)
        .and_then(|e| e.atributo(nome))
        .map(str::to_owned)
        .ok_or_else(|| ErroDsig::Estrutura(format!("atributo {nome} ausente")))
}

fn exigir_algoritmo(
    documento: &Documento,
    caminho: &[usize],
    campo: &'static str,
    esperado: &str,
) -> Result<(), ErroDsig> {
    exigir_filhos(documento, caminho, &[])?;
    let uri = atributo(documento, caminho, "Algorithm")?;
    if uri == esperado {
        Ok(())
    } else {
        Err(ErroDsig::AlgoritmoNaoSuportado { campo, uri })
    }
}

fn texto_do_filho(documento: &Documento, pai: &[usize], local: &str) -> Result<String, ErroDsig> {
    let caminho = filho_unico(documento, pai, local)?;
    exigir_filhos(documento, &caminho, &[])?;
    documento
        .elemento_em(&caminho)
        .map(|e| e.texto())
        .ok_or_else(|| ErroDsig::Estrutura(format!("elemento {local} inválido")))
}

fn exigir_filhos(
    documento: &Documento,
    caminho: &[usize],
    esperados: &[&str],
) -> Result<(), ErroDsig> {
    let elemento = documento
        .elemento_em(caminho)
        .ok_or_else(|| ErroDsig::Estrutura("caminho inválido".to_owned()))?;
    let mut encontrados = Vec::new();
    for (indice, filho) in elemento.filhos().iter().enumerate() {
        if let carimbar_xml::No::Elemento(filho) = filho {
            let mut filho_caminho = caminho.to_vec();
            filho_caminho.push(indice);
            if documento.namespace_do_elemento(&filho_caminho).as_deref() != Some(NS_DSIG) {
                return Err(ErroDsig::Estrutura(
                    "elemento fora do namespace XMLDSig".to_owned(),
                ));
            }
            encontrados.push(filho.nome().local());
        }
    }
    if encontrados != esperados {
        return Err(ErroDsig::Estrutura(format!(
            "filhos inválidos em {}",
            elemento.nome().local()
        )));
    }
    Ok(())
}

fn decodificar(texto: &str, campo: &'static str) -> Result<Vec<u8>, ErroDsig> {
    let limpo: String = texto.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    STANDARD.decode(limpo).map_err(|_| ErroDsig::Base64(campo))
}
