# Plano de implementação do MVP

Issue central: [#4](https://github.com/seatrunk/carimbar/issues/4).

Base de planejamento: auditoria da `main` no commit `0abd67f`.
Este documento é um plano novo, derivado do estado do código e do escopo do README;
não substitui silenciosamente decisões do plano histórico ausente
`carimbar-mvp-arquitetura-2026-11-04.md`.

## Objetivo e estado inicial

Entregar emissão direta de NFS-e Nacional e NF-e modelo 55, com núcleo Rust,
cofre multiempresa A1, PDFs, CLI, MCP local e binding Python síncrono.

A fundação já contém tipos de domínio, schemas oficiais com verificação de
integridade, validações Python e infraestrutura de CI/release. Os módulos de
emissão e suas interfaces ainda são stubs. Nenhuma etapa funcional abaixo é
considerada concluída pela simples existência dos crates ou por CI verde.

## Sequência de implementação

A ordem indica marcos de entrega, não uma obrigação de serializar trabalhos
independentes. As dependências específicas de cada issue prevalecem.
Cada issue pode resultar em vários PRs pequenos.

### Etapa 0. Contratos fiscais e distribuição

Confirmar versões oficiais, ambientes e decisões da arquitetura; recuperar a publicação inicial em uma frente independente.

- [x] [#5](https://github.com/seatrunk/carimbar/issues/5) — Documentar contratos fiscais, arquitetura e limites do MVP.
- [ ] [#6](https://github.com/seatrunk/carimbar/issues/6) — Concluir e tornar recuperável a publicação inicial 0.0.1.

Critério do marco: Contratos e limites documentados; procedimento de release verificável. A publicação 0.0.1 não é condição para começar o desenvolvimento nem comprova prontidão fiscal.

### Etapa 1. XML, certificados e assinaturas

Implementar XML e leitura segura de A1; integrar XMLDSig depois que suas duas dependências estiverem prontas.

- [x] [#7](https://github.com/seatrunk/carimbar/issues/7) — Implementar leitura, escrita, canonicalização e validação XSD de XML fiscal.
- [x] [#8](https://github.com/seatrunk/carimbar/issues/8) — Implementar leitura segura e validação de certificados A1 PKCS#12.
- [x] [#9](https://github.com/seatrunk/carimbar/issues/9) — Implementar assinatura e verificação XMLDSig interoperáveis.

Entrega da #9: RSA-SHA256 por padrão e perfil RSA-SHA1/SHA-1 de NF-e via feature
opcional `nfe-sha1`, ambos com interoperabilidade via `xmlsec1`. O perfil legado
exige escolha explícita, usa OpenSSL apenas para assinar e é validado contra o
XSD de assinatura da NF-e. Isso não implementa nem homologa a emissão fiscal.

Critério do marco: XML validado por XSD e assinatura verificada por implementação independente, com material de teste sintético.

### Etapa 2. Conectividade e cofre

Implementar endpoints e mTLS, além do cofre local com isolamento por empresa. Ambas as entregas dependem de certificados; o cofre pode avançar assim que essa dependência estiver pronta.

- [x] [#10](https://github.com/seatrunk/carimbar/issues/10) — Implementar endpoints versionados e transporte mTLS SOAP/JSON.
- [x] [#12](https://github.com/seatrunk/carimbar/issues/12) — Implementar cofre multiempresa com criptografia e auditoria.

Entrega da #12: cofre SQLite com DEK por empresa em AES-256-GCM e KEK fora do
arquivo (chamador ou Argon2id). Importação valida o título do A1 contra o CNPJ,
listagem expõe só metadados públicos, e remoção/rotação de DEK e de KEK são
transacionais. A auditoria é encadeada por SHA-256 com limites de detecção
documentados. A interface `ProvedorKek` deixa KMS externo para incremento
posterior. Testes usam PKI sintética e cobrem isolamento entre empresas,
adulteração, rotação e falha de escrita.

Entrega da #10: tabela de endpoints versionada e datada (`carimbar-endpoints`) e
cliente HTTP/1.1 sobre mTLS (`carimbar-transport`) com SOAP 1.2 e JSON. O
transporte não embute raízes, recebe a identidade A1 e as raízes em DER, e
separa `AntesDoEnvio` de `ResultadoDesconhecido` para não repetir emissão.
Testes de integração usam PKI sintética e servidor TLS local.

Critério do marco: Comunicação mTLS demonstrada com servidor local; certificado cifrado em repouso e recuperável apenas com a chave correta.

### Etapa 3. Primeiro fluxo NFS-e pela biblioteca

Construir DPS, emissão, consulta e eventos; integrar Emissor, journal, idempotência e bloqueios de produção; acrescentar parâmetros municipais e sincronização ADN.

- [x] [#11](https://github.com/seatrunk/carimbar/issues/11) — Implementar DPS e emissão, consulta e eventos da NFS-e Nacional.
- [x] [#13](https://github.com/seatrunk/carimbar/issues/13) — Implementar Emissor, journal, idempotência e bloqueios de produção.
- [x] [#14](https://github.com/seatrunk/carimbar/issues/14) — Implementar distribuição ADN por NSU e parametrização municipal.

Entrega da #11: modelo da DPS v1.01 com Id derivado dos campos, diferenças de
produção e produção restrita, grupos obrigatórios e IBS/CBS, serialização na
ordem do XSD e assinatura XMLDSig SHA-256. O cliente Sefin envia GZip/base64,
interpreta autorização, rejeição, duplicidade e resultado desconhecido, e expõe
consulta de NFS-e e de DPS para reconciliação. O cancelamento é o evento
implementado; os demais tipos de evento ficam para incremento. Testes usam
respostas simuladas e os XSD oficiais, sem acessar Sefin/ADN.

Entrega da #13: fachada `Emissor` sobre cofre, journal e transporte. O journal
SQLite persiste operação, XML preparado/enviado, XML autorizado, chave e
protocolo, com estados explícitos. A idempotência usa chave única por empresa,
ambiente e operação e hash do conteúdo; o envio é reivindicado por `UPDATE`
atômico, e resultado desconhecido só é reenviado após reconciliação por
`GET /dps/{id}`. Produção exige promoção explícita com `ConfirmacaoProducao`
vinculada ao CNPJ, certificado da empresa e kill switch `CARIMBAR_PRODUCAO`.
Testes cobrem repetição, conteúdo divergente, reinício, timeout, concorrência
entre conexões, kill switch e divergência de CNPJ. O primeiro fluxo local de
NFS-e roda com transporte simulado.

Entrega da #14: cliente ADN para `GET /contribuintes/DFe/{NSU}` com leitura
PascalCase, GZip/base64, tratamento de 400/404 com corpo válido e cursor por
empresa/ambiente. A sincronização persiste documentos e cursor na mesma
transação, deduplica NSU, retoma após reinício, respeita limite de páginas e
mantém o cursor anterior em falha. A API de parâmetros municipais cobre convênio,
alíquota/histórico, benefício, regimes especiais e retenções, com cache por
ambiente/município, TTL e atualização explícita. A fachada expõe essas operações
sem duplicar transporte.

Entrega da #16: CLI `carimbar` com comandos de empresa (importar/listar/
promover/rebaixar), emissão de DPS XML, consulta, cancelamento, DANFSe, ADN e
parâmetros municipais. A senha do PFX vem de `CARIMBAR_SENHA_PFX` e a KEK de
`CARIMBAR_KEK`; segredos não entram em argumentos. A saída pode ser humana ou
JSON, com códigos de saída estáveis (2 uso, 3 fiscal, 4 transporte/desconhecido,
5 produção bloqueada, 6 conflito). Emissão usa a fachada (journal, idempotência
e kill switch) e o fluxo completo roda com transporte replay em build de
depuração. `--help` e `--version` funcionam.

Entrega da #17: servidor MCP stdio com `rmcp`, ferramentas de consulta, emissão,
cancelamento, DANFSe, ADN e parâmetros. Operações fiscais usam `preparar_*` e
`confirmar_*`; o token é de uso único, expira, fica vinculado a empresa/ambiente/
conteúdo e não sobrevive ao reinício. Produção exige `confirmar_producao=true`.
O servidor não expõe importação de certificado, senha, chave nem promoção de
produção; o kill switch da fachada continua valendo. Testes cobrem tools/list,
fluxo completo com replay, recompensa de token, expiração, gate de produção,
kill switch e stdout exclusivamente JSON.




Critério do marco: Fluxo completo com transporte simulado pelas APIs públicas, persistência e retomada; timeout não gera reenvio cego nem nota duplicada.

### Etapa 4. NFS-e utilizável e primeira homologação

Entregar DANFSe, CLI e MCP; iniciar a validação real de NFS-e por disparo manual protegido assim que fachada e CLI estiverem disponíveis.

- [x] [#15](https://github.com/seatrunk/carimbar/issues/15) — Gerar DANFSe em PDF a partir da NFS-e autorizada.
- [x] [#16](https://github.com/seatrunk/carimbar/issues/16) — Implementar CLI de emissão, consulta e administração de empresas.
- [x] [#17](https://github.com/seatrunk/carimbar/issues/17) — Implementar servidor MCP local com preparação e confirmação de operações.
- [ ] [#21](https://github.com/seatrunk/carimbar/issues/21) — Validar o MVP ponta a ponta e criar homologação manual protegida.

Critério do marco: NFS-e emitida e consultada em ambiente de teste, com PDF e interfaces utilizáveis. A issue de homologação permanece aberta até validar também NF-e e Python.

### Etapa 5. NF-e modelo 55 e DANFE

Acrescentar NF-e com IBS/CBS, autorização, consulta, eventos, inutilização e contingência SVC; gerar DANFE e ampliar os comandos CLI/MCP.

- [ ] [#18](https://github.com/seatrunk/carimbar/issues/18) — Implementar NF-e modelo 55 com IBS/CBS, eventos e contingência SVC.
- [ ] [#19](https://github.com/seatrunk/carimbar/issues/19) — Gerar DANFE em PDF a partir da NF-e autorizada.

Critério do marco: Fluxo NF-e integrado ao mesmo journal, cofre e controles de produção, com XML autorizado e DANFE; evidência de homologação registrada na issue transversal.

### Etapa 6. Python e conclusão do MVP

Concluir a API síncrona Python, validar wheels/sdist e executar a matriz final de homologação. Uma API Python inicial de NFS-e pode ser entregue após a etapa 4; a issue só fecha com ambos os modelos.

- [ ] [#20](https://github.com/seatrunk/carimbar/issues/20) — Expor Cofre e Emissor no Python com testes dos pacotes distribuídos.
- [ ] [#21](https://github.com/seatrunk/carimbar/issues/21) — Validar o MVP ponta a ponta e criar homologação manual protegida.

Critério do marco: Rust, CLI, MCP e Python exercitam os fluxos suportados; os critérios de todas as entregas do MVP têm evidência verificável.

## Dependências entre entregas

As setas abaixo são descritas como “depende de”. São dependências técnicas
mínimas; os critérios finais também podem exigir integração com interfaces
entregues em etapas posteriores.

- [#5](https://github.com/seatrunk/carimbar/issues/5) depende de nenhuma outra issue de implementação.
- [#6](https://github.com/seatrunk/carimbar/issues/6) depende de nenhuma outra issue de implementação.
- [#7](https://github.com/seatrunk/carimbar/issues/7) depende de [#5](https://github.com/seatrunk/carimbar/issues/5).
- [#8](https://github.com/seatrunk/carimbar/issues/8) depende de [#5](https://github.com/seatrunk/carimbar/issues/5).
- [#9](https://github.com/seatrunk/carimbar/issues/9) depende de [#7](https://github.com/seatrunk/carimbar/issues/7), [#8](https://github.com/seatrunk/carimbar/issues/8).
- [#10](https://github.com/seatrunk/carimbar/issues/10) depende de [#5](https://github.com/seatrunk/carimbar/issues/5), [#8](https://github.com/seatrunk/carimbar/issues/8).
- [#11](https://github.com/seatrunk/carimbar/issues/11) depende de [#7](https://github.com/seatrunk/carimbar/issues/7), [#9](https://github.com/seatrunk/carimbar/issues/9), [#10](https://github.com/seatrunk/carimbar/issues/10).
- [#12](https://github.com/seatrunk/carimbar/issues/12) depende de [#8](https://github.com/seatrunk/carimbar/issues/8).
- [#13](https://github.com/seatrunk/carimbar/issues/13) depende de [#11](https://github.com/seatrunk/carimbar/issues/11), [#12](https://github.com/seatrunk/carimbar/issues/12).
- [#14](https://github.com/seatrunk/carimbar/issues/14) depende de [#11](https://github.com/seatrunk/carimbar/issues/11), [#10](https://github.com/seatrunk/carimbar/issues/10).
- [#15](https://github.com/seatrunk/carimbar/issues/15) depende de [#11](https://github.com/seatrunk/carimbar/issues/11), [#7](https://github.com/seatrunk/carimbar/issues/7).
- [#16](https://github.com/seatrunk/carimbar/issues/16) depende de [#13](https://github.com/seatrunk/carimbar/issues/13), [#15](https://github.com/seatrunk/carimbar/issues/15).
- [#17](https://github.com/seatrunk/carimbar/issues/17) depende de [#13](https://github.com/seatrunk/carimbar/issues/13), [#15](https://github.com/seatrunk/carimbar/issues/15).
- [#18](https://github.com/seatrunk/carimbar/issues/18) depende de [#7](https://github.com/seatrunk/carimbar/issues/7), [#9](https://github.com/seatrunk/carimbar/issues/9), [#10](https://github.com/seatrunk/carimbar/issues/10), [#13](https://github.com/seatrunk/carimbar/issues/13).
- [#19](https://github.com/seatrunk/carimbar/issues/19) depende de [#18](https://github.com/seatrunk/carimbar/issues/18), [#15](https://github.com/seatrunk/carimbar/issues/15).
- [#20](https://github.com/seatrunk/carimbar/issues/20) depende de [#13](https://github.com/seatrunk/carimbar/issues/13), [#15](https://github.com/seatrunk/carimbar/issues/15), [#18](https://github.com/seatrunk/carimbar/issues/18), [#19](https://github.com/seatrunk/carimbar/issues/19).
- [#21](https://github.com/seatrunk/carimbar/issues/21) depende de [#13](https://github.com/seatrunk/carimbar/issues/13), [#16](https://github.com/seatrunk/carimbar/issues/16).

A homologação [#21](https://github.com/seatrunk/carimbar/issues/21) começa após [#13](https://github.com/seatrunk/carimbar/issues/13) e [#16](https://github.com/seatrunk/carimbar/issues/16),
mas seu encerramento depende também de [#14](https://github.com/seatrunk/carimbar/issues/14), [#15](https://github.com/seatrunk/carimbar/issues/15), [#17](https://github.com/seatrunk/carimbar/issues/17), [#18](https://github.com/seatrunk/carimbar/issues/18), [#19](https://github.com/seatrunk/carimbar/issues/19), [#20](https://github.com/seatrunk/carimbar/issues/20).
Essa distinção permite comprovar NFS-e cedo sem declarar o MVP completo.

## Regras de entrega

- Incorporar testes relevantes em cada implementação; não adiar validação para a última etapa.
- Usar apenas dados sintéticos no CI de PR e nunca versionar certificados, senhas ou dados reais.
- Conferir interoperabilidade de canonicalização/assinatura com vetores ou implementação independente.
- Manter valores fiscais em Decimal/Dinheiro; definir tipos apropriados para alíquotas e quantidades sem usar float.
- Reutilizar as regras da fachada em CLI, MCP e Python; não duplicar controles de produção.
- Exercitar falhas, concorrência, reinício e resultado desconhecido para prevenir emissão duplicada.
- Rodar os checks exigidos em CONTRIBUTING.md e registrar a evidência real no PR.
- Restringir chamadas governamentais a homologação/produção restrita por disparo manual protegido.
- Tratar publicação e habilitação de produção como ações explícitas; esta tarefa de planejamento não as executa.
- Fechar uma issue apenas com os critérios de aceite demonstrados; registrar limitações e pendências.

## Fora do MVP

NFC-e, servidor HTTP/MCP remoto, fila, webhooks e a camada self-hosted AGPL
permanecem para uma fase posterior. A interface opcional de KMS pode ser prevista
no cofre; provedores externos não bloqueiam a entrega local.

## Acompanhamento no T3 Code e no GitHub

O plano desta conversa do T3 Code contém os marcos e as referências das issues.
A issue central [#4](https://github.com/seatrunk/carimbar/issues/4) concentra o acompanhamento durável no GitHub.
Este arquivo permite consultar e versionar a sequência dentro do repositório.

O estado de execução deve ser conferido nas issues e nos PRs. Ao concluir um
marco, atualizar os checkboxes deste arquivo, a issue central e o plano da conversa.
A issue de homologação aparece em dois marcos porque sua entrega é incremental.
