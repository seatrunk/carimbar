use crate::erro::ErroCertificado;
use carimbar_types::Cnpj;
use p12_keystore::{KeyStore, Pkcs12ImportPolicy};
use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use x509_parser::der_parser::{self, asn1_rs::FromDer};
use x509_parser::extensions::{GeneralName, ParsedExtension};
use x509_parser::prelude::X509Certificate;
use zeroize::Zeroizing;

/// OID ICP-Brasil do CNPJ do titular de certificado de pessoa jurídica.
pub const OID_CNPJ_ICP_BRASIL: &str = "2.16.76.1.3.3";

const OID_RSA: &str = "1.2.840.113549.1.1.1";
const TAMANHO_MAX_PFX: usize = 1024 * 1024;

/// Certificado A1 aberto e validado para um CNPJ.
///
/// Não tem `Clone` e `Debug` oculta a chave privada. A chave é zerada ao
/// sair de escopo.
pub struct CertificadoA1 {
    cnpj: Cnpj,
    cadeia: Vec<Vec<u8>>,
    valido_de: i64,
    valido_ate: i64,
    chave_pkcs8: Zeroizing<Vec<u8>>,
}

/// Material de identidade TLS (cadeia e chave PKCS#8), emprestado do certificado.
///
/// Quem consome (transporte mTLS) não recebe cópia da chave.
pub struct IdentidadeTls<'a> {
    cadeia: &'a [Vec<u8>],
    chave_pkcs8: &'a [u8],
}

impl CertificadoA1 {
    /// Abre um PKCS#12 A1 e valida senha, chave, titular e validade no instante atual.
    pub fn abrir(pfx: &[u8], senha: &str, cnpj: &Cnpj) -> Result<Self, ErroCertificado> {
        Self::abrir_em(pfx, senha, cnpj, SystemTime::now())
    }

    /// Abre um PKCS#12 A1 e valida a validade no `instante` informado.
    pub fn abrir_em(
        pfx: &[u8],
        senha: &str,
        cnpj: &Cnpj,
        instante: SystemTime,
    ) -> Result<Self, ErroCertificado> {
        if pfx.len() > TAMANHO_MAX_PFX {
            return Err(ErroCertificado::ArquivoGrande {
                limite: TAMANHO_MAX_PFX,
            });
        }
        let armazem = KeyStore::from_pkcs12(pfx, senha, Pkcs12ImportPolicy::Strict)
            .map_err(erro_de_importacao)?;
        let (_, par) = armazem
            .private_key_chain()
            .ok_or(ErroCertificado::SemChavePrivada)?;

        let oid = par.key().oid().to_string();
        if oid != OID_RSA {
            return Err(ErroCertificado::ChaveNaoSuportada(oid));
        }
        let chave_pkcs8 = Zeroizing::new(par.key().as_der().to_vec());
        let chave_publica = chave_publica_rsa(&chave_pkcs8)?;

        let mut cadeia: Vec<Vec<u8>> = par.certs().iter().map(|c| c.as_der().to_vec()).collect();
        let indice_folha = indice_da_folha(&cadeia, &chave_publica)?;
        let folha_der = cadeia.remove(indice_folha);
        cadeia.insert(0, folha_der);

        let (_, folha) = parse(&cadeia[0])?;
        verificar_titular(&folha, cnpj)?;
        let valido_de = folha.validity().not_before.timestamp();
        let valido_ate = folha.validity().not_after.timestamp();
        let agora = segundos_desde_epoca(instante);
        if agora < valido_de {
            return Err(ErroCertificado::AindaNaoValido(
                folha.validity().not_before.to_string(),
            ));
        }
        if agora > valido_ate {
            return Err(ErroCertificado::Expirado(
                folha.validity().not_after.to_string(),
            ));
        }

        Ok(Self {
            cnpj: *cnpj,
            cadeia,
            valido_de,
            valido_ate,
            chave_pkcs8,
        })
    }

    /// CNPJ do titular validado.
    pub fn cnpj(&self) -> &Cnpj {
        &self.cnpj
    }

    /// Início da validade da folha.
    pub fn valido_desde(&self) -> SystemTime {
        tempo_desde_epoca(self.valido_de)
    }

    /// Fim da validade da folha.
    pub fn valido_ate(&self) -> SystemTime {
        tempo_desde_epoca(self.valido_ate)
    }

    /// Certificado folha em DER.
    pub fn certificado_der(&self) -> &[u8] {
        &self.cadeia[0]
    }

    /// Cadeia em DER, com a folha na primeira posição.
    pub fn cadeia_der(&self) -> &[Vec<u8>] {
        &self.cadeia
    }

    /// Assina `mensagem` com RSASSA-PKCS1-v1_5 e SHA-256 (algoritmo XMLDSig RSA-SHA256).
    ///
    /// A validade é verificada apenas em [`CertificadoA1::abrir`]/[`CertificadoA1::abrir_em`].
    pub fn assinar_rsa_sha256(&self, mensagem: &[u8]) -> Result<Vec<u8>, ErroCertificado> {
        let par = RsaKeyPair::from_pkcs8(&self.chave_pkcs8)
            .map_err(|_| ErroCertificado::ChaveInvalida)?;
        let mut assinatura = vec![0_u8; par.public().modulus_len()];
        par.sign(
            &RSA_PKCS1_SHA256,
            &SystemRandom::new(),
            mensagem,
            &mut assinatura,
        )
        .map_err(|_| ErroCertificado::Assinatura("falha na operação RSA".to_owned()))?;
        Ok(assinatura)
    }

    /// Assina com RSA PKCS#1 v1.5/SHA-1 para o contrato legado da NF-e.
    ///
    /// Disponível somente com a feature `nfe-sha1`. Usa o backend OpenSSL;
    /// SHA-256 continua no `ring`. Não há fallback automático entre algoritmos.
    /// A validade é verificada na abertura, como em [`Self::assinar_rsa_sha256`].
    #[cfg(feature = "nfe-sha1")]
    pub fn assinar_rsa_sha1_nfe(&self, mensagem: &[u8]) -> Result<Vec<u8>, ErroCertificado> {
        let assinar = || {
            let chave = openssl::pkey::PKey::private_key_from_pkcs8(&self.chave_pkcs8)?;
            let mut assinante =
                openssl::sign::Signer::new(openssl::hash::MessageDigest::sha1(), &chave)?;
            assinante.set_rsa_padding(openssl::rsa::Padding::PKCS1)?;
            assinante.update(mensagem)?;
            assinante.sign_to_vec()
        };
        assinar().map_err(|_: openssl::error::ErrorStack| {
            ErroCertificado::Assinatura("falha na operação RSA-SHA1 da NF-e".to_owned())
        })
    }

    /// Identidade TLS para o transporte mTLS. Não copia a chave.
    pub fn identidade_tls(&self) -> IdentidadeTls<'_> {
        IdentidadeTls {
            cadeia: &self.cadeia,
            chave_pkcs8: &self.chave_pkcs8,
        }
    }
}

impl IdentidadeTls<'_> {
    /// Cadeia em DER, com a folha na primeira posição.
    pub fn cadeia_der(&self) -> &[Vec<u8>] {
        self.cadeia
    }

    /// Chave privada em PKCS#8 DER. Usar apenas para montar a identidade TLS.
    pub fn chave_pkcs8_der(&self) -> &[u8] {
        self.chave_pkcs8
    }
}

impl fmt::Debug for CertificadoA1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CertificadoA1")
            .field("cnpj", &self.cnpj)
            .field("valido_de", &self.valido_de)
            .field("valido_ate", &self.valido_ate)
            .field("chave_privada", &"<oculta>")
            .finish()
    }
}

impl fmt::Debug for IdentidadeTls<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IdentidadeTls")
            .field("certificados", &self.cadeia.len())
            .field("chave_privada", &"<oculta>")
            .finish()
    }
}

fn erro_de_importacao(erro: p12_keystore::error::Error) -> ErroCertificado {
    match erro {
        p12_keystore::error::Error::MacError(_) => ErroCertificado::SenhaIncorreta,
        _ => ErroCertificado::ArquivoInvalido,
    }
}

fn chave_publica_rsa(pkcs8: &[u8]) -> Result<Vec<u8>, ErroCertificado> {
    let par = RsaKeyPair::from_pkcs8(pkcs8).map_err(|_| ErroCertificado::ChaveInvalida)?;
    Ok(par.public().as_ref().to_vec())
}

fn indice_da_folha(cadeia: &[Vec<u8>], chave_publica: &[u8]) -> Result<usize, ErroCertificado> {
    for (indice, der) in cadeia.iter().enumerate() {
        let (_, cert) = parse(der)?;
        if cert.public_key().subject_public_key.data.as_ref() == chave_publica {
            return Ok(indice);
        }
    }
    Err(ErroCertificado::ChaveNaoCorresponde)
}

fn parse(der: &[u8]) -> Result<(&[u8], X509Certificate<'_>), ErroCertificado> {
    X509Certificate::from_der(der).map_err(|_| ErroCertificado::CertificadoInvalido)
}

fn verificar_titular(folha: &X509Certificate<'_>, cnpj: &Cnpj) -> Result<(), ErroCertificado> {
    let encontrado = cnpj_do_titular(folha)?;
    match encontrado {
        None => Err(ErroCertificado::SemCnpjTitular),
        Some(texto) if texto == cnpj.as_str() => Ok(()),
        Some(texto) => Err(ErroCertificado::TitularIncompativel {
            esperado: cnpj.as_str().to_owned(),
            encontrado: texto,
        }),
    }
}

fn cnpj_do_titular(folha: &X509Certificate<'_>) -> Result<Option<String>, ErroCertificado> {
    for extensao in folha.extensions() {
        let ParsedExtension::SubjectAlternativeName(san) = extensao.parsed_extension() else {
            continue;
        };
        for nome in &san.general_names {
            let GeneralName::OtherName(oid, valor) = nome else {
                continue;
            };
            if oid.to_id_string() != OID_CNPJ_ICP_BRASIL {
                continue;
            }
            let texto = texto_da_outra_nome(valor)?;
            return Ok(Some(texto));
        }
    }
    Ok(None)
}

fn texto_da_outra_nome(valor: &[u8]) -> Result<String, ErroCertificado> {
    let invalido = || ErroCertificado::CertificadoInvalido;
    let (_, externo) = der_parser::parse_der(valor).map_err(|_| invalido())?;
    let interno = match &externo.content {
        der_parser::ber::BerObjectContent::Unknown(any) if externo.header.is_contextspecific() => {
            any.data
        }
        _ => valor,
    };
    let (_, texto) = der_parser::parse_der(interno).map_err(|_| invalido())?;
    texto.as_str().map(str::to_owned).map_err(|_| invalido())
}

fn segundos_desde_epoca(instante: SystemTime) -> i64 {
    match instante.duration_since(UNIX_EPOCH) {
        Ok(decorrido) => i64::try_from(decorrido.as_secs()).unwrap_or(i64::MAX),
        Err(antes) => -i64::try_from(antes.duration().as_secs()).unwrap_or(i64::MAX),
    }
}

fn tempo_desde_epoca(segundos: i64) -> SystemTime {
    match u64::try_from(segundos) {
        Ok(positivo) => UNIX_EPOCH + Duration::from_secs(positivo),
        Err(_) => UNIX_EPOCH,
    }
}
