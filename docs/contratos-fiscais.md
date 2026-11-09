# Contratos fiscais, arquitetura e limites do MVP

Este documento registra o que o MVP pretende suportar e o que já está verificável
no repositório. Ele complementa [o plano do MVP](plano-mvp.md) e a
[issue central #4](https://github.com/seatrunk/carimbar/issues/4).

Consulta às fontes oficiais: **2026-11-08**.

> Regra: este documento descreve contratos e decisões técnicas. Ele não afirma
> que a emissão está implementada. O estado real de cada entrega está nas issues
> e no código.

## 1. Arquitetura

O workspace separa tipos puros, XML, criptografia, transporte, regras fiscais e
interfaces. A direção das dependências é sempre para baixo; nenhuma camada de
domínio conhece CLI, MCP ou Python.

| Crate | Responsabilidade | Estado nesta etapa |
|---|---|---|
| `carimbar-types` | CNPJ (numérico e alfanumérico), chave de acesso, ambiente, dinheiro em `Decimal`, erros de domínio | Implementado |
| `carimbar-xml` | Leitura/escrita determinística, C14N 1.0 e validação XSD opcional | Em implementação (#7) |
| `carimbar-cert` | PKCS#12 A1, cadeia X.509, validade, titular e chave privada protegida | Implementado (#8) |
| `carimbar-dsig` | XMLDSig enveloped, assinatura e verificação | RSA-SHA256 padrão; NF-e/RSA-SHA1 via feature opcional (#9) |
| `carimbar-transport` | HTTP mTLS, SOAP 1.2 e JSON, timeouts e classificação de falhas | Implementado (#10); sem raízes embutidas |
| `carimbar-endpoints` | Tabela UF × modelo × ambiente × serviço, versionada e datada | Implementado (#10); URLs conferidas em 2026-11-08 |
| `carimbar-vault` | Cofre multiempresa, envelope encryption e auditoria | Implementado (#12); só os segredos são cifrados |
| `carimbar-nfse` | DPS, NFS-e, eventos, ADN e parametrização municipal | DPS, emissão, consulta e cancelamento (#11); ADN e parâmetros municipais (#14) |
| `carimbar-nfe` | NF-e 4.00 com IBS/CBS, eventos e contingência SVC | Pendente (#18) |
| `carimbar-danfe` | DANFSe e DANFE em PDF a partir do XML autorizado | DANFSe implementado (#15); DANFE em #19 |
| `carimbar` | Fachada `Emissor`, journal, idempotência e bloqueios de produção | Implementado (#13); NFS-e pela fachada |
| `carimbar-cli` | CLI local | Pendente (#16) |
| `carimbar-mcp` | MCP local em duas fases (`preparar_*`/`confirmar_*`) | Pendente (#17) |
| `carimbar-py` | Binding Python síncrono | Esqueleto; API completa em #20 |

Decisões transversais já registradas no código:

- `unsafe_code` é proibido em todos os crates, exceto as macros do PyO3 em
  `carimbar-py`.
- Valores fiscais usam `Decimal`/`Dinheiro`; `f64` em valor fiscal é bug.
- Produção exige `ConfirmacaoProducao` construída com o CNPJ explícito. Não há
  `Default` nem `Clone` para esse tipo.
- Testes de PR usam dados sintéticos e não acessam SEFAZ, Sefin ou ADN.

## 2. NFS-e Nacional

### 2.1 Pacote vendorizado

| Item | Valor |
|---|---|
| Pasta | `schemas/nfse_v1.01_20261020/` |
| Origem | `https://www.gov.br/nfse/pt-br/biblioteca/documentacao-tecnica/producao-restrita/esquemas_prod_restrita_20261020.zip` |
| Ambiente do pacote | Produção restrita / homologação |
| Versão | v1.01, pacote de 2026-10-20 |
| SHA-256 do zip | `25fdcecc651a67a216496ab10d7566631e6ed10f721e37d62c7845dadb0ea267` |
| Namespace | `http://www.sped.fazenda.gov.br/nfse` |
| Namespace XMLDSig | `http://www.w3.org/2000/09/xmldsig#` |

Schemas incluídos: `DPS_v1.01.xsd`, `NFSe_v1.01.xsd`, `evento_v1.01.xsd`,
`pedRegEvento_v1.01.xsd`, `CNC_v1.00.xsd`, tipos simples/complexos, tipos de
eventos, tipos CNC e `xmldsig-core-schema.xsd`.

### 2.2 Formatos e versões

- `DPS` raiz `DPS`, versão `1.01`, contém `infDPS` e `Signature` opcional.
- Identificador da DPS (`TSIdDPS`), no pacote de produção restrita: literal
  `DPS` + 45 posições: código do município (7) + tipo de inscrição federal (1)
  + inscrição federal (14) + série (5) + número (15). Na produção, o padrão é
  `DPS` + 42 dígitos.
- Chave da NFS-e (`TSChaveNFSe`), produção restrita:
  `[0-9]{6}([0-9A-Z]{14})[0-9]{30}`. Na produção: `[0-9]{50}`.
- O grupo `IBSCBS` da DPS é opcional no schema vendorizado e pertence ao leiaute
  com IBS/CBS da reforma tributária.
- `tpAmb`: `1` produção, `2` homologação.

### 2.3 Produção versus produção restrita

Os dois pacotes estão vendorizados e têm `SHA256SUMS`:

| Ambiente | Pasta | Origem | SHA-256 do zip |
|---|---|---|---|
| Produção restrita / homologação | `schemas/nfse_v1.01_20261020/` | `.../producao-restrita/esquemas_prod_restrita_20261020.zip` (2026-10-20) | `25fdcecc651a67a216496ab10d7566631e6ed10f721e37d62c7845dadb0ea267` |
| Produção | `schemas/nfse_v1.01_20260209/` | `.../documentacao-atual/nfse-esquemas_xsd-v1-01-20260209.zip` (2026-02-09) | `e7935cbd9470527c6cc32984c1b2263e614183bf0139ce2733eaaed2de9a8072` |

Os arquivos da produção foram comparados um a um com a restrita (nomes dos tipos,
padrões e presença de elementos). Diferenças que afetam o código:

- **CNPJ:** a produção aceita só dígitos (`[0-9]{14}`). A restrita aceita o formato
  alfanumérico (`[0-9A-Z]{14}`). Um CNPJ alfanumérico válido na restrita é
  rejeitado pela produção; teste `cnpj_alfanumerico_so_passa_na_producao_restrita`.
- **Identificadores:** DPS e chave da NFS-e mudam de padrão (seção 2.2).
- **`finNFSe`:** existe na restrita e não existe na produção. Uma DPS com esse campo
  é inválida na produção; teste `dps_numerica_sintetica_passa_na_producao` usa a
  versão sem o campo.
- **Ajuste de base de cálculo:** a restrita traz `TCInfoAjusteBC`,
  `TSRTCTpAjusteBC` e tipos de imóveis (`TCRTCInfoBensMoveis`,
  `TCEnderImovel`). A produção traz `TCInfoDedRed` e `TSIdeDedRed` (deduções).
- **IBS/CBS:** ambos os pacotes têm o grupo `TCRTCInfoIBSCBS`.

Consequência de design (para #11 e #13): a escolha do XSD tem de seguir o ambiente
da chamada. Nunca validar ou emitir em produção com o pacote de produção restrita,
nem o contrário. O pacote de produção é o que a fachada usa quando `tpAmb=1`.

## 3. NF-e modelo 55

### 3.1 Pacote vendorizado

| Item | Valor |
|---|---|
| Pasta | `schemas/PL_010b_v1.30/` |
| Origem | `https://dfe-portal.svrs.rs.gov.br/NFE/DownloadArquivoEstatico/?sistema=NFE&tipoArquivo=2&nomeArquivo=PL_010b_NT2025_002_v1.30.zip` |
| Pacote | PL_010b v1.30 (NT 2025.002 v1.30) |
| Data do pacote no portal | 2025-10-07 |
| SHA-256 do zip | `2deaa8d430d0acb47deae06b9b6d1202dbb436baf3d2d3aa834a534a3d6fa1b8` |
| Namespace | `http://www.portalfiscal.inf.br/nfe` |
| Versão do leiaute | 4.00 |

O pacote inclui `nfe_v4.00.xsd`, `leiauteNFe_v4.00.xsd`,
`DFeTiposBasicos_v1.00.xsd`, `tiposBasico_v4.00.xsd` e
`xmldsig-core-schema_v1.01.xsd`.

### 3.2 IBS/CBS e versões posteriores

O pacote vendorizado cobre o leiaute da NT 2025.002 v1.30 com os grupos de IBS e
CBS. Em 2026-11-08, o portal da SVRS lista o pacote PL_010b v1.30 (2025-10-07) como o
pacote de schemas da NT 2025.002, mas a NT 2025.002 já está em v1.53 (2026-10-30)
e há um schema de evento da mesma NT, versão 1.40, publicado em 2026-07-23 e não
vendorizado. Também há o pacote da NT 2022.002 v1.30 (2026-03-20). Portanto, o
PL_010b v1.30 não é o pacote mais recente do leiaute. Notas posteriores relevantes:

- NT 2026.004 v1.01 (CNPJ alfanumérico, homologação em 2026-06-15). O CNPJ
  alfanumérico da NF-e não é conferido nos XSD vendorizados: o PL_010b v1.30 não
  o contém (verificar antes de #18).
- NT 2026.010 v1.00 (DANFE e IBS/CBS).
- NT 2026.002 / 2026.003 (DANFE Simplificado Tipo 2).
- NT 2026.008 (valor líquido do produto).

Decisão: a implementação de NF-e (#18) deve começar por uma conferência entre o
texto das NTs vigentes e o pacote PL_010b v1.30. Se houver schema novo, ele entra
em pasta nova, nunca sobrescrevendo `PL_010b_v1.30/`.

### 3.3 Chave de acesso

A chave da NF-e tem 44 posições:

`cUF(2) + AAMM(4) + CNPJ(14) + mod(2) + série(3) + nNF(9) + tpEmis(1) + cNF(8) + cDV(1)`

O CNPJ pode ser alfanumérico. O DV é calculado por módulo 11 com pesos 2 a 9 da
direita para a esquerda e valor de cada caractere igual a `ASCII - 48`, cobrindo
dígitos e letras `A`–`Z`.

## 4. XMLDSig e canonicalização

- Perfil implementado: XMLDSig enveloped, referência ao elemento fiscal por `#Id`,
  transformação `enveloped-signature` e C14N 1.0 inclusiva sem comentários.
- C14N 1.0 é o perfil usado pelos leiautes fiscais. Ao canonicalizar uma subárvore,
  os atributos `xml:*` são herdados do ancestral mais próximo, inclusive
  `xml:base` e `xml:id`. As regras especiais do C14N 1.1 não são aplicadas.
- A canonicalização deve preservar prefixos, ordenar declarações de namespace
  por prefixo e atributos por URI de namespace + nome local, e expandir
  referências de caractere.
- Assinatura e verificação são conferidas nas duas direções com uma implementação
  independente (`xmlsec1`) em testes locais e no CI, com material sintético e sem
  serviço externo.

Limite conhecido: o parser fiscal rejeita DTD e entidades externas. Por isso,
vetores de C14N que dependem de `<!DOCTYPE ...>` são adaptados para a forma sem
DTD. Isso é intencional: XML fiscal não deve carregar DTD.

### 4.1 Implementação atual e schemas distribuídos

`carimbar-xml` implementa leitura e escrita determinísticas, C14N 1.0 inclusiva
sem comentários e validação XSD opcional (feature `xsd`).

Decisão sobre schemas em pacotes distribuídos: **os XSD oficiais não são
embutidos no crate publicado**. `carimbar_xml::xsd::validar` recebe o caminho do
XSD principal e resolve `xs:include`/`xs:import` relativos a ele. Aplicações e o
workspace vendorizam o pacote oficial em `schemas/<pacote>/` com `SHA256SUMS`; o
teste `carimbar-xml/tests/schemas_sha256.rs` verifica a integridade no CI. Isso
evita redistribuir schema oficial dentro de um pacote sem controle de versão e
mantém o crate pequeno.

Evidência de interoperabilidade nesta etapa:

- vetores 3.1, 3.3 e 3.4 do W3C C14N, adaptados somente para remover DTD;
- comparação manual com `xmllint --c14n` (com comentários) em documento fiscal
  sintético, sem diferenças fora dos comentários omitidos de propósito;
- DPS sintética completa validada contra `DPS_v1.01.xsd` oficial (restrita) e
  versão sem `finNFSe` validada contra o da produção;
- DPS e NF-e inválidas rejeitadas com diagnóstico `cvc-*`.

### 4.2 Perfis XMLDSig implementados (#9)

`carimbar-dsig` assina e verifica os perfis abaixo. Qualquer outro algoritmo é recusado.

| Item | Valor |
|---|---|
| Assinatura | `Signature` filha do elemento raiz (`NFe`/`DPS`), após o elemento assinado |
| Referência | `URI="#Id"`, `Id` único; alvo deve ser filho direto da raiz |
| Transformadas | Exatamente `enveloped-signature` seguida de C14N 1.0 inclusiva, sem parâmetros |
| Canonicalização | C14N 1.0 inclusiva, sem comentários |
| Digest | SHA-256 por padrão; SHA-1 no perfil explícito `NfeSha1` |
| Assinatura | RSASSA-PKCS1-v1_5 com SHA-256 por padrão; RSA-SHA1 no perfil explícito de NF-e |
| `KeyInfo` | `X509Data/X509Certificate` com a folha |

Decisões desta etapa:

- **Perfil de assinatura NF-e validado.** O arquivo oficial vendorizado
  `schemas/PL_010b_v1.30/xmldsig-core-schema_v1.01.xsd` fixa `SignatureMethod`
  em RSA-SHA1 e `DigestMethod` em SHA-1. A feature `nfe-sha1` habilita
  `PerfilAssinatura::NfeSha1`, usado explicitamente por `assinar_com_perfil` e
  `verificar_com_perfil`. O teste extrai a `Signature` e a valida contra esse XSD;
  a assinatura SHA-256 é rejeitada pelo mesmo schema. A modelagem e validação do
  XML completo da NF-e pertencem à #18. Não há fallback nem detecção automática:
  `assinar`/`verificar` continuam exigindo SHA-256 mesmo com a feature ligada.
- **Exceção de backend restrita ao legado.** `ring` não assina RSA-SHA1. Somente
  a feature opcional `carimbar-cert/nfe-sha1` usa OpenSSL para essa operação;
  SHA-256 e a verificação RSA de ambos os perfis permanecem no `ring`. A exceção
  de `openssl-sys` está registrada em `deny.toml`, com wrapper `openssl`, e o CI
  confere que o grafo padrão não contém OpenSSL. O build legado requer headers
  e bibliotecas OpenSSL do sistema (`libssl-dev` e `pkg-config` no Ubuntu).
  Se o backend/política criptográfica recusar SHA-1, retorna erro sem tentar
  outro algoritmo. A alternativa RustCrypto RSA não foi adicionada: o banco
  RustSec ainda a lista em RUSTSEC-2023-0071 sem versão corrigida.
- **Escopo da evidência.** XSD e interoperabilidade local não demonstram aceitação
  pelo Sefin/SEFAZ. Emissão e homologação continuam nas #11, #18 e #21.
- **Confiança não é verificada.** `verificar` prova integridade e autoria pela
  chave do certificado em `KeyInfo`. Cadeia, validade no tempo e revogação ficam
  com quem usa o resultado (`carimbar-cert` e a política de confiança). O consumidor
  também compara o `Id` verificado com o esperado e não trata irmãos do elemento
  referenciado como conteúdo autenticado.
- **Chave privada fora do crate.** A assinatura passa pela trait `Signer`;
  `CertificadoA1` a implementa e nunca expõe a chave.

Evidência de interoperabilidade: documentos assinados pelo `carimbar` são
verificados pelo `xmlsec1` (com `--trusted-pem`), e assinaturas geradas pelo
`xmlsec1` são verificadas pelo `carimbar`, nos dois perfis. A DPS SHA-256 passa no
XSD oficial; a `Signature` NF-e SHA-1 passa no XSD específico de assinatura.

O verificador recusa IDs duplicados, alvo movido para níveis internos, múltiplas
assinaturas, transformadas ausentes/repetidas/fora de ordem, parâmetros não
suportados, elementos em namespaces incorretos e certificado DER com conteúdo
excedente. Assinar novamente um documento já assinado também é recusado.

## 5. CNPJ alfanumérico e identificadores

- IN RFB 2.229/2024: CNPJ com 12 posições `[0-9A-Z]` e 2 DVs numéricos.
- Letras maiúsculas são aceitas; minúsculas são rejeitadas.
- O DV usa o mesmo módulo 11, com valor de caractere `ASCII - 48`.
- A máscara (` . `, `/`, `-`) é removida na leitura.
- A chave de acesso da NF-e e os identificadores da NFS-e usam os mesmos
  caracteres alfabéticos permitidos pelo contrato correspondente.

O tipo `Cnpj` implementa e testa essas regras. O `carimbar-cert` (#8) já lê o
CNPJ do titular pelo OID ICP-Brasil `2.16.76.1.3.3` e valida o vínculo na abertura.

## 6. Ambientes e segurança

- `tpAmb = 1`: produção. Exige confirmação explícita e nunca roda no CI.
- `tpAmb = 2`: homologação/produção restrita. É o alvo dos testes de integração.
- CI usa `CARIMBAR_PRODUCAO=0` como kill switch.
- Nenhum certificado, senha, `.env` ou dado real de empresa é versionado. O
  script `scripts/check-no-certs.sh` verifica isso.
- Chaves privadas devem ficar em memória zerável e nunca aparecer em `Debug`,
  logs ou mensagens de erro.
- Chamadas governamentais ficam restritas a job manual com environment
  protegido (#21).

## 6.1 Resolução de endpoints (#10)

`carimbar-endpoints` resolve a URL localmente, sem rede e sem certificado. Cada
resultado carrega versão do serviço, autorizador, fonte oficial e a data
`VERIFICADO_EM` (`2026-11-08`).

- **NF-e:** autorizador normal por UF; `SVC-AN`/`SVC-RS` mediante contingência
  explícita; distribuição sempre no Ambiente Nacional (`www1`/`hom1`).
- **NFS-e:** Sefin Nacional para emissão, consulta de NFS-e, eventos e DPS; ADN
  para distribuição por NSU, eventos por chave e DANFSe.
- **Combinações não publicadas** retornam erro explícito: consulta de cadastro
  fora das UFs atendidas, contingência que não cobre a UF e serviço inexistente.
- **Ambiente:** o `tpAmb` vem de `carimbar_types::Ambiente`; produção e
  homologação usam hosts distintos e um teste garante que a URL de produção não
  reaproveita host de homologação.
- **Limite:** a tabela é dado, não homologação. Notas técnicas mudam URLs;
  reconfira a fonte antes de emitir. Sefins municipais fora do padrão nacional
  não são resolvidas.

## 6.2 Transporte mTLS (#10)

`carimbar-transport` executa a chamada HTTP sobre TLS mútuo, com a identidade
A1 e as raízes de confiança fornecidas pelo chamador em DER. Nenhuma raiz é
embutida no crate, então o build não carrega a cadeia ICP-Brasil por conta
própria.

- **Protocolo:** HTTP/1.1 mínimo e estrito, sem redirecionamento, sem proxy e
  sem reuso de conexão (`Connection: close`). Respostas `chunked`,
  `Content-Length` e leitura até o fim são suportadas.
- **Limites:** 8 MiB de resposta por padrão, até 100 cabeçalhos e linha de
  status/cabeçalho de 8 KiB. Corpo maior que o limite é recusado.
- **SOAP:** `Requisicao::post_soap12` monta o `Envelope` SOAP 1.2 e envia a ação
  no parâmetro `action` do `Content-Type`. Ação com aspa ou quebra de linha é
  recusada.
- **JSON:** `post_json` e `get_json` para Sefin/ADN, sem conversão de corpo: o
  chamador entrega os bytes já serializados.
- **Classificação de falhas:** `ErroEnvio::AntesDoEnvio` cobre URL, DNS, TCP,
  handshake TLS e montagem do cabeçalho; `ErroEnvio::ResultadoDesconhecido`
  cobre qualquer falha depois do início da escrita. Envio de emissão usa
  `executar`, que nunca repete sozinho.
- **Retry:** `executar_seguro` só deve ser usado em operações idempotentes;
  repete falhas transitórias de `AntesDoEnvio` e respostas 429/5xx até o limite
  de tentativas configurado.
- **Teste:** a suíte de integração gera uma PKI sintética com `openssl` (duas
  CAs, servidor e clientes), sobe um servidor TLS local e cobre mTLS, CA
  desconhecida, identidade de cliente recusada, timeout, chunked, limite de
  resposta, `HEAD`/204 e retry. Nenhuma conexão sai da máquina de teste.

Limitações assumidas: sem proxy corporativo, sem OCSP/CRL e sem pin de
certificado no MVP; a resolução DNS usa o resolvedor do sistema e não tem
timeout próprio.

## 6.3 Cofre multiempresa (#12)

`carimbar-vault` guarda certificados A1 em SQLite local com envelope encryption
e isolamento por empresa.

- **DEK por empresa:** cada CNPJ tem uma chave de dados de 32 bytes. O PFX e a
  senha do certificado são cifrados com AES-256-GCM sob essa DEK, com CNPJ,
  identificador e versão no contexto autenticado; trocar empresa, identificador
  ou versão faz a decifragem falhar.
- **KEK fora do arquivo:** a chave mestra embrulha as DEKs e nunca é gravada.
  Ela vem do chamador (`ChaveMestra::de_bytes`/`gerar`) ou é derivada de senha
  com Argon2id; nesse caso o arquivo guarda apenas sal e custo, e reabrir com
  senha errada retorna `ChaveIncorreta` sem decifrar registro algum.
- **Metadados em claro:** CNPJ, identificador, titular, validade, SHA-256 do
  certificado folha e a trilha de auditoria. PFX, senha, DEK e chave privada
  ficam cifrados; o teste de repouso procura os bytes originais no `.db` depois
  do checkpoint do WAL.
- **Rotação:** `rotacionar_dek` gera nova DEK e recifra todos os certificados da
  empresa na mesma transação; `rotacionar_kek` reembrulha as DEKs de todas as
  empresas após validar a nova KEK. Em falha, o cofre permanece com a
  configuração anterior e o teste injeta erro no meio do reembrulho para provar
  que não sobra estado parcial.
- **KMS opcional:** `ProvedorKek` é a interface mínima (`selar`/`abrir` com
  AAD). Um provedor externo pode implementá-la; o MVP não traz integração
  pronta nem keyring do sistema.
- **Auditoria encadeada:** cada evento guarda o SHA-256 do anterior sobre campos
  delimitados por tamanho. A cadeia detecta alteração de campo, remoção
  intermediária e reordenação sem recálculo. Limites: não detecta remoção do
  último registro nem reescrita completa da cadeia por quem tem escrita no
  arquivo; para garantia mais forte, ancore o último hash fora do cofre.

Limitações assumidas: o arquivo SQLite não é cifrado como um todo (apenas os
segredos), o cofre é local e serializa escritas, e não substitui backup,
replicação ou um KMS dedicado.

## 6.4 DPS, emissão e eventos da NFS-e (#11)

`carimbar-nfse` monta a DPS v1.01, assina com XMLDSig SHA-256 e fala com as APIs
JSON da Sefin Nacional.

- **Id derivado:** o `Id` da DPS é calculado de município, tipo de inscrição,
  inscrição federal, série, número e ambiente. Não existe campo separado que
  possa divergir do XML. Em produção, CNPJ alfanumérico é recusado pelo padrão
  publicado; na produção restrita ele é aceito.
- **Diferença de pacotes:** a DPS de homologação inclui `<finNFSe>0</finNFSe>`,
  que não existe no XSD de produção. A DPS de produção é validada contra o
  pacote `nfse_v1.01_20260209` e a de homologação contra
  `nfse_v1.01_20261020`.
- **Assinatura:** perfil SHA-256 de `carimbar-dsig`, sobre `infDPS` (DPS) e
  `infPedReg` (evento). A chave privada continua fora do crate.
- **API:** requisições `POST /nfse` e `POST /nfse/{chave}/eventos` levam o XML
  em GZip/base64. A resposta é lida com limite de 8 MiB descompactados. O cliente
  interpreta autorização, rejeição fiscal, duplicidade (HTTP 409) e resultado
  desconhecido. Emissão nunca é repetida automaticamente; para reconciliar há
  `GET /dps/{idDps}` e `GET /nfse/{chave}`.
- **Eventos:** o MVP implementa o cancelamento (`e101101`). O `Id` do pedido é
  `PRE` + chave + tipo, sem o número sequencial; o `EVT...` gerado pelo ambiente
  não é produzido localmente.
- **Limite do XSD restrito:** o `TSIdPedRegEvt` do pacote de produção restrita
  de 2026-10-20 tem um padrão que rejeita o Id documentado para CNPJs cujo
  terceiro dígito não é 1 ou 2. O pacote de produção corrigiu o padrão para
  `PRE[0-9]{56}`. Os testes validam a estrutura do pedido contra o pacote de
  produção e registram essa divergência; a emissão real de eventos continua no
  fluxo manual de homologação (#21).
- **Escopo:** endereço, telefone, comércio exterior, obra, evento, benefício
  municipal e os grupos de crédito presumido/diferimento do IBS/CBS não estão
  modelados nesta entrega.

## 6.5 Fachada, journal e produção (#13)

`carimbar` expõe `Emissor`, que coordena cofre, journal, assinatura e transporte
sem duplicar regras entre CLI, MCP e Python.

- **Duas fases:** `preparar_nfse` valida a DPS, calcula o hash e persiste a
  operação como `preparada`; `confirmar_nfse` assina, reivindica o envio com um
  `UPDATE ... WHERE estado IN (...)` e chama a Sefin uma vez.
- **Idempotência:** chave única por empresa, ambiente, tipo e Id da DPS. Mesmo
  conteúdo devolve a operação existente; conteúdo diferente retorna
  `ConflitoIdempotencia`. O journal também guarda XML preparado, XML enviado,
  XML autorizado, chave e protocolo.
- **Resultado desconhecido:** uma falha depois do início da escrita persiste o
  estado `desconhecida`; `reconciliar_nfse` consulta `GET /dps/{id}` e, se
  houver NFS-e, busca `GET /nfse/{chave}`. Só depois de uma reconciliação que
  confirme ausência (`nao_encontrada`) um novo `confirmar_nfse` pode reenviar.
- **Reinício:** o journal e o cofre são arquivos locais; reabrir a fachada
  retoma operações `autorizada`, `rejeitada`, `enviada` e `desconhecida` sem
  perder o estado.
- **Produção:** `promover_producao` exige `ConfirmacaoProducao` do CNPJ da
  empresa, certificado pertencente a ela e kill switch liberado. O kill switch
  lê `CARIMBAR_PRODUCAO` a cada operação de produção; `0`, `false` ou `nao`
  bloqueiam. A promoção é administrativa e não é exposta pelo MCP.
- **Limites:** o journal não processa segredos; o cofre continua sendo a única
  fonte de certificado. A reconciliação de cancelamento depende de uma consulta
  de eventos que ainda não foi implementada; cancelamentos com resultado
  desconhecido ficam nesse estado até a próxima entrega.


## 6.6 Distribuição ADN e parâmetros municipais (#14)

`carimbar-nfse` implementa a distribuição de DF-e do ADN e a API de
parametrização municipal; a fachada persiste o cursor e os documentos.

- **Distribuição:** `GET /contribuintes/DFe/{NSU}` aceita 400/404 com corpo
  válido e usa `StatusProcessamento` como sinal. Cada documento traz
  `ArquivoXml` em GZip/base64 e é descompactado com o mesmo limite de 8 MiB.
- **Cursor:** o cursor é por empresa e ambiente. Documentos e cursor são
  gravados na mesma transação; o cursor só avança depois dos inserts. NSU
  repetido é ignorado, e falha de transporte/escrita deixa o cursor anterior.
- **Retomada:** `Emissor::sincronizar_adn` limita o número de páginas, permite
  reinício e expõe os documentos persistidos.
- **Parâmetros:** convênio, alíquota/histórico, benefício, regimes especiais e
  retenções usam a base `adn.../parametrizacao`. O cache é em memória, com TTL,
  chave por ambiente/serviço/parâmetros e atualização forçada; convênio e
  alíquota têm tipos próprios, e os demais retornam o JSON bruto.
- **Ausência:** convênio ou alíquota sem parâmetro retorna
  `ParametroAusente`, em vez de tratar o corpo 400/404 como sucesso vazio.
- **Limites:** o cache não é persistente entre processos; os demais parâmetros
  ainda não têm tipos completos. A sincronização não interpreta semanticamente
  cada evento, apenas preserva o XML e o tipo para as camadas seguintes.


## 6.7 DANFSe em PDF (#15)

`carimbar-danfe` gera o DANFSe offline a partir do XML da NFS-e autorizada.

- **Entrada:** exige raiz `NFSe` com `infNFSe` e `cStat` autorizado (`100` ou
  `101`). DPS isolada e XML com outro status são recusados.
- **Transcrição:** chave, número, processamento, municípios, emitente, tomador,
  serviço, valores, base, alíquota, ISSQN e líquido vêm do XML, sem recálculo.
- **Layout:** A4, cabeçalho, seções, rodapé numerado, quebra de texto longo e
  múltiplas páginas. Homologação é marcada; cancelamento pode vir do `cStat`,
  do evento (`e101101`/`e105102`) ou de opção explícita.
- **QR:** aponta para `https://www.nfse.gov.br/consultapublica?chave=...`. A URL
  deve ser reconferida antes de produção.
- **Decisão de backend:** o esqueleto previa Typst. O MVP usa `printpdf` com
  fontes padrão Helvetica para evitar o runtime e a fonte empacotada do Typst.
  O PDF é offline e determinístico, mas a cobertura de glifos é a das fontes
  padrão. Uma troca para Typst pode ser feita sem mudar a API pública.
- **Integração:** `Emissor::gerar_danfse` só gera para operação `autorizada`,
  usando o XML persistido no journal; não há nova chamada à Sefin.

## 7. Decisões em aberto

| Tema | Pendência | Issue |
|---|---|---|
| NFS-e produção | Baixar e validar o pacote de produção 2026-02-09 | #5 / #21 |
| NF-e | Conferir NTs posteriores à v1.30 e decidir se há schema novo | #18 |
| Certificados | Mapear OIDs ICP-Brasil de CNPJ e cadeia | #8 |
| Endpoints | Reconfirmar URLs antes de emitir; a tabela local tem fonte e data | #10 |
| C14N | Validar vetores com `xmlsec1` e decidir se C14N 1.1 é necessária | #7 / #9 |
| DANFE/DANFSe | Confirmar requisitos de leiaute e fonte de dados | #15 / #19 |
| Python | Definir se a API inicial expõe só NFS-e ou também NF-e | #20 |

## 8. Fora do MVP

Ficam para a fase 2, sem bloquear as entregas atuais:

- NFC-e, modelo 65.
- Servidor HTTP e MCP remoto.
- Fila, webhooks e contingência além da SVC da NF-e.
- Camada self-hosted AGPL-3.0.
- Provedores externos de KMS; o cofre pode prever a interface, mas a entrega
  local não depende deles.

## 9. Fontes

- Portal NFS-e — documentação de produção:
  `https://www.gov.br/nfse/pt-br/biblioteca/documentacao-tecnica/documentacao-atual`
  (atualizado em 2026-11-04; acesso em 2026-11-08).
- Portal NFS-e — documentação técnica de homologação/produção restrita:
  `https://www.gov.br/nfse/pt-br/biblioteca/documentacao-tecnica/producao-restrita`
  (atualizado em 2026-10-20; acesso em 2026-11-08).
- Portal NF-e/SVRS — documentos e schemas:
  `https://dfe-portal.svrs.rs.gov.br/NFe/Documentos` (acesso em 2026-11-08).
- Pacotes vendorizados e seus SHA-256: [schemas/README.md](../schemas/README.md).
- IN RFB 2.229/2024: citada pelo domínio `Cnpj` (código da Etapa 0). Não foi
  reconferida nesta revisão; a Nota Técnica Conjunta DFe — CNPJ Alfanumérico (v1.00,
  2025-05-07) aparece na lista de NTs do portal NF-e.
- Pacote de produção NFS-e: `nfse-esquemas_xsd-v1-01-20260209.zip`, baixado em
  2026-11-08 de `https://www.gov.br/nfse/pt-br/biblioteca/documentacao-tecnica/documentacao-atual`.
