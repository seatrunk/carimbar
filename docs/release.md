# Release

## Versão em desenvolvimento

O workspace está em `0.0.2` e tem **dois crates publicáveis**:

| Crate | Conteúdo | Ação em 0.0.2 |
|---|---|---|
| `carimbar` | biblioteca com os módulos `types`, `cert`, `endpoints`, `xml`, `dsig`, `transport`, `vault`, `danfe`, `nfse`, `nfe` e a fachada `Emissor`/`Journal` | **update**: `0.0.1` já existe |
| `carimbar-cli` | binários `carimbar` e `carimbar-mcp` | **novo**: nunca publicado |

`carimbar-py` tem `publish = false`: não vai ao crates.io, e sim ao PyPI
(maturin) como pacote `carimbar`. O pacote Python `carimbar` ainda não existe no
PyPI.

Este ajuste não publica pacotes nem cria tags. Publicar é irreversível: crates.io
e PyPI não apagam versões (no máximo yank).

### Consolidação de crates (da 0.0.1 para a 0.0.2)

Até a `0.0.1`, o crates.io tinha cinco crates (`carimbar`, `carimbar-types`,
`carimbar-cert`, `carimbar-endpoints`, `carimbar-danfe`). Na `0.0.2` os quatro
crates de apoio viraram **módulos de `carimbar` com features**; o `0.0.1` deles
continua publicado, mas não recebe `0.0.2`. O único nome novo no crates.io é
`carimbar-cli`. Isso evita criar oito nomes que ficariam reservados para sempre.

### Mudança de comportamento (breaking): kill switch de produção

Desde a #41, `KillSwitchAmbiente` só libera produção com `CARIMBAR_PRODUCAO=1`
(depois de `trim()`). Antes, a ausência da variável liberava produção e apenas
`0`/`false`/`nao` bloqueavam. Agora ausência, vazio e qualquer valor diferente de
`1` bloqueiam produção (fail-closed). Isso precisa entrar nas notas da `0.0.2`.

## Grafo de features

`carimbar` expõe os módulos `types`, `cert`, `endpoints`, `xml`, `dsig`,
`transport`, `vault`, `danfe`, `nfse` e `nfe`. O módulo `types` está sempre
ligado; os demais seguem a ordem topológica:

```
types (sempre)
  ├─ cert
  ├─ endpoints
  └─ xml
       ├─ danfe
       ├─ dsig   (cert + xml)
       ├─ transport (cert)
       ├─ vault  (cert)
       └─ nfse   (dsig + endpoints + transport + xml)
            └─ nfe (nfse + dsig + endpoints + transport + xml)
```

`fachada` liga todos os módulos e expõe `Emissor`, `Journal` e `ErroEmissor`;
`default = ["fachada"]`. Continuam opt-in: `xsd` (validação XSD), `nfe-sha1`
(RSA-SHA1 da NF-e via OpenSSL) e `nfe-sha1-vendored` (OpenSSL embutido). O grafo
padrão não inclui OpenSSL.

## Jobs

| Job | Depende de | O que faz |
|---|---|---|
| `verificar` | — | falha se a ref não for a tag `v<versão do workspace>`; fmt, clippy `-D warnings`, testes, check de certificados e tabela de estado dos registros no resumo do run |
| `wheels` | `verificar` | maturin (abi3) em 6 alvos: Linux glibc x86_64/aarch64, musllinux x86_64, macOS Intel, macOS Apple Silicon e Windows x64 |
| `wheel-smoke` | `wheels` | instala o wheel na plataforma correspondente e roda o teste básico de importação/validação |
| `wheel-smoke-linux` | `wheels` | instala e importa os wheels musllinux (Alpine) e aarch64 (QEMU) |
| `wheel-pytest` | `wheels` | roda a suíte `pytest` completa sobre o wheel manylinux x86_64 |
| `sdist` | `verificar` | gera o sdist do pacote Python |
| `sdist-smoke` | `sdist` | instala o sdist (compila a partir da fonte) e roda o teste básico |
| `dist-check` | `wheels`, `sdist` | `twine check` em todos os artefatos antes de publicar |
| `pypi` | `dist-check`, `wheel-smoke`, `wheel-smoke-linux`, `wheel-pytest`, `sdist-smoke` | trusted publishing (OIDC), environment `pypi`, com `skip-existing: true` |
| `crates-io` | `verificar` | `cargo publish --workspace --locked --dry-run`, depois `scripts/publish-crates.sh --dry-run` (plano) e `scripts/publish-crates.sh` com `CARGO_REGISTRY_TOKEN`, environment `crates-io` |

O workflow tem `concurrency: release` (sem cancelar o que está em andamento):
dois runs não publicam ao mesmo tempo.

### Por que wheels separados de macOS

O build `universal2-apple-darwin` era frágil e a tentativa na branch
`release/macos-intel` perdeu comunicação com o runner. O release agora gera dois
wheels nativos:

- `macos-15-intel` → `x86_64-apple-darwin`;
- `macos-15` → `aarch64-apple-darwin`.

O runner `macos-14` foi aposentado pelo GitHub em 2026-11-02 e não é mais usado.

### OpenSSL vendored nos wheels Linux

O wheel habilita `nfe-sha1` com OpenSSL vendored, e o `Configure` do OpenSSL usa
Perl. Nos containers manylinux faltam `IPC::Cmd` e `Time::Piece`, instalados antes
do build:

```sh
dnf install -y perl-IPC-Cmd perl-Time-Piece   # ou yum
```

No musllinux (Alpine) os dois módulos já vêm no pacote `perl`; o workflow instala
`perl` com `apk` para garantir. Sem essa preparação o build falha com
`Can't locate IPC/Cmd.pm` ou `Can't locate Time/Piece.pm`.

## Crates publicados

Dois, em ordem topológica (`carimbar`, depois `carimbar-cli`). A ordem é
calculada pelo `scripts/publish-crates.sh` a partir do `cargo metadata`, que
publica um crate por vez e espera o índice entre um e outro.

### Estado observado em 2026-11-09

| Situação | Crates |
|---|---|
| `0.0.1` publicado, `0.0.2` ausente | `carimbar` |
| nunca publicado | `carimbar-cli` |
| PyPI | `carimbar` ausente |

Para revalidar antes de agir, rode `./scripts/release-status.sh`. O job
`verificar` executa o mesmo script e anexa a tabela ao resumo do run.

## Primeira publicação (0.0.2)

1. Conferir que a `main` está verde no CI e que `version` em `[workspace.package]`
   é a desejada (`0.0.2`). A versão do `pyproject` sai da mesma fonte.
2. Criar e enviar a tag no commit da `main`:
   `git tag -a v0.0.2 -m "carimbar 0.0.2" && git push origin v0.0.2`.
3. Em Actions → Release, aprovar os deploys `crates-io` e `pypi` quando pedidos
   (só depois de `verificar`, `dist-check` e dos smokes passarem).
4. Conferir https://crates.io/crates/carimbar,
   https://crates.io/crates/carimbar-cli e https://pypi.org/p/carimbar.

Não reuse `v0.0.1`: `carimbar` já tem essa versão e o número é imutável.

### Escopo do token (`publish-update` x `publish-new`)

A `0.0.2` mistura os dois casos:

- `carimbar` já existe em `0.0.1` → exige **publish-update**;
- `carimbar-cli` é novo → exige **publish-new**.

Um token só com publish-new falha com **403** já no primeiro crate da ordem
(`carimbar`). Use um token novo com **publish-new + publish-update**, padrão
`carimbar*`, validade de poucos dias, guardado no environment `crates-io`. A
alternativa é cadastrar trusted publishing em `carimbar` antes (o que resolve o
update), mantendo o token só para o `carimbar-cli` novo. Trusted publishing não
cria crate novo.

## Recuperação de release parcial

O re-disparo com `workflow_dispatch` na ref da tag existente serve para **falha
transitória** (429, erro de rede, artefato parcial): ele republica só o que falta
e executa o **workflow e os scripts gravados na própria tag**. Defeito no
workflow ou nos scripts exige uma tag nova, que é decisão do Ricardo.

**Atenção:** os environments `crates-io` e `pypi` só aceitam **tags `v*`**. A
branch `main` foi removida das regras. Por isso o re-disparo manual tem de ser
feito **com a ref da tag**, não a partir de `main`:

```sh
gh workflow run release.yml --ref v0.0.2 -f tag=v0.0.2
```

No site, em Actions → Release → Run workflow, escolha a tag `v0.0.2` em "Use
workflow from". Disparar de `main` é recusado pelo environment `crates-io` antes
de qualquer publicação.

### crates.io parou no meio

`scripts/publish-crates.sh` é idempotente e faz um plano antes de publicar:

- consulta o índice e **pula** o que já está publicado na versão atual;
- distingue **update** (crate já existe, precisa de publish-update) de **novo**
  (precisa de publish-new) e imprime a tabela no resumo do run;
- em `429` ("too many"), espera o horário indicado pelo crates.io (+30 s) e tenta
  de novo; se a espera passar de `MAX_ESPERA_S` (1800 s), para com mensagem clara
  em vez de dormir sem limite;
- em `401/403` (token sem publish-new/publish-update, expirado ou fora do padrão
  `carimbar*`), **para na hora**: repetir não resolve;
- em `5xx`/erro de rede, faz backoff exponencial (até 600 s);
- em "dependência ainda não visível no índice", espera 30 s e tenta de novo.

O job roda um `--dry-run` do plano antes de publicar, então o resumo do run
mostra exatamente `carimbar` (update) e `carimbar-cli` (novo).

1. Corrija a causa, se houver.
2. Rode o workflow de novo **com a ref da tag** e `tag` = a tag existente.
3. Aprove o environment `crates-io`.

### PyPI parou no meio

O `pypa/gh-action-pypi-publish` usa `skip-existing: true`: arquivos já enviados
são ignorados e os restantes são publicados. Rode o workflow de novo com a mesma
tag. Não é preciso apagar nem renomear artefatos.

### Wheels ou smokes falharam

Os artefatos ficam no run. Para falha transitória, rode o `workflow_dispatch` de
novo **com a ref da tag** existente. Como `--ref vX` executa o commit da tag, o
workflow e os scripts são os da tag: uma correção que só exista em `main` não é
usada. Se o defeito estiver no workflow ou nos scripts, é preciso uma tag nova
(decisão do Ricardo); não reutilize a `v0.0.2` forçando a ref. O job `verificar`
continua exigindo que a tag aponte para a versão do workspace.

### Regras de environment

Os environments `crates-io` e `pypi` exigem aprovação do seatrunk e aceitam só
tags `v*`. A branch `main` foi removida das regras de deployment (antes era
liberada para o `workflow_dispatch` de recuperação). A consequência é que todo
re-disparo manual precisa da ref da tag (`--ref vX`). A aprovação manual do
environment continua valendo; não desligue a proteção de forma permanente.

### Ver o que falta sem publicar

Com a tag em checkout:

```sh
./scripts/release-status.sh
scripts/publish-crates.sh --dry-run
```

## Token → OIDC (depois da 0.0.2)

O `CARGO_REGISTRY_TOKEN` desta publicação tem escopo **publish-new +
publish-update** para `carimbar*` e validade curta. Depois da primeira
publicação:

1. Em cada crate no crates.io (`carimbar` e `carimbar-cli`, Settings → Trusted
   Publishing), adicionar o publisher GitHub: repo `seatrunk/carimbar`, workflow
   `release.yml`, environment `crates-io`.
2. Trocar o job `crates-io` para OIDC:
   ```yaml
   permissions:
     id-token: write
   steps:
     - uses: actions/checkout@v5
     - uses: rust-lang/crates-io-auth-action@v1
       id: auth
     - run: cargo publish --workspace --locked
       env:
         CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}
   ```
3. Revogar o token publish-new/publish-update no crates.io e apagar o secret do
   environment.

A transição para OIDC é obrigatória antes da `0.0.3`, porque o token de escopo
curto não deve continuar guardado.

## PyPI

Trusted publisher já cadastrado: owner `seatrunk`, repo `carimbar`, workflow
`release.yml`, environment `pypi`. Não há token.

## Evidência da release

Para fechar a #6, registre no comentário da issue:

- URL do run e SHA do commit da tag;
- saída de `scripts/release-status.sh` mostrando `carimbar` e `carimbar-cli` na
  versão planejada, e o pacote Python no PyPI;
- a tabela do plano (`carimbar` = update, `carimbar-cli` = novo) anexada pelo
  `--dry-run` do `publish-crates.sh`;
- lista dos wheels e do sdist gerados (nomes dos artefatos do run);
- resultado de `twine check`, dos smokes de instalação e do `pytest`;
- confirmação de que os environments exigiram aprovação.

O run do GitHub é a evidência primária. Não anexe certificados, senhas, `.env` ou
dados reais de empresas.
