//! Cofre local em SQLite com envelope encryption por empresa.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::cert::CertificadoA1;
use crate::types::Cnpj;
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::vault::auditoria::{EventoAuditoria, RegistroAuditoria, calcular_hash, hash_inicial};
use crate::vault::crypto::{
    AAD_VERIFICADOR, ChaveMestra, ProvedorKek, SENTINELA, TAMANHO_CHAVE, abrir_com_chave,
    gerar_chave, hex, selar_com_chave,
};
use crate::vault::erro::ErroCofre;
use crate::vault::kdf::{CustoKdf, Sal, TAMANHO_SAL};
use crate::vault::modelo::MetadadosCertificado;

/// Versão do formato gravada no arquivo.
pub const FORMATO: &str = "carimbar-vault/1";

const TAMANHO_MAXIMO_ID: usize = 64;

const SCHEMA: &str = concat!(
    "CREATE TABLE empresas (",
    "  cnpj TEXT PRIMARY KEY,",
    "  dek_selada BLOB NOT NULL,",
    "  dek_versao INTEGER NOT NULL,",
    "  criada_em INTEGER NOT NULL",
    ") STRICT;",
    "CREATE TABLE certificados (",
    "  empresa TEXT NOT NULL,",
    "  id TEXT NOT NULL,",
    "  titular TEXT NOT NULL,",
    "  valido_de INTEGER NOT NULL,",
    "  valido_ate INTEGER NOT NULL,",
    "  sha256 TEXT NOT NULL,",
    "  pfx_selado BLOB NOT NULL,",
    "  senha_selada BLOB NOT NULL,",
    "  importado_em INTEGER NOT NULL,",
    "  PRIMARY KEY (empresa, id),",
    "  FOREIGN KEY (empresa) REFERENCES empresas(cnpj) ON DELETE CASCADE",
    ") STRICT;",
    "CREATE TABLE auditoria (",
    "  seq INTEGER PRIMARY KEY AUTOINCREMENT,",
    "  em INTEGER NOT NULL,",
    "  evento TEXT NOT NULL,",
    "  empresa TEXT,",
    "  detalhe TEXT NOT NULL,",
    "  hash_anterior BLOB NOT NULL,",
    "  hash BLOB NOT NULL",
    ") STRICT;",
    "CREATE TABLE meta (",
    "  chave TEXT PRIMARY KEY,",
    "  valor BLOB NOT NULL",
    ") STRICT;",
);

/// Cofre multiempresa de certificados A1.
///
/// - Cada empresa tem uma DEK própria; os certificados e senhas são cifrados
///   com AES-256-GCM sob essa DEK, com o CNPJ, o identificador e a versão no
///   contexto autenticado.
/// - As DEKs são embrulhadas por uma KEK que **não** é gravada no arquivo:
///   venha ela do chamador, de um KMS ou de Argon2id sobre uma senha.
/// - Metadados públicos (CNPJ, identificador, validade, hash da folha e
///   auditoria) ficam em claro para permitir listar e auditar. PFX, senha e
///   chaves ficam cifrados.
pub struct Cofre {
    conn: Connection,
    provedor: Box<dyn ProvedorKek>,
}

impl std::fmt::Debug for Cofre {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cofre")
            .field("provedor", &"<oculto>")
            .finish_non_exhaustive()
    }
}

impl Cofre {
    /// Cria um cofre novo em `caminho` com uma KEK já provisionada.
    ///
    /// # Errors
    ///
    /// Retorna [`ErroCofre::CofreJaExiste`] se o arquivo já contém um cofre.
    pub fn criar(
        caminho: impl AsRef<Path>,
        provedor: Box<dyn ProvedorKek>,
    ) -> Result<Self, ErroCofre> {
        Self::iniciar(Connection::open(caminho)?, provedor, None)
    }

    /// Cria um cofre efêmero em memória, útil para testes e uso descartável.
    ///
    /// # Errors
    ///
    /// Retorna erro se o SQLite não puder ser inicializado.
    pub fn criar_em_memoria(provedor: Box<dyn ProvedorKek>) -> Result<Self, ErroCofre> {
        Self::iniciar(Connection::open_in_memory()?, provedor, None)
    }

    /// Cria um cofre novo derivando a KEK de `senha` com Argon2id.
    ///
    /// O sal e o custo ficam gravados no arquivo; a KEK, não. Reabra com
    /// [`Self::abrir_com_senha`].
    ///
    /// # Errors
    ///
    /// Retorna [`ErroCofre::CofreJaExiste`] ou erro de derivação/gravação.
    pub fn criar_com_senha(
        caminho: impl AsRef<Path>,
        senha: &str,
        custo: CustoKdf,
    ) -> Result<Self, ErroCofre> {
        let sal = Sal::novo()?;
        let chave = ChaveMestra::derivar(senha, &sal, custo)?;
        Self::iniciar(
            Connection::open(caminho)?,
            Box::new(chave),
            Some((sal, custo)),
        )
    }

    /// Abre um cofre existente com uma KEK já provisionada.
    ///
    /// # Errors
    ///
    /// Retorna [`ErroCofre::ChaveIncorreta`] quando a KEK não autentica o
    /// verificador do cofre.
    pub fn abrir(
        caminho: impl AsRef<Path>,
        provedor: Box<dyn ProvedorKek>,
    ) -> Result<Self, ErroCofre> {
        let conn = Connection::open_with_flags(caminho, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        Self::abrir_com(conn, provedor)
    }

    /// Abre um cofre cuja KEK foi derivada de senha, usando o sal e o custo
    /// gravados na criação.
    ///
    /// # Errors
    ///
    /// Retorna [`ErroCofre::ChaveIncorreta`] para senha errada e
    /// [`ErroCofre::Formato`] se o cofre usar um provedor externo de KEK.
    pub fn abrir_com_senha(caminho: impl AsRef<Path>, senha: &str) -> Result<Self, ErroCofre> {
        let conn = Connection::open_with_flags(caminho, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        preparar_conexao(&conn)?;

        let kdf = meta_texto(&conn, "kdf")?
            .ok_or_else(|| ErroCofre::Formato("parâmetro kdf ausente".to_owned()))?;
        if kdf != "argon2id" {
            return Err(ErroCofre::Formato(
                "cofre usa provedor externo de KEK; abra com Cofre::abrir".to_owned(),
            ));
        }
        let sal = ler_sal(&conn)?;
        let custo = CustoKdf {
            memoria_kib: meta_u32(&conn, "kdf_m")?,
            iteracoes: meta_u32(&conn, "kdf_t")?,
            paralelismo: meta_u32(&conn, "kdf_p")?,
        };
        let chave = ChaveMestra::derivar(senha, &sal, custo)?;
        Self::abrir_com(conn, Box::new(chave))
    }

    /// Importa um PKCS#12 A1 para a empresa.
    ///
    /// O certificado é validado contra o CNPJ da empresa antes de qualquer
    /// gravação. O PFX e a senha são cifrados com a DEK da empresa. Se a
    /// empresa ainda não existir, ela é criada com uma DEK nova na mesma
    /// transação.
    ///
    /// # Errors
    ///
    /// Retorna [`ErroCofre::JaExiste`] para identificador repetido,
    /// [`ErroCofre::Certificado`] para PFX/senha/titular inválidos e
    /// [`ErroCofre::Parametro`] para identificador fora do formato.
    pub fn importar(
        &mut self,
        empresa: &Cnpj,
        id: &str,
        pfx: &[u8],
        senha: &str,
    ) -> Result<MetadadosCertificado, ErroCofre> {
        validar_id(id)?;
        let certificado = CertificadoA1::abrir(pfx, senha, empresa)?;
        let agora = agora();

        let metadados = MetadadosCertificado {
            empresa: *empresa,
            id: id.to_owned(),
            titular: *certificado.cnpj(),
            valido_de: certificado.valido_desde(),
            valido_ate: certificado.valido_ate(),
            sha256: sha256_hex(certificado.certificado_der()),
            // O banco guarda segundos; truncar aqui mantém o retorno igual ao
            // que será lido em `listar`.
            importado_em: tempo(agora),
        };

        let cnpj = empresa.as_str();
        let provedor = self.provedor.as_ref();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let (dek, versao) = dek_para_escrita(&tx, provedor, cnpj, agora)?;

        let repetido: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM certificados WHERE empresa = ?1 AND id = ?2)",
            params![cnpj, id],
            |row| row.get(0),
        )?;
        if repetido {
            return Err(ErroCofre::JaExiste);
        }

        let pfx_selado = selar_com_chave(&dek, pfx, &aad_segredo("pfx", cnpj, id, versao))?;
        let senha_selada = selar_com_chave(
            &dek,
            senha.as_bytes(),
            &aad_segredo("senha", cnpj, id, versao),
        )?;

        tx.execute(
            concat!(
                "INSERT INTO certificados ",
                "(empresa, id, titular, valido_de, valido_ate, sha256, pfx_selado, senha_selada, importado_em) ",
                "VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
            ),
            params![
                cnpj,
                id,
                metadados.titular.as_str(),
                segundos(metadados.valido_de),
                segundos(metadados.valido_ate),
                metadados.sha256,
                pfx_selado,
                senha_selada,
                agora
            ],
        )?;

        auditar(
            &tx,
            EventoAuditoria::CertificadoImportado,
            Some(cnpj),
            &format!(
                "id={id},sha256={},titular={}",
                metadados.sha256, metadados.titular
            ),
            agora,
        )?;
        tx.commit()?;
        Ok(metadados)
    }

    /// Lista os metadados dos certificados da empresa, ordenados por `id`.
    ///
    /// Empresa inexistente retorna lista vazia. Nenhum segredo é decifrado.
    ///
    /// # Errors
    ///
    /// Retorna erro de leitura do SQLite ou de metadados corrompidos.
    pub fn listar(&self, empresa: &Cnpj) -> Result<Vec<MetadadosCertificado>, ErroCofre> {
        let mut stmt = self.conn.prepare(concat!(
            "SELECT id, titular, valido_de, valido_ate, sha256, importado_em ",
            "FROM certificados WHERE empresa = ?1 ORDER BY id"
        ))?;
        let linhas = stmt
            .query_map([empresa.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        linhas
            .into_iter()
            .map(
                |(id, titular, valido_de, valido_ate, sha256, importado_em)| {
                    Ok(MetadadosCertificado {
                        empresa: *empresa,
                        id,
                        titular: Cnpj::parse(&titular)
                            .map_err(|_| ErroCofre::Formato("titular inválido".to_owned()))?,
                        valido_de: tempo(valido_de),
                        valido_ate: tempo(valido_ate),
                        sha256,
                        importado_em: tempo(importado_em),
                    })
                },
            )
            .collect()
    }

    /// Decifra e devolve o certificado A1 da empresa, pronto para assinar.
    ///
    /// # Errors
    ///
    /// Retorna [`ErroCofre::CertificadoNaoEncontrado`] quando não existe,
    /// [`ErroCofre::RegistroAdulterado`] quando o registro não autentica e
    /// [`ErroCofre::Certificado`] quando o A1 já expirou.
    pub fn certificado(&self, empresa: &Cnpj, id: &str) -> Result<CertificadoA1, ErroCofre> {
        let cnpj = empresa.as_str();
        let linha = self
            .conn
            .query_row(
                concat!(
                    "SELECT c.pfx_selado, c.senha_selada, e.dek_selada, e.dek_versao ",
                    "FROM certificados c JOIN empresas e ON e.cnpj = c.empresa ",
                    "WHERE c.empresa = ?1 AND c.id = ?2"
                ),
                params![cnpj, id],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?;

        let Some((pfx_selado, senha_selada, dek_selada, versao)) = linha else {
            return Err(ErroCofre::CertificadoNaoEncontrado);
        };
        let versao = u32::try_from(versao).map_err(|_| ErroCofre::RegistroAdulterado)?;
        let dek = abrir_dek(self.provedor.as_ref(), cnpj, versao, &dek_selada)?;

        let pfx = abrir_segredo(&dek, "pfx", cnpj, id, versao, &pfx_selado)?;
        let senha = abrir_segredo(&dek, "senha", cnpj, id, versao, &senha_selada)?;
        let senha = std::str::from_utf8(senha.as_slice())
            .map_err(|_| ErroCofre::Formato("senha armazenada ilegível".to_owned()))?;

        CertificadoA1::abrir(pfx.as_slice(), senha, empresa).map_err(ErroCofre::from)
    }

    /// Remove um certificado da empresa.
    ///
    /// # Errors
    ///
    /// Retorna [`ErroCofre::CertificadoNaoEncontrado`] se não existir.
    pub fn remover(&mut self, empresa: &Cnpj, id: &str) -> Result<(), ErroCofre> {
        let cnpj = empresa.as_str();
        let agora = agora();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let removidos = tx.execute(
            "DELETE FROM certificados WHERE empresa = ?1 AND id = ?2",
            params![cnpj, id],
        )?;
        if removidos == 0 {
            return Err(ErroCofre::CertificadoNaoEncontrado);
        }
        auditar(
            &tx,
            EventoAuditoria::CertificadoRemovido,
            Some(cnpj),
            &format!("id={id}"),
            agora,
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Rotaciona a DEK da empresa e recifra todos os seus certificados.
    ///
    /// Devolve quantos certificados foram recifrados.
    ///
    /// # Errors
    ///
    /// Retorna [`ErroCofre::EmpresaNaoEncontrada`] se a empresa não existir. A
    /// operação é transacional: em erro, nada é alterado.
    pub fn rotacionar_dek(&mut self, empresa: &Cnpj) -> Result<u32, ErroCofre> {
        let cnpj = empresa.as_str();
        let agora = agora();
        let provedor = self.provedor.as_ref();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let linha = tx
            .query_row(
                "SELECT dek_selada, dek_versao FROM empresas WHERE cnpj = ?1",
                [cnpj],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let Some((dek_selada, versao)) = linha else {
            return Err(ErroCofre::EmpresaNaoEncontrada);
        };
        let versao = u32::try_from(versao).map_err(|_| ErroCofre::RegistroAdulterado)?;
        let dek_antiga = abrir_dek(provedor, cnpj, versao, &dek_selada)?;
        let nova_versao = versao
            .checked_add(1)
            .ok_or_else(|| ErroCofre::Parametro("versão da DEK esgotada".to_owned()))?;
        let nova_dek = gerar_chave()?;

        let linhas = {
            let mut stmt = tx.prepare(
                "SELECT id, pfx_selado, senha_selada FROM certificados WHERE empresa = ?1",
            )?;
            stmt.query_map([cnpj], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
        };

        for (id, pfx_selado, senha_selada) in &linhas {
            let pfx = abrir_segredo(&dek_antiga, "pfx", cnpj, id, versao, pfx_selado)?;
            let senha = abrir_segredo(&dek_antiga, "senha", cnpj, id, versao, senha_selada)?;
            let pfx_novo =
                selar_com_chave(&nova_dek, &pfx, &aad_segredo("pfx", cnpj, id, nova_versao))?;
            let senha_nova = selar_com_chave(
                &nova_dek,
                &senha,
                &aad_segredo("senha", cnpj, id, nova_versao),
            )?;
            tx.execute(
                "UPDATE certificados SET pfx_selado = ?3, senha_selada = ?4 WHERE empresa = ?1 AND id = ?2",
                params![cnpj, id, pfx_novo, senha_nova],
            )?;
        }

        let dek_nova_selada = provedor.selar(&nova_dek[..], &aad_dek(cnpj, nova_versao))?;
        tx.execute(
            "UPDATE empresas SET dek_selada = ?2, dek_versao = ?3 WHERE cnpj = ?1",
            params![cnpj, dek_nova_selada, i64::from(nova_versao)],
        )?;
        auditar(
            &tx,
            EventoAuditoria::DekRotacionada,
            Some(cnpj),
            &format!("versao={nova_versao},certificados={}", linhas.len()),
            agora,
        )?;
        tx.commit()?;
        Ok(u32::try_from(linhas.len()).unwrap_or(u32::MAX))
    }

    /// Troca a KEK e reembrulha a DEK de todas as empresas.
    ///
    /// A nova KEK é testada antes de qualquer gravação e só passa a valer se a
    /// transação tiver sucesso.
    ///
    /// # Errors
    ///
    /// Retorna erro se o novo provedor não selar/abrir de forma consistente ou
    /// se algum registro não autenticar. Nesse caso o cofre continua com a KEK
    /// anterior.
    pub fn rotacionar_kek(&mut self, novo: Box<dyn ProvedorKek>) -> Result<(), ErroCofre> {
        let verificador = novo.selar(SENTINELA, AAD_VERIFICADOR)?;
        let conferido = novo.abrir(&verificador, AAD_VERIFICADOR)?;
        if conferido.as_slice() != SENTINELA {
            return Err(ErroCofre::Parametro(
                "provedor de KEK inconsistente".to_owned(),
            ));
        }

        let agora = agora();
        {
            let antigo = self.provedor.as_ref();
            let tx = self
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)?;

            let empresas = {
                let mut stmt = tx.prepare("SELECT cnpj, dek_selada, dek_versao FROM empresas")?;
                stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?
            };

            for (cnpj, dek_selada, versao) in &empresas {
                let versao = u32::try_from(*versao).map_err(|_| ErroCofre::RegistroAdulterado)?;
                let dek = abrir_dek(antigo, cnpj, versao, dek_selada)?;
                let reembrulhada = novo.selar(&dek[..], &aad_dek(cnpj, versao))?;
                tx.execute(
                    "UPDATE empresas SET dek_selada = ?2 WHERE cnpj = ?1",
                    params![cnpj, reembrulhada],
                )?;
            }

            meta_gravar(&tx, "verificador", &verificador)?;
            auditar(
                &tx,
                EventoAuditoria::KekRotacionada,
                None,
                &format!("empresas={}", empresas.len()),
                agora,
            )?;
            tx.commit()?;
        }

        self.provedor = novo;
        Ok(())
    }

    /// Lista os registros de auditoria em ordem de inserção.
    ///
    /// # Errors
    ///
    /// Retorna erro de leitura ou metadado de auditoria corrompido.
    pub fn auditoria(&self) -> Result<Vec<RegistroAuditoria>, ErroCofre> {
        linhas_auditoria(&self.conn)?
            .into_iter()
            .map(|linha| {
                Ok(RegistroAuditoria {
                    seq: linha.seq,
                    em: tempo(linha.em),
                    evento: EventoAuditoria::de_str(&linha.evento)?,
                    empresa: linha
                        .empresa
                        .as_deref()
                        .map(Cnpj::parse)
                        .transpose()
                        .map_err(|_| {
                            ErroCofre::Formato("empresa da auditoria inválida".to_owned())
                        })?,
                    detalhe: linha.detalhe,
                    hash: hex(&linha.hash),
                })
            })
            .collect()
    }

    /// Recalcula a cadeia de auditoria e confere todos os registros.
    ///
    /// Detecta alteração de campo, remoção de registro intermediário ou
    /// reordenação sem recomputar os hashes. Não detecta remoção do último
    /// registro nem reescrita completa da cadeia por quem tem acesso de escrita.
    ///
    /// # Errors
    ///
    /// Retorna [`ErroCofre::Auditoria`] com a posição do primeiro registro que
    /// não confere.
    pub fn verificar_auditoria(&self) -> Result<(), ErroCofre> {
        let mut anterior = hash_inicial();
        for linha in linhas_auditoria(&self.conn)? {
            if linha.hash_anterior.as_slice() != anterior {
                return Err(ErroCofre::Auditoria(linha.seq));
            }
            let calculado = calcular_hash(
                &anterior,
                linha.em,
                &linha.evento,
                linha.empresa.as_deref(),
                &linha.detalhe,
            );
            if linha.hash.as_slice() != calculado {
                return Err(ErroCofre::Auditoria(linha.seq));
            }
            anterior = calculado;
        }
        Ok(())
    }

    fn iniciar(
        mut conn: Connection,
        provedor: Box<dyn ProvedorKek>,
        kdf: Option<(Sal, CustoKdf)>,
    ) -> Result<Self, ErroCofre> {
        preparar_conexao(&conn)?;
        if tabela_existe(&conn, "meta")? && meta_texto(&conn, "formato")?.is_some() {
            return Err(ErroCofre::CofreJaExiste);
        }

        let agora = agora();
        {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            tx.execute_batch(SCHEMA)?;
            meta_gravar(&tx, "formato", FORMATO.as_bytes())?;
            meta_gravar(&tx, "criado_em", agora.to_string().as_bytes())?;
            let verificador = provedor.selar(SENTINELA, AAD_VERIFICADOR)?;
            meta_gravar(&tx, "verificador", &verificador)?;
            match &kdf {
                Some((sal, custo)) => {
                    meta_gravar(&tx, "kdf", b"argon2id")?;
                    meta_gravar(&tx, "kdf_sal", sal.as_bytes())?;
                    meta_gravar(&tx, "kdf_m", custo.memoria_kib.to_string().as_bytes())?;
                    meta_gravar(&tx, "kdf_t", custo.iteracoes.to_string().as_bytes())?;
                    meta_gravar(&tx, "kdf_p", custo.paralelismo.to_string().as_bytes())?;
                }
                None => meta_gravar(&tx, "kdf", b"externa")?,
            }
            auditar(
                &tx,
                EventoAuditoria::CofreCriado,
                None,
                &format!("formato={FORMATO}"),
                agora,
            )?;
            tx.commit()?;
        }

        Ok(Self { conn, provedor })
    }

    fn abrir_com(conn: Connection, provedor: Box<dyn ProvedorKek>) -> Result<Self, ErroCofre> {
        preparar_conexao(&conn)?;
        let formato = meta_texto(&conn, "formato")?
            .ok_or_else(|| ErroCofre::Formato("meta do cofre ausente".to_owned()))?;
        if formato != FORMATO {
            return Err(ErroCofre::Formato(format!(
                "formato não suportado: {formato}"
            )));
        }
        let verificador = meta_ler(&conn, "verificador")?
            .ok_or_else(|| ErroCofre::Formato("verificador ausente".to_owned()))?;
        let claro = provedor
            .abrir(&verificador, AAD_VERIFICADOR)
            .map_err(|_| ErroCofre::ChaveIncorreta)?;
        if claro.as_slice() != SENTINELA {
            return Err(ErroCofre::ChaveIncorreta);
        }
        Ok(Self { conn, provedor })
    }
}

struct LinhaAuditoria {
    seq: i64,
    em: i64,
    evento: String,
    empresa: Option<String>,
    detalhe: String,
    hash_anterior: Vec<u8>,
    hash: Vec<u8>,
}

fn linhas_auditoria(conn: &Connection) -> Result<Vec<LinhaAuditoria>, ErroCofre> {
    let mut stmt = conn.prepare(concat!(
        "SELECT seq, em, evento, empresa, detalhe, hash_anterior, hash ",
        "FROM auditoria ORDER BY seq"
    ))?;
    let linhas = stmt
        .query_map([], |row| {
            Ok(LinhaAuditoria {
                seq: row.get(0)?,
                em: row.get(1)?,
                evento: row.get(2)?,
                empresa: row.get(3)?,
                detalhe: row.get(4)?,
                hash_anterior: row.get(5)?,
                hash: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(linhas)
}

fn preparar_conexao(conn: &Connection) -> Result<(), ErroCofre> {
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))
        .ok();
    Ok(())
}

fn tabela_existe(conn: &Connection, nome: &str) -> Result<bool, ErroCofre> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [nome],
        |row| row.get(0),
    )?)
}

fn meta_gravar(conn: &Connection, chave: &str, valor: &[u8]) -> Result<(), ErroCofre> {
    conn.execute(
        "INSERT INTO meta (chave, valor) VALUES (?1, ?2)
         ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor",
        params![chave, valor],
    )?;
    Ok(())
}

fn meta_ler(conn: &Connection, chave: &str) -> Result<Option<Vec<u8>>, ErroCofre> {
    Ok(conn
        .query_row("SELECT valor FROM meta WHERE chave = ?1", [chave], |row| {
            row.get(0)
        })
        .optional()?)
}

fn meta_texto(conn: &Connection, chave: &str) -> Result<Option<String>, ErroCofre> {
    match meta_ler(conn, chave)? {
        Some(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| ErroCofre::Formato(format!("meta {chave} não é UTF-8"))),
        None => Ok(None),
    }
}

fn meta_u32(conn: &Connection, chave: &str) -> Result<u32, ErroCofre> {
    let texto = meta_texto(conn, chave)?
        .ok_or_else(|| ErroCofre::Formato(format!("meta {chave} ausente")))?;
    texto
        .parse()
        .map_err(|_| ErroCofre::Formato(format!("meta {chave} inválida")))
}

fn ler_sal(conn: &Connection) -> Result<Sal, ErroCofre> {
    let bytes = meta_ler(conn, "kdf_sal")?
        .ok_or_else(|| ErroCofre::Formato("sal do KDF ausente".to_owned()))?;
    let sal: [u8; TAMANHO_SAL] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| ErroCofre::Formato("sal do KDF inválido".to_owned()))?;
    Ok(Sal::de_bytes(sal))
}

fn validar_id(id: &str) -> Result<(), ErroCofre> {
    if id.is_empty() || id.len() > TAMANHO_MAXIMO_ID {
        return Err(ErroCofre::Parametro(format!(
            "id deve ter entre 1 e {TAMANHO_MAXIMO_ID} caracteres"
        )));
    }
    if !id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ErroCofre::Parametro(
            "id deve usar apenas ASCII alfanumérico, '-', '_' ou '.'".to_owned(),
        ));
    }
    Ok(())
}

fn aad_dek(cnpj: &str, versao: u32) -> Vec<u8> {
    format!("carimbar-vault/v1/dek/{cnpj}/{versao}").into_bytes()
}

fn aad_segredo(campo: &str, cnpj: &str, id: &str, versao: u32) -> Vec<u8> {
    format!("carimbar-vault/v1/{campo}/{cnpj}/{id}/{versao}").into_bytes()
}

fn abrir_dek(
    provedor: &dyn ProvedorKek,
    cnpj: &str,
    versao: u32,
    selada: &[u8],
) -> Result<Zeroizing<[u8; TAMANHO_CHAVE]>, ErroCofre> {
    let claro = provedor
        .abrir(selada, &aad_dek(cnpj, versao))
        .map_err(|_| ErroCofre::RegistroAdulterado)?;
    let bytes: [u8; TAMANHO_CHAVE] = claro
        .as_slice()
        .try_into()
        .map_err(|_| ErroCofre::RegistroAdulterado)?;
    Ok(Zeroizing::new(bytes))
}

/// DEK decifrada e a versão a que ela corresponde.
type DekAberta = (Zeroizing<[u8; TAMANHO_CHAVE]>, u32);

fn dek_existente(
    conn: &Connection,
    provedor: &dyn ProvedorKek,
    cnpj: &str,
) -> Result<Option<DekAberta>, ErroCofre> {
    let linha = conn
        .query_row(
            "SELECT dek_selada, dek_versao FROM empresas WHERE cnpj = ?1",
            [cnpj],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    let Some((selada, versao)) = linha else {
        return Ok(None);
    };
    let versao = u32::try_from(versao).map_err(|_| ErroCofre::RegistroAdulterado)?;
    let dek = abrir_dek(provedor, cnpj, versao, &selada)?;
    Ok(Some((dek, versao)))
}

fn dek_para_escrita(
    conn: &Connection,
    provedor: &dyn ProvedorKek,
    cnpj: &str,
    agora: i64,
) -> Result<DekAberta, ErroCofre> {
    if let Some(encontrada) = dek_existente(conn, provedor, cnpj)? {
        return Ok(encontrada);
    }

    let dek = gerar_chave()?;
    let versao = 1_u32;
    let selada = provedor.selar(&dek[..], &aad_dek(cnpj, versao))?;
    conn.execute(
        "INSERT INTO empresas (cnpj, dek_selada, dek_versao, criada_em) VALUES (?1, ?2, ?3, ?4)",
        params![cnpj, selada, i64::from(versao), agora],
    )?;
    auditar(
        conn,
        EventoAuditoria::EmpresaCriada,
        Some(cnpj),
        &format!("cnpj={cnpj},dek_versao={versao}"),
        agora,
    )?;
    Ok((dek, versao))
}

fn abrir_segredo(
    dek: &[u8; TAMANHO_CHAVE],
    campo: &str,
    cnpj: &str,
    id: &str,
    versao: u32,
    selado: &[u8],
) -> Result<Zeroizing<Vec<u8>>, ErroCofre> {
    abrir_com_chave(dek, selado, &aad_segredo(campo, cnpj, id, versao))
        .map_err(|_| ErroCofre::RegistroAdulterado)
}

fn auditar(
    conn: &Connection,
    evento: EventoAuditoria,
    empresa: Option<&str>,
    detalhe: &str,
    em: i64,
) -> Result<(), ErroCofre> {
    let ultimo = conn
        .query_row(
            "SELECT hash FROM auditoria ORDER BY seq DESC LIMIT 1",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?;
    let anterior: [u8; 32] = match ultimo {
        Some(hash) => hash
            .as_slice()
            .try_into()
            .map_err(|_| ErroCofre::Formato("hash de auditoria inválido".to_owned()))?,
        None => hash_inicial(),
    };
    let hash = calcular_hash(&anterior, em, evento.as_str(), empresa, detalhe);
    conn.execute(
        concat!(
            "INSERT INTO auditoria (em, evento, empresa, detalhe, hash_anterior, hash) ",
            "VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        ),
        params![
            em,
            evento.as_str(),
            empresa,
            detalhe,
            anterior.as_slice(),
            hash.as_slice()
        ],
    )?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex(&digest)
}

fn agora() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duracao| {
            i64::try_from(duracao.as_secs()).unwrap_or(i64::MAX)
        })
}

fn segundos(instante: SystemTime) -> i64 {
    instante.duration_since(UNIX_EPOCH).map_or(0, |duracao| {
        i64::try_from(duracao.as_secs()).unwrap_or(i64::MAX)
    })
}

fn tempo(segundos: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(u64::try_from(segundos).unwrap_or(0))
}
