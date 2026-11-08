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
| `carimbar-dsig` | XMLDSig enveloped, assinatura e verificação | Pendente (#9) |
| `carimbar-transport` | HTTP mTLS, SOAP 1.2 e JSON, timeouts e classificação de falhas | Pendente (#10) |
| `carimbar-endpoints` | Tabela UF × modelo × ambiente × serviço, versionada e datada | Pendente (#10) |
| `carimbar-vault` | Cofre multiempresa, envelope encryption e auditoria | Pendente (#12) |
| `carimbar-nfse` | DPS, NFSe, eventos, ADN e parametrização municipal | Pendente (#11, #14) |
| `carimbar-nfe` | NF-e 4.00 com IBS/CBS, eventos e contingência SVC | Pendente (#18) |
| `carimbar-danfe` | DANFSe e DANFE em PDF a partir do XML autorizado | Pendente (#15, #19) |
| `carimbar` | Fachada `Emissor`, journal, idempotência e bloqueios de produção | Pendente (#13) |
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

- Perfil planejado: XMLDSig enveloped, referência ao documento inteiro,
  transformação `enveloped-signature` e C14N 1.0 inclusiva sem comentários.
- C14N 1.0 é o perfil usado pelos leiautes fiscais; C14N 1.1 resolve casos de
  subconjunto com `xml:base`/`xml:id` que o MVP não precisa assinar.
- A canonicalização deve preservar prefixos, ordenar declarações de namespace
  por prefixo e atributos por URI de namespace + nome local, e expandir
  referências de caractere.
- Assinatura e verificação serão conferidas com uma implementação independente
  (`xmlsec1`) em testes manuais; o CI usa vetores sintéticos e não depende de
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

## 5. CNPJ alfanumérico e identificadores

- IN RFB 2.229/2024: CNPJ com 12 posições `[0-9A-Z]` e 2 DVs numéricos.
- Letras maiúsculas são aceitas; minúsculas são rejeitadas.
- O DV usa o mesmo módulo 11, com valor de caractere `ASCII - 48`.
- A máscara (` . `, `/`, `-`) é removida na leitura.
- A chave de acesso da NF-e e os identificadores da NFS-e usam os mesmos
  caracteres alfabéticos permitidos pelo contrato correspondente.

O tipo `Cnpj` já implementa e testa essas regras. A integração com certificados
(#8) ainda precisa mapear os OIDs ICP-Brasil que ligam o titular ao CNPJ.

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

## 7. Decisões em aberto

| Tema | Pendência | Issue |
|---|---|---|
| NFS-e produção | Baixar e validar o pacote de produção 2026-02-09 | #5 / #21 |
| NF-e | Conferir NTs posteriores à v1.30 e decidir se há schema novo | #18 |
| Certificados | Mapear OIDs ICP-Brasil de CNPJ e cadeia | #8 |
| Endpoints | Confirmar URLs e versões por UF/ambiente | #10 |
| C14N | Validar vetores com `xmlsec1` e decidir se C14N 1.1 é necessária | #7 / #9 |
| DANFE/DANFSe | Confirmar requisitos de leiaute e fonte de dados | #15 / #19 |
| Vault | Definir formato do arquivo local e política de rotação de chave | #12 |
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
