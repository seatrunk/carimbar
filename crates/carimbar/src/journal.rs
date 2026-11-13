//! Journal local de operações fiscais e estado de produção por empresa.
//!
//! O journal é um SQLite separado do cofre. Ele guarda apenas XML fiscal,
//! identificadores públicos, hash de conteúdo e estado da operação; nenhum
//! segredo, certificado ou senha.
//!
//! A idempotência é garantida por uma chave única composta de empresa,
//! ambiente, tipo e identificador da operação. O hash do conteúdo detecta o
//! reuso da mesma chave com dados divergentes. O envio é reivindicado por um
//! `UPDATE` atômico antes da chamada de rede, para que duas instâncias não
//! enviem a mesma operação.

use crate::erro::ErroEmissor;
use crate::nfse::DocumentoDistribuido;
use crate::types::{Cnpj, TpAmb};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Tipo de operação persistida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TipoOperacao {
    /// Emissão de NFS-e a partir de uma DPS.
    EmitirNfse,
    /// Cancelamento de NFS-e.
    CancelarNfse,
    /// Emissão de NF-e.
    EmitirNfe,
    /// Cancelamento de NF-e.
    CancelarNfe,
    /// Carta de correção da NF-e.
    CceNfe,
    /// Inutilização de numeração da NF-e.
    InutilizarNfe,
}

impl TipoOperacao {
    /// Texto estável no banco.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmitirNfse => "nfse:emitir",
            Self::CancelarNfse => "nfse:cancelar",
            Self::EmitirNfe => "nfe:emitir",
            Self::CancelarNfe => "nfe:cancelar",
            Self::CceNfe => "nfe:cce",
            Self::InutilizarNfe => "nfe:inutilizar",
        }
    }

    fn from_str(texto: &str) -> Result<Self, ErroEmissor> {
        match texto {
            "nfse:emitir" => Ok(Self::EmitirNfse),
            "nfse:cancelar" => Ok(Self::CancelarNfse),
            "nfe:emitir" => Ok(Self::EmitirNfe),
            "nfe:cancelar" => Ok(Self::CancelarNfe),
            "nfe:cce" => Ok(Self::CceNfe),
            "nfe:inutilizar" => Ok(Self::InutilizarNfe),
            outro => Err(ErroEmissor::Journal(format!("tipo desconhecido: {outro}"))),
        }
    }
}

/// Estado persistido de uma operação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EstadoOperacao {
    /// Conteúdo preparado; nenhum envio realizado.
    Preparada,
    /// A requisição foi escrita; o resultado ainda não foi lido.
    Enviada,
    /// NFS-e autorizada e XML preservado.
    Autorizada,
    /// Rejeição fiscal persistida.
    Rejeitada,
    /// O envio falhou antes de começar; repetir é seguro.
    FalhaAntesDoEnvio,
    /// O resultado não pôde ser lido; consulte antes de reenviar.
    Desconhecida,
    /// A reconciliação confirmou que a operação não foi processada.
    NaoEncontrada,
    /// Cancelamento registrado.
    Cancelada,
}

impl EstadoOperacao {
    /// Texto estável no banco.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preparada => "preparada",
            Self::Enviada => "enviada",
            Self::Autorizada => "autorizada",
            Self::Rejeitada => "rejeitada",
            Self::FalhaAntesDoEnvio => "falha_antes_envio",
            Self::Desconhecida => "desconhecida",
            Self::NaoEncontrada => "nao_encontrada",
            Self::Cancelada => "cancelada",
        }
    }

    fn from_str(texto: &str) -> Result<Self, ErroEmissor> {
        match texto {
            "preparada" => Ok(Self::Preparada),
            "enviada" => Ok(Self::Enviada),
            "autorizada" => Ok(Self::Autorizada),
            "rejeitada" => Ok(Self::Rejeitada),
            "falha_antes_envio" => Ok(Self::FalhaAntesDoEnvio),
            "desconhecida" => Ok(Self::Desconhecida),
            "nao_encontrada" => Ok(Self::NaoEncontrada),
            "cancelada" => Ok(Self::Cancelada),
            outro => Err(ErroEmissor::Journal(format!(
                "estado desconhecido: {outro}"
            ))),
        }
    }
}

/// Operação fiscal persistida.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operacao {
    /// Identificador local.
    pub id: i64,
    /// Empresa emitente.
    pub empresa: Cnpj,
    /// Ambiente no momento da preparação.
    pub ambiente: TpAmb,
    /// Tipo da operação.
    pub tipo: TipoOperacao,
    /// Chave de idempotência (Id da DPS ou Id do pedido de evento).
    pub chave_operacao: String,
    /// Chave de idempotência escolhida pelo cliente (por exemplo, o id da
    /// venda), quando houver.
    pub chave_cliente: Option<String>,
    /// SHA-256 do XML preparado, em hexadecimal.
    pub hash_conteudo: String,
    /// Estado atual.
    pub estado: EstadoOperacao,
    /// XML preparado (sem assinatura).
    pub xml_preparado: String,
    /// XML assinado efetivamente enviado.
    pub xml_enviado: Option<String>,
    /// XML autorizado ou evento retornado.
    pub xml_retorno: Option<String>,
    /// Chave da NFS-e, quando autorizada.
    pub chave_nfse: Option<String>,
    /// Protocolo/data de processamento, quando disponível.
    pub protocolo: Option<String>,
    /// Diagnóstico da última falha ou rejeição.
    pub erro: Option<String>,
    /// Criação, em segundos desde a época.
    pub criado_em: i64,
    /// Última atualização, em segundos desde a época.
    pub atualizado_em: i64,
}

/// Documento do ADN persistido no journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentoAdnArmazenado {
    /// NSU.
    pub nsu: i64,
    /// Chave de acesso.
    pub chave_acesso: Option<String>,
    /// Tipo do documento.
    pub tipo_documento: String,
    /// Tipo do evento.
    pub tipo_evento: Option<String>,
    /// XML.
    pub xml: String,
    /// Data e hora de geração.
    pub data_hora_geracao: Option<String>,
}

/// Journal SQLite de operações e empresas.
pub struct Journal {
    conn: Connection,
}

impl std::fmt::Debug for Journal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Journal").finish_non_exhaustive()
    }
}

impl Journal {
    /// Abre ou cria o journal no caminho informado.
    pub fn abrir(caminho: impl AsRef<Path>) -> Result<Self, ErroEmissor> {
        Self::iniciar(Connection::open(caminho)?)
    }

    /// Cria um journal efêmero em memória.
    pub fn em_memoria() -> Result<Self, ErroEmissor> {
        Self::iniciar(Connection::open_in_memory()?)
    }

    fn iniciar(conn: Connection) -> Result<Self, ErroEmissor> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS empresas (
                 cnpj TEXT PRIMARY KEY,
                 ambiente TEXT NOT NULL CHECK (ambiente IN ('homologacao','producao')),
                 certificado_id TEXT,
                 criado_em INTEGER NOT NULL,
                 atualizado_em INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS operacoes (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 empresa TEXT NOT NULL,
                 ambiente TEXT NOT NULL CHECK (ambiente IN ('producao','homologacao')),
                 tipo TEXT NOT NULL,
                 chave_operacao TEXT NOT NULL,
                 chave_cliente TEXT,
                 hash_conteudo TEXT NOT NULL,
                 estado TEXT NOT NULL,
                 xml_preparado TEXT NOT NULL,
                 xml_enviado TEXT,
                 xml_retorno TEXT,
                 chave_nfse TEXT,
                 protocolo TEXT,
                 erro TEXT,
                 criado_em INTEGER NOT NULL,
                 atualizado_em INTEGER NOT NULL,
                 UNIQUE (empresa, ambiente, tipo, chave_operacao)
             );
             CREATE INDEX IF NOT EXISTS operacoes_empresa_estado
                 ON operacoes (empresa, ambiente, estado);
             CREATE TABLE IF NOT EXISTS adn_cursor (
                 empresa TEXT NOT NULL,
                 ambiente TEXT NOT NULL,
                 ultimo_nsu INTEGER NOT NULL,
                 atualizado_em INTEGER NOT NULL,
                 PRIMARY KEY (empresa, ambiente)
             );
             CREATE TABLE IF NOT EXISTS numeracao (
                 empresa TEXT NOT NULL,
                 ambiente TEXT NOT NULL,
                 modelo TEXT NOT NULL,
                 serie INTEGER NOT NULL,
                 ultimo INTEGER NOT NULL,
                 PRIMARY KEY (empresa, ambiente, modelo, serie)
             );
             CREATE TABLE IF NOT EXISTS adn_documentos (
                 empresa TEXT NOT NULL,
                 ambiente TEXT NOT NULL,
                 nsu INTEGER NOT NULL,
                 chave_acesso TEXT,
                 tipo_documento TEXT NOT NULL,
                 tipo_evento TEXT,
                 xml TEXT NOT NULL,
                 data_hora_geracao TEXT,
                 criado_em INTEGER NOT NULL,
                 PRIMARY KEY (empresa, ambiente, nsu)
             );",
        )?;
        // Migração de journals criados antes da chave de idempotência do
        // cliente. `CREATE TABLE IF NOT EXISTS` não adiciona colunas.
        let tem_chave_cliente: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('operacoes') WHERE name = 'chave_cliente')",
            [],
            |row| row.get(0),
        )?;
        if !tem_chave_cliente {
            conn.execute("ALTER TABLE operacoes ADD COLUMN chave_cliente TEXT", [])?;
        }
        conn.execute_batch(
            "CREATE UNIQUE INDEX IF NOT EXISTS operacoes_chave_cliente
                 ON operacoes (empresa, ambiente, tipo, chave_cliente)
                 WHERE chave_cliente IS NOT NULL;",
        )?;
        Ok(Self { conn })
    }

    /// Garante que a empresa existe no journal, em homologação por padrão.
    pub fn garantir_empresa(&self, empresa: &Cnpj) -> Result<TpAmb, ErroEmissor> {
        let agora = agora();
        self.conn.execute(
            "INSERT OR IGNORE INTO empresas (cnpj, ambiente, criado_em, atualizado_em)
             VALUES (?1, 'homologacao', ?2, ?2)",
            params![empresa.as_str(), agora],
        )?;
        self.ambiente_empresa(empresa)
    }

    /// Ambiente configurado para a empresa.
    pub fn ambiente_empresa(&self, empresa: &Cnpj) -> Result<TpAmb, ErroEmissor> {
        let ambiente: Option<String> = self
            .conn
            .query_row(
                "SELECT ambiente FROM empresas WHERE cnpj = ?1",
                [empresa.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        match ambiente.as_deref() {
            Some("producao") => Ok(TpAmb::Producao),
            Some("homologacao") => Ok(TpAmb::Homologacao),
            Some(outro) => Err(ErroEmissor::Journal(format!(
                "ambiente desconhecido: {outro}"
            ))),
            None => Err(ErroEmissor::EmpresaNaoEncontrada(*empresa)),
        }
    }

    /// Promove a empresa para produção, vinculando o certificado.
    pub fn promover_empresa(
        &self,
        empresa: &Cnpj,
        certificado_id: &str,
    ) -> Result<(), ErroEmissor> {
        let alteradas = self.conn.execute(
            "UPDATE empresas SET ambiente = 'producao', certificado_id = ?2, atualizado_em = ?3
             WHERE cnpj = ?1",
            params![empresa.as_str(), certificado_id, agora()],
        )?;
        if alteradas == 0 {
            return Err(ErroEmissor::EmpresaNaoEncontrada(*empresa));
        }
        Ok(())
    }

    /// Devolve a empresa para homologação.
    pub fn rebaixar_empresa(&self, empresa: &Cnpj) -> Result<(), ErroEmissor> {
        let alteradas = self.conn.execute(
            "UPDATE empresas SET ambiente = 'homologacao', certificado_id = NULL, atualizado_em = ?2
             WHERE cnpj = ?1",
            params![empresa.as_str(), agora()],
        )?;
        if alteradas == 0 {
            return Err(ErroEmissor::EmpresaNaoEncontrada(*empresa));
        }
        Ok(())
    }

    /// Certificado vinculado à promoção para produção, se houver.
    pub fn certificado_producao(&self, empresa: &Cnpj) -> Result<Option<String>, ErroEmissor> {
        Ok(self
            .conn
            .query_row(
                "SELECT certificado_id FROM empresas WHERE cnpj = ?1",
                [empresa.as_str()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten())
    }

    /// Busca uma operação pela chave de idempotência.
    pub fn buscar(
        &self,
        empresa: &Cnpj,
        ambiente: TpAmb,
        tipo: TipoOperacao,
        chave_operacao: &str,
    ) -> Result<Option<Operacao>, ErroEmissor> {
        let linha = self
            .conn
            .query_row(
                "SELECT id, empresa, ambiente, tipo, chave_operacao, hash_conteudo, estado,
                        xml_preparado, xml_enviado, xml_retorno, chave_nfse, protocolo, erro,
                        criado_em, atualizado_em, chave_cliente
                 FROM operacoes
                 WHERE empresa = ?1 AND ambiente = ?2 AND tipo = ?3 AND chave_operacao = ?4",
                params![
                    empresa.as_str(),
                    ambiente_str(ambiente),
                    tipo.as_str(),
                    chave_operacao
                ],
                ler_linha,
            )
            .optional()?;
        linha.transpose()
    }

    /// Cria a operação ou devolve a existente com o mesmo hash.
    ///
    /// Retorna `true` quando a operação foi criada agora.
    pub fn criar(
        &mut self,
        empresa: &Cnpj,
        ambiente: TpAmb,
        tipo: TipoOperacao,
        chave_operacao: &str,
        hash_conteudo: &str,
        xml_preparado: &str,
    ) -> Result<(Operacao, bool), ErroEmissor> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let existente = tx
            .query_row(
                "SELECT id, empresa, ambiente, tipo, chave_operacao, hash_conteudo, estado,
                        xml_preparado, xml_enviado, xml_retorno, chave_nfse, protocolo, erro,
                        criado_em, atualizado_em, chave_cliente
                 FROM operacoes
                 WHERE empresa = ?1 AND ambiente = ?2 AND tipo = ?3 AND chave_operacao = ?4",
                params![
                    empresa.as_str(),
                    ambiente_str(ambiente),
                    tipo.as_str(),
                    chave_operacao
                ],
                ler_linha,
            )
            .optional()?;
        if let Some(linha) = existente {
            let operacao = linha?;
            if operacao.hash_conteudo != hash_conteudo {
                return Err(ErroEmissor::ConflitoIdempotencia {
                    chave: chave_operacao.to_owned(),
                });
            }
            tx.commit()?;
            return Ok((operacao, false));
        }
        let agora = agora();
        tx.execute(
            "INSERT INTO operacoes
             (empresa, ambiente, tipo, chave_operacao, hash_conteudo, estado,
              xml_preparado, criado_em, atualizado_em)
             VALUES (?1, ?2, ?3, ?4, ?5, 'preparada', ?6, ?7, ?7)",
            params![
                empresa.as_str(),
                ambiente_str(ambiente),
                tipo.as_str(),
                chave_operacao,
                hash_conteudo,
                xml_preparado,
                agora
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        let operacao = self.operacao(id)?;
        Ok((operacao, true))
    }

    /// Busca a operação pela chave de idempotência do cliente.
    pub fn operacao_por_chave_cliente(
        &self,
        empresa: &Cnpj,
        ambiente: TpAmb,
        tipo: TipoOperacao,
        chave_cliente: &str,
    ) -> Result<Option<Operacao>, ErroEmissor> {
        let linha = self
            .conn
            .query_row(
                "SELECT id, empresa, ambiente, tipo, chave_operacao, hash_conteudo, estado,
                        xml_preparado, xml_enviado, xml_retorno, chave_nfse, protocolo, erro,
                        criado_em, atualizado_em, chave_cliente
                 FROM operacoes
                 WHERE empresa = ?1 AND ambiente = ?2 AND tipo = ?3 AND chave_cliente = ?4",
                params![
                    empresa.as_str(),
                    ambiente_str(ambiente),
                    tipo.as_str(),
                    chave_cliente
                ],
                ler_linha,
            )
            .optional()?;
        linha.transpose()
    }

    /// Cria a operação vinculada a uma chave de idempotência do cliente.
    ///
    /// Quando a chave já existe com o mesmo conteúdo, devolve a operação
    /// existente. Se a operação anterior foi rejeitada (ou ficou em um estado
    /// em que repetir é seguro) e o conteúdo mudou, a linha é repreparada para
    /// reaproveitar o mesmo número da DPS. Se o número mudou, a chave é liberada
    /// da linha antiga e uma nova operação é criada.
    ///
    /// Retorna `true` quando uma operação nova (ou repreparada) foi gravada.
    #[allow(clippy::too_many_arguments)]
    pub fn criar_com_chave_cliente(
        &mut self,
        empresa: &Cnpj,
        ambiente: TpAmb,
        tipo: TipoOperacao,
        chave_operacao: &str,
        chave_cliente: Option<&str>,
        hash_conteudo: &str,
        xml_preparado: &str,
    ) -> Result<(Operacao, bool), ErroEmissor> {
        let Some(chave_cliente) = chave_cliente else {
            return self.criar(
                empresa,
                ambiente,
                tipo,
                chave_operacao,
                hash_conteudo,
                xml_preparado,
            );
        };
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        if let Some(linha) = tx
            .query_row(
                "SELECT id, empresa, ambiente, tipo, chave_operacao, hash_conteudo, estado,
                        xml_preparado, xml_enviado, xml_retorno, chave_nfse, protocolo, erro,
                        criado_em, atualizado_em, chave_cliente
                 FROM operacoes
                 WHERE empresa = ?1 AND ambiente = ?2 AND tipo = ?3 AND chave_cliente = ?4",
                params![
                    empresa.as_str(),
                    ambiente_str(ambiente),
                    tipo.as_str(),
                    chave_cliente
                ],
                ler_linha,
            )
            .optional()?
        {
            let existente = linha?;
            if existente.chave_operacao == chave_operacao {
                if existente.hash_conteudo == hash_conteudo {
                    tx.commit()?;
                    return Ok((existente, false));
                }
                if repetivel(&existente.estado) {
                    tx.execute(
                        "UPDATE operacoes
                         SET hash_conteudo = ?2, xml_preparado = ?3, estado = 'preparada',
                             xml_enviado = NULL, xml_retorno = NULL, protocolo = NULL,
                             erro = NULL, atualizado_em = ?4
                         WHERE id = ?1",
                        params![existente.id, hash_conteudo, xml_preparado, agora()],
                    )?;
                    tx.commit()?;
                    let operacao = self.operacao(existente.id)?;
                    return Ok((operacao, true));
                }
                tx.commit()?;
                return Err(ErroEmissor::ConflitoIdempotencia {
                    chave: chave_cliente.to_owned(),
                });
            }
            if !repetivel(&existente.estado) {
                tx.commit()?;
                return Err(ErroEmissor::ConflitoIdempotencia {
                    chave: chave_cliente.to_owned(),
                });
            }
            // O número mudou (por exemplo, após E0014): libera a chave da
            // linha antiga, preservando a rejeição no histórico.
            tx.execute(
                "UPDATE operacoes SET chave_cliente = NULL, atualizado_em = ?2 WHERE id = ?1",
                params![existente.id, agora()],
            )?;
        }
        // A mesma DPS pode já existir sem chave do cliente (por exemplo, uma
        // preparação anterior por XML). Mantém o contrato de idempotência do Id.
        if let Some(linha) = tx
            .query_row(
                "SELECT id, empresa, ambiente, tipo, chave_operacao, hash_conteudo, estado,
                        xml_preparado, xml_enviado, xml_retorno, chave_nfse, protocolo, erro,
                        criado_em, atualizado_em, chave_cliente
                 FROM operacoes
                 WHERE empresa = ?1 AND ambiente = ?2 AND tipo = ?3 AND chave_operacao = ?4",
                params![
                    empresa.as_str(),
                    ambiente_str(ambiente),
                    tipo.as_str(),
                    chave_operacao
                ],
                ler_linha,
            )
            .optional()?
        {
            let mut existente = linha?;
            if existente.hash_conteudo != hash_conteudo
                || existente
                    .chave_cliente
                    .as_deref()
                    .is_some_and(|outra| outra != chave_cliente)
            {
                tx.commit()?;
                return Err(ErroEmissor::ConflitoIdempotencia {
                    chave: chave_operacao.to_owned(),
                });
            }
            if existente.chave_cliente.is_none() {
                tx.execute(
                    "UPDATE operacoes SET chave_cliente = ?2, atualizado_em = ?3 WHERE id = ?1",
                    params![existente.id, chave_cliente, agora()],
                )?;
                existente.chave_cliente = Some(chave_cliente.to_owned());
            }
            tx.commit()?;
            return Ok((existente, false));
        }
        let agora = agora();
        tx.execute(
            "INSERT INTO operacoes
             (empresa, ambiente, tipo, chave_operacao, chave_cliente, hash_conteudo, estado,
              xml_preparado, criado_em, atualizado_em)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'preparada', ?7, ?8, ?8)",
            params![
                empresa.as_str(),
                ambiente_str(ambiente),
                tipo.as_str(),
                chave_operacao,
                chave_cliente,
                hash_conteudo,
                xml_preparado,
                agora
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        let operacao = self.operacao(id)?;
        Ok((operacao, true))
    }

    /// Libera a chave de idempotência do cliente de uma operação.
    ///
    /// Usado após E0014: a operação rejeitada continua no histórico, mas o
    /// próximo número pode ser vinculado à mesma chave do cliente.
    pub fn liberar_chave_cliente(&mut self, id: i64) -> Result<(), ErroEmissor> {
        self.conn.execute(
            "UPDATE operacoes SET chave_cliente = NULL, atualizado_em = ?2 WHERE id = ?1",
            params![id, agora()],
        )?;
        Ok(())
    }

    /// Lê a operação pelo identificador local.
    pub fn operacao(&self, id: i64) -> Result<Operacao, ErroEmissor> {
        let linha = self
            .conn
            .query_row(
                "SELECT id, empresa, ambiente, tipo, chave_operacao, hash_conteudo, estado,
                        xml_preparado, xml_enviado, xml_retorno, chave_nfse, protocolo, erro,
                        criado_em, atualizado_em, chave_cliente
                 FROM operacoes WHERE id = ?1",
                [id],
                ler_linha,
            )
            .optional()?;
        linha
            .transpose()?
            .ok_or(ErroEmissor::OperacaoNaoEncontrada(id))
    }

    /// Reivindica o envio de forma atômica.
    ///
    /// Só uma instância consegue mudar `preparada`, `falha_antes_envio` ou
    /// `nao_encontrada` para `enviada`. Retorna `false` se outra instância já
    /// reivindicou ou se o estado não permite envio.
    pub fn reivindicar_envio(&mut self, id: i64, xml_enviado: &str) -> Result<bool, ErroEmissor> {
        let agora = agora();
        let alteradas = self.conn.execute(
            "UPDATE operacoes
             SET estado = 'enviada', xml_enviado = ?2, erro = NULL, atualizado_em = ?3
             WHERE id = ?1 AND estado IN ('preparada','falha_antes_envio','nao_encontrada')",
            params![id, xml_enviado, agora],
        )?;
        Ok(alteradas == 1)
    }

    /// Persiste uma autorização.
    pub fn marcar_autorizada(
        &mut self,
        id: i64,
        chave_nfse: &str,
        xml_retorno: &str,
        protocolo: Option<&str>,
    ) -> Result<(), ErroEmissor> {
        self.conn.execute(
            "UPDATE operacoes
             SET estado = 'autorizada', chave_nfse = ?2, xml_retorno = ?3, protocolo = ?4,
                 erro = NULL, atualizado_em = ?5
             WHERE id = ?1",
            params![id, chave_nfse, xml_retorno, protocolo, agora()],
        )?;
        Ok(())
    }

    /// Persiste uma rejeição.
    pub fn marcar_rejeitada(&mut self, id: i64, erro: &str) -> Result<(), ErroEmissor> {
        self.conn.execute(
            "UPDATE operacoes SET estado = 'rejeitada', erro = ?2, atualizado_em = ?3 WHERE id = ?1",
            params![id, erro, agora()],
        )?;
        Ok(())
    }

    /// Persiste uma falha antes do envio.
    pub fn marcar_falha_antes_envio(&mut self, id: i64, erro: &str) -> Result<(), ErroEmissor> {
        self.conn.execute(
            "UPDATE operacoes
             SET estado = 'falha_antes_envio', erro = ?2, atualizado_em = ?3
             WHERE id = ?1 AND estado IN ('preparada','falha_antes_envio','enviada','desconhecida')",
            params![id, erro, agora()],
        )?;
        Ok(())
    }

    /// Persiste um resultado desconhecido.
    pub fn marcar_desconhecida(&mut self, id: i64, erro: &str) -> Result<(), ErroEmissor> {
        self.conn.execute(
            "UPDATE operacoes SET estado = 'desconhecida', erro = ?2, atualizado_em = ?3 WHERE id = ?1",
            params![id, erro, agora()],
        )?;
        Ok(())
    }

    /// Persiste que a operação não foi encontrada na reconciliação.
    pub fn marcar_nao_encontrada(&mut self, id: i64, detalhe: &str) -> Result<(), ErroEmissor> {
        self.conn.execute(
            "UPDATE operacoes
             SET estado = 'nao_encontrada', erro = ?2, atualizado_em = ?3
             WHERE id = ?1 AND estado IN ('enviada','desconhecida')",
            params![id, detalhe, agora()],
        )?;
        Ok(())
    }

    /// Persiste um evento de cancelamento registrado.
    pub fn marcar_cancelada(&mut self, id: i64, xml_evento: &str) -> Result<(), ErroEmissor> {
        self.conn.execute(
            "UPDATE operacoes
             SET estado = 'cancelada', xml_retorno = ?2, erro = NULL, atualizado_em = ?3
             WHERE id = ?1",
            params![id, xml_evento, agora()],
        )?;
        Ok(())
    }

    /// Reserva o próximo número da NF-e para empresa/ambiente/modelo/série.
    ///
    /// A reserva é atômica no SQLite, inclusive entre conexões.
    pub fn reservar_numero_nfe(
        &self,
        empresa: &Cnpj,
        ambiente: TpAmb,
        serie: u32,
    ) -> Result<u64, ErroEmissor> {
        let numero: i64 = self.conn.query_row(
            "INSERT INTO numeracao (empresa, ambiente, modelo, serie, ultimo)
             VALUES (?1, ?2, '55', ?3, 1)
             ON CONFLICT(empresa, ambiente, modelo, serie)
             DO UPDATE SET ultimo = ultimo + 1
             RETURNING ultimo",
            params![empresa.as_str(), ambiente_str(ambiente), i64::from(serie)],
            |row| row.get(0),
        )?;
        u64::try_from(numero).map_err(|_| ErroEmissor::Journal("numeração negativa".to_owned()))
    }

    /// Reserva o próximo número da DPS para empresa/ambiente/série.
    ///
    /// A numeração da NFS-e é independente da NF-e e atômica no SQLite,
    /// inclusive entre conexões. O chamador deve informar uma série de 1 a
    /// 49999 (E0010).
    pub fn reservar_numero_nfse(
        &self,
        empresa: &Cnpj,
        ambiente: TpAmb,
        serie: u32,
    ) -> Result<u64, ErroEmissor> {
        let numero: i64 = self.conn.query_row(
            "INSERT INTO numeracao (empresa, ambiente, modelo, serie, ultimo)
             VALUES (?1, ?2, 'NFSE', ?3, 1)
             ON CONFLICT(empresa, ambiente, modelo, serie)
             DO UPDATE SET ultimo = ultimo + 1
             RETURNING ultimo",
            params![empresa.as_str(), ambiente_str(ambiente), i64::from(serie)],
            |row| row.get(0),
        )?;
        u64::try_from(numero)
            .map_err(|_| ErroEmissor::Journal("numeração da NFS-e negativa".to_owned()))
    }

    /// Cursor de distribuição do ADN, começando em zero.
    pub fn adn_cursor(&self, empresa: &Cnpj, ambiente: TpAmb) -> Result<i64, ErroEmissor> {
        Ok(self
            .conn
            .query_row(
                "SELECT ultimo_nsu FROM adn_cursor WHERE empresa = ?1 AND ambiente = ?2",
                params![empresa.as_str(), ambiente_str(ambiente)],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    /// Persiste documentos e avança o cursor na mesma transação.
    ///
    /// O cursor só é atualizado depois que os documentos foram inseridos. NSUs
    /// repetidos são ignorados. Retorna quantos documentos eram novos.
    pub fn adn_persistir_lote(
        &mut self,
        empresa: &Cnpj,
        ambiente: TpAmb,
        documentos: &[DocumentoDistribuido],
        novo_cursor: u64,
    ) -> Result<usize, ErroEmissor> {
        let tx = self
            .conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let agora = agora();
        let mut novos = 0_usize;
        for documento in documentos {
            let inseridos = tx.execute(
                "INSERT OR IGNORE INTO adn_documentos
                 (empresa, ambiente, nsu, chave_acesso, tipo_documento, tipo_evento, xml,
                  data_hora_geracao, criado_em)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    empresa.as_str(),
                    ambiente_str(ambiente),
                    i64::try_from(documento.nsu)
                        .map_err(|_| ErroEmissor::Journal("NSU fora de i64".to_owned()))?,
                    documento.chave_acesso,
                    documento.tipo_documento.as_str(),
                    documento.tipo_evento,
                    documento.xml,
                    documento.data_hora_geracao,
                    agora
                ],
            )?;
            novos += inseridos;
        }
        tx.execute(
            "INSERT INTO adn_cursor (empresa, ambiente, ultimo_nsu, atualizado_em)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(empresa, ambiente) DO UPDATE SET
                 ultimo_nsu = MAX(ultimo_nsu, excluded.ultimo_nsu),
                 atualizado_em = excluded.atualizado_em",
            params![
                empresa.as_str(),
                ambiente_str(ambiente),
                i64::try_from(novo_cursor)
                    .map_err(|_| ErroEmissor::Journal("cursor fora de i64".to_owned()))?,
                agora
            ],
        )?;
        tx.commit()?;
        Ok(novos)
    }

    /// Documentos do ADN já persistidos, ordenados por NSU.
    pub fn adn_documentos(
        &self,
        empresa: &Cnpj,
        ambiente: TpAmb,
    ) -> Result<Vec<DocumentoAdnArmazenado>, ErroEmissor> {
        let mut stmt = self.conn.prepare(
            "SELECT nsu, chave_acesso, tipo_documento, tipo_evento, xml, data_hora_geracao
             FROM adn_documentos WHERE empresa = ?1 AND ambiente = ?2 ORDER BY nsu",
        )?;
        let linhas = stmt
            .query_map(params![empresa.as_str(), ambiente_str(ambiente)], |row| {
                Ok(DocumentoAdnArmazenado {
                    nsu: row.get(0)?,
                    chave_acesso: row.get(1)?,
                    tipo_documento: row.get(2)?,
                    tipo_evento: row.get(3)?,
                    xml: row.get(4)?,
                    data_hora_geracao: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(linhas)
    }

    /// Total de operações da empresa no ambiente, para diagnóstico e testes.
    pub fn total_operacoes(&self, empresa: &Cnpj, ambiente: TpAmb) -> Result<i64, ErroEmissor> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM operacoes WHERE empresa = ?1 AND ambiente = ?2",
            params![empresa.as_str(), ambiente_str(ambiente)],
            |row| row.get(0),
        )?)
    }
}

fn ler_linha(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<Operacao, ErroEmissor>> {
    let empresa: String = row.get(1)?;
    let ambiente: String = row.get(2)?;
    let tipo: String = row.get(3)?;
    let estado: String = row.get(6)?;
    let empresa = match Cnpj::parse(&empresa) {
        Ok(cnpj) => cnpj,
        Err(erro) => return Ok(Err(ErroEmissor::Journal(erro.to_string()))),
    };
    let ambiente = match ambiente.as_str() {
        "producao" => TpAmb::Producao,
        "homologacao" => TpAmb::Homologacao,
        outro => {
            return Ok(Err(ErroEmissor::Journal(format!(
                "ambiente desconhecido: {outro}"
            ))));
        }
    };
    let tipo = match TipoOperacao::from_str(&tipo) {
        Ok(tipo) => tipo,
        Err(erro) => return Ok(Err(erro)),
    };
    let estado = match EstadoOperacao::from_str(&estado) {
        Ok(estado) => estado,
        Err(erro) => return Ok(Err(erro)),
    };
    Ok(Ok(Operacao {
        id: row.get(0)?,
        empresa,
        ambiente,
        tipo,
        chave_operacao: row.get(4)?,
        hash_conteudo: row.get(5)?,
        estado,
        xml_preparado: row.get(7)?,
        xml_enviado: row.get(8)?,
        xml_retorno: row.get(9)?,
        chave_nfse: row.get(10)?,
        protocolo: row.get(11)?,
        erro: row.get(12)?,
        criado_em: row.get(13)?,
        atualizado_em: row.get(14)?,
        chave_cliente: row.get(15)?,
    }))
}

fn ambiente_str(ambiente: TpAmb) -> &'static str {
    match ambiente {
        TpAmb::Producao => "producao",
        TpAmb::Homologacao => "homologacao",
    }
}

/// Estados em que corrigir o conteúdo e tentar de novo é seguro.
fn repetivel(estado: &EstadoOperacao) -> bool {
    matches!(
        estado,
        EstadoOperacao::Rejeitada
            | EstadoOperacao::FalhaAntesDoEnvio
            | EstadoOperacao::NaoEncontrada
    )
}

fn agora() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duracao| i64::try_from(duracao.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

/// SHA-256 em hexadecimal minúsculo.
pub fn hash_conteudo(texto: &str) -> String {
    let digest = Sha256::digest(texto.as_bytes());
    let mut saida = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(saida, "{byte:02x}");
    }
    saida
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cnpj() -> Cnpj {
        Cnpj::parse("00000001000136").expect("CNPJ sintético")
    }

    #[test]
    fn cria_uma_vez_e_devolve_a_mesma_operacao() {
        let mut journal = Journal::em_memoria().expect("journal");
        journal.garantir_empresa(&cnpj()).expect("empresa");
        let (primeira, nova) = journal
            .criar(
                &cnpj(),
                TpAmb::Homologacao,
                TipoOperacao::EmitirNfse,
                "DPS1",
                &hash_conteudo("a"),
                "<DPS/>",
            )
            .expect("cria");
        assert!(nova);
        let (segunda, nova) = journal
            .criar(
                &cnpj(),
                TpAmb::Homologacao,
                TipoOperacao::EmitirNfse,
                "DPS1",
                &hash_conteudo("a"),
                "<DPS/>",
            )
            .expect("repete");
        assert!(!nova);
        assert_eq!(primeira.id, segunda.id);
        assert_eq!(
            journal
                .total_operacoes(&cnpj(), TpAmb::Homologacao)
                .expect("total"),
            1
        );
    }

    #[test]
    fn mesmo_id_com_conteudo_diferente_conflita() {
        let mut journal = Journal::em_memoria().expect("journal");
        journal.garantir_empresa(&cnpj()).expect("empresa");
        journal
            .criar(
                &cnpj(),
                TpAmb::Homologacao,
                TipoOperacao::EmitirNfse,
                "DPS1",
                &hash_conteudo("a"),
                "<DPS/>",
            )
            .expect("cria");
        let erro = journal
            .criar(
                &cnpj(),
                TpAmb::Homologacao,
                TipoOperacao::EmitirNfse,
                "DPS1",
                &hash_conteudo("b"),
                "<DPS>outro</DPS>",
            )
            .expect_err("conflito");
        assert!(matches!(erro, ErroEmissor::ConflitoIdempotencia { .. }));
    }

    #[test]
    fn chave_do_cliente_devolve_a_mesma_operacao() {
        let mut journal = Journal::em_memoria().expect("journal");
        journal.garantir_empresa(&cnpj()).expect("empresa");
        let (primeira, nova) = journal
            .criar_com_chave_cliente(
                &cnpj(),
                TpAmb::Homologacao,
                TipoOperacao::EmitirNfse,
                "DPS1",
                Some("venda-1"),
                &hash_conteudo("a"),
                "<DPS/>",
            )
            .expect("cria");
        assert!(nova);
        let (segunda, nova) = journal
            .criar_com_chave_cliente(
                &cnpj(),
                TpAmb::Homologacao,
                TipoOperacao::EmitirNfse,
                "DPS1",
                Some("venda-1"),
                &hash_conteudo("a"),
                "<DPS/>",
            )
            .expect("repete");
        assert!(!nova);
        assert_eq!(primeira.id, segunda.id);
        assert_eq!(segunda.chave_cliente.as_deref(), Some("venda-1"));

        let erro = journal
            .criar_com_chave_cliente(
                &cnpj(),
                TpAmb::Homologacao,
                TipoOperacao::EmitirNfse,
                "DPS1",
                Some("venda-1"),
                &hash_conteudo("b"),
                "<DPS>outro</DPS>",
            )
            .expect_err("conflito");
        assert!(matches!(erro, ErroEmissor::ConflitoIdempotencia { .. }));
    }

    #[test]
    fn chave_do_cliente_vincula_dps_preparada_por_xml() {
        let mut journal = Journal::em_memoria().expect("journal");
        journal.garantir_empresa(&cnpj()).expect("empresa");
        let (operacao, _) = journal
            .criar(
                &cnpj(),
                TpAmb::Homologacao,
                TipoOperacao::EmitirNfse,
                "DPS1",
                &hash_conteudo("a"),
                "<DPS/>",
            )
            .expect("cria por XML");
        let (vinculada, nova) = journal
            .criar_com_chave_cliente(
                &cnpj(),
                TpAmb::Homologacao,
                TipoOperacao::EmitirNfse,
                "DPS1",
                Some("venda-1"),
                &hash_conteudo("a"),
                "<DPS/>",
            )
            .expect("vincula");
        assert!(!nova);
        assert_eq!(vinculada.id, operacao.id);
        assert_eq!(vinculada.chave_cliente.as_deref(), Some("venda-1"));
    }

    #[test]
    fn rejeicao_permite_repreparar_o_mesmo_numero_e_migrar_a_chave() {
        let mut journal = Journal::em_memoria().expect("journal");
        journal.garantir_empresa(&cnpj()).expect("empresa");
        let (operacao, _) = journal
            .criar_com_chave_cliente(
                &cnpj(),
                TpAmb::Homologacao,
                TipoOperacao::EmitirNfse,
                "DPS1",
                Some("venda-1"),
                &hash_conteudo("a"),
                "<DPS/>",
            )
            .expect("cria");
        journal
            .marcar_rejeitada(operacao.id, "E0247: e-mail inválido")
            .expect("rejeita");
        let (reparada, nova) = journal
            .criar_com_chave_cliente(
                &cnpj(),
                TpAmb::Homologacao,
                TipoOperacao::EmitirNfse,
                "DPS1",
                Some("venda-1"),
                &hash_conteudo("b"),
                "<DPS>corrigida</DPS>",
            )
            .expect("reprepara");
        assert!(nova);
        assert_eq!(reparada.id, operacao.id, "reaproveita a linha e o número");
        assert_eq!(reparada.estado, EstadoOperacao::Preparada);
        assert_eq!(reparada.hash_conteudo, hash_conteudo("b"));

        journal
            .marcar_rejeitada(reparada.id, "E0014: número já utilizado")
            .expect("rejeita");
        let (nova_operacao, nova) = journal
            .criar_com_chave_cliente(
                &cnpj(),
                TpAmb::Homologacao,
                TipoOperacao::EmitirNfse,
                "DPS2",
                Some("venda-1"),
                &hash_conteudo("c"),
                "<DPS>outro numero</DPS>",
            )
            .expect("novo número");
        assert!(nova);
        assert_ne!(nova_operacao.id, reparada.id);
        assert_eq!(nova_operacao.chave_cliente.as_deref(), Some("venda-1"));
        assert_eq!(
            journal.operacao(reparada.id).expect("antiga").chave_cliente,
            None
        );
    }

    #[test]
    fn reivindicacao_de_envio_e_atomica() {
        let mut journal = Journal::em_memoria().expect("journal");
        journal.garantir_empresa(&cnpj()).expect("empresa");
        let (operacao, _) = journal
            .criar(
                &cnpj(),
                TpAmb::Homologacao,
                TipoOperacao::EmitirNfse,
                "DPS1",
                &hash_conteudo("a"),
                "<DPS/>",
            )
            .expect("cria");
        assert!(
            journal
                .reivindicar_envio(operacao.id, "<assinada/>")
                .expect("primeira")
        );
        assert!(
            !journal
                .reivindicar_envio(operacao.id, "<assinada/>")
                .expect("segunda")
        );
        assert_eq!(
            journal.operacao(operacao.id).expect("lê").estado,
            EstadoOperacao::Enviada
        );
    }

    #[test]
    fn promocao_e_rebaixamento_ficam_persistidos() {
        let journal = Journal::em_memoria().expect("journal");
        journal.garantir_empresa(&cnpj()).expect("empresa");
        assert_eq!(
            journal.ambiente_empresa(&cnpj()).expect("ambiente"),
            TpAmb::Homologacao
        );
        journal.promover_empresa(&cnpj(), "a1").expect("promove");
        assert_eq!(
            journal.ambiente_empresa(&cnpj()).expect("ambiente"),
            TpAmb::Producao
        );
        assert_eq!(
            journal.certificado_producao(&cnpj()).expect("cert"),
            Some("a1".to_owned())
        );
        journal.rebaixar_empresa(&cnpj()).expect("rebaixa");
        assert_eq!(
            journal.ambiente_empresa(&cnpj()).expect("ambiente"),
            TpAmb::Homologacao
        );
    }
}

#[cfg(test)]
mod tests_concorrencia {
    use super::*;
    use std::path::PathBuf;

    fn caminho() -> PathBuf {
        let unico = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("carimbar-journal-{unico}.db"))
    }

    #[test]
    fn duas_conexoes_reivindicam_o_envio_uma_unica_vez() {
        let caminho = caminho();
        let _ = std::fs::remove_file(&caminho);
        let mut primeira = Journal::abrir(&caminho).expect("primeira conexão");
        let empresa = Cnpj::parse("00000001000136").expect("CNPJ");
        primeira.garantir_empresa(&empresa).expect("empresa");
        let (operacao, _) = primeira
            .criar(
                &empresa,
                TpAmb::Homologacao,
                TipoOperacao::EmitirNfse,
                "DPS-concorrente",
                &hash_conteudo("a"),
                "<DPS/>",
            )
            .expect("cria");
        let mut segunda = Journal::abrir(&caminho).expect("segunda conexão");
        assert!(
            primeira
                .reivindicar_envio(operacao.id, "<assinada/>")
                .expect("primeira")
        );
        assert!(
            !segunda
                .reivindicar_envio(operacao.id, "<assinada/>")
                .expect("segunda")
        );
    }
}

#[cfg(test)]
mod tests_numeracao {
    use super::*;
    use std::collections::BTreeSet;

    fn caminho() -> std::path::PathBuf {
        let unico = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duracao| duracao.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("carimbar-numeracao-{unico}.db"))
    }

    #[test]
    fn duas_conexoes_reservam_numeros_sem_colisao() {
        let caminho = caminho();
        let empresa = Cnpj::parse("00000001000136").expect("CNPJ");
        let primeira = Journal::abrir(&caminho).expect("journal");
        primeira.garantir_empresa(&empresa).expect("empresa");
        let segunda = Journal::abrir(&caminho).expect("journal");
        let numeros = std::thread::scope(|escopo| {
            let a = escopo.spawn(move || {
                let mut lista = Vec::new();
                for _ in 0..10 {
                    lista.push(
                        primeira
                            .reservar_numero_nfe(&empresa, TpAmb::Homologacao, 1)
                            .expect("número"),
                    );
                }
                lista
            });
            let b = escopo.spawn(move || {
                let mut lista = Vec::new();
                for _ in 0..10 {
                    lista.push(
                        segunda
                            .reservar_numero_nfe(&empresa, TpAmb::Homologacao, 1)
                            .expect("número"),
                    );
                }
                lista
            });
            let mut todos = a.join().expect("thread A");
            todos.extend(b.join().expect("thread B"));
            todos
        });
        let unicos: BTreeSet<u64> = numeros.iter().copied().collect();
        assert_eq!(unicos.len(), 20);
        assert_eq!(unicos.iter().next(), Some(&1));
        assert_eq!(unicos.iter().next_back(), Some(&20));
    }

    #[test]
    fn numeracao_da_nfse_e_independente_por_serie_e_da_nfe() {
        let caminho = caminho();
        let empresa = Cnpj::parse("00000001000136").expect("CNPJ");
        let journal = Journal::abrir(&caminho).expect("journal");
        journal.garantir_empresa(&empresa).expect("empresa");
        assert_eq!(
            journal
                .reservar_numero_nfse(&empresa, TpAmb::Homologacao, 1)
                .expect("série 1"),
            1
        );
        assert_eq!(
            journal
                .reservar_numero_nfse(&empresa, TpAmb::Homologacao, 1)
                .expect("série 1"),
            2
        );
        assert_eq!(
            journal
                .reservar_numero_nfse(&empresa, TpAmb::Homologacao, 2)
                .expect("série 2"),
            1
        );
        assert_eq!(
            journal
                .reservar_numero_nfe(&empresa, TpAmb::Homologacao, 1)
                .expect("NF-e"),
            1
        );
    }
}
