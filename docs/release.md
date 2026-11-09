# Release

## Versão em desenvolvimento

O workspace está em `0.0.2` e tem **dois crates publicáveis**:

| Crate | Conteúdo | Resultado em 0.0.2 |
|---|---|---|
| `carimbar` | biblioteca com os módulos `types`, `cert`, `endpoints`, `xml`, `dsig`, `transport`, `vault`, `danfe`, `nfse`, `nfe` e a fachada `Emissor`/`Journal` | **publicado** por token: update de `0.0.1` para `0.0.2` |
| `carimbar-cli` | binários `carimbar` e `carimbar-mcp` | **publicado** por token: primeira versão `0.0.2` |

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
| `crates-io` | `verificar` | dry-run e plano sem token; em seguida trusted publishing (OIDC, token de 30 min). `carimbar` e `carimbar-cli` em 0.0.2 já são publish-update. Token de API só para um nome novo, à parte |

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
Perl com `IPC::Cmd` e `Time::Piece`. A imagem que o `maturin-action` usa depende
do alvo, e cada uma tem um gerenciador de pacotes diferente:

| Alvo | Imagem | Gerenciador |
|---|---|---|
| `linux-x86_64` (`manylinux: auto`) | manylinux (PyPA) | `dnf`/`yum` |
| `linux-aarch64` (`manylinux: auto`) | `ghcr.io/rust-cross/manylinux2014-cross:aarch64` (Ubuntu) | `apt-get` |
| `musllinux-x86_64` (`musllinux_1_2`) | `ghcr.io/rust-cross/rust-musl-cross:x86_64-musl` (Ubuntu) | `apt-get` |

O `before-script-linux` detecta o gerenciador e instala Perl antes do build:

```sh
if command -v apk >/dev/null 2>&1; then
  apk add --no-cache perl
elif command -v apt-get >/dev/null 2>&1; then
  apt-get update && apt-get install -y perl
elif command -v dnf >/dev/null 2>&1; then
  dnf install -y perl-IPC-Cmd perl-Time-Piece
elif command -v yum >/dev/null 2>&1; then
  yum install -y perl-IPC-Cmd perl-Time-Piece
else
  echo "before-script-linux: gerenciador de pacotes não reconhecido" >&2
  exit 1
fi
perl -MIPC::Cmd -MTime::Piece -e1
```

No musllinux **não** é Alpine: a imagem `rust-musl-cross` é Ubuntu e usa
`apt-get`. O ramo `apk` continua para imagens musl baseadas em Alpine. Sem essa
preparação o build falha com `Can't locate IPC/Cmd.pm` ou
`Can't locate Time/Piece.pm`. O job `wheel-cross` do `ci.yml` builda e importa
esses dois alvos cross em PRs com o mesmo bloco; mantenha os dois iguais.

## Crates publicados

Dois, em ordem topológica (`carimbar`, depois `carimbar-cli`). A ordem é
calculada pelo `scripts/publish-crates.sh` a partir do `cargo metadata`, que
publica um crate por vez e espera o índice entre um e outro.

### Estado observado em 2026-11-09

| Situação | Onde |
|---|---|
| `0.0.2` publicado por token em 09/11 19:14 BRT (run 42705514774, job `crates-io`) | `carimbar`, `carimbar-cli` |
| PyPI | `carimbar` ausente |

O run 42705514774 fechou em failure: os wheels `linux-aarch64` e
`musllinux-x86_64` falharam (`yum` ausente nas imagens rust-cross) e o job
`pypi` ficou skipped. Trusted publishing dos dois crates foi configurado em
09/11 19:20 BRT (owner `seatrunk`, repo `carimbar`, workflow `release.yml`,
environment `crates-io`). Daqui em diante os dois são publish-update e o OIDC
cobre os dois.

Para revalidar antes de agir, rode `./scripts/release-status.sh`. O job
`verificar` executa o mesmo script e anexa a tabela ao resumo do run.

## Publicação 0.0.2

`carimbar` 0.0.2 e `carimbar-cli` 0.0.2 já estão no crates.io, publicados por
token no run 42705514774. O PyPI não publicou. Conferir
https://crates.io/crates/carimbar, https://crates.io/crates/carimbar-cli e
https://pypi.org/p/carimbar.

Não reuse `v0.0.1`: `carimbar` já tem essa versão e o número é imutável. Uma
versão nova exige ok do Ricardo. A primeira publicação via OIDC é uma tag nova,
depois do merge deste job.

### Trusted publishing e crate novo

O job `crates-io` autentica por OIDC. Trusted publishing de `carimbar` e
`carimbar-cli` foi configurado em 09/11 19:20 BRT (owner `seatrunk`, repo
`carimbar`, workflow `release.yml`, environment `crates-io`). O job não lê
`secrets.CARGO_REGISTRY_TOKEN`.

Trusted publishing não cria crate novo. A primeira versão de um nome que ainda
não existe no crates.io precisa de um token de API com **publish-new**, por
configuração à parte deste job. `carimbar` 0.0.2 e `carimbar-cli` 0.0.2 já
existem: daqui em diante os dois são **publish-update** e o OIDC cobre os dois.

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
workflow from". Disparar de `main` é recusado pelos environments `crates-io` e
`pypi`: os dois só aceitam tags `v*`. Não dá para recuperar o PyPI disparando
da `main`. O re-disparo da `v0.0.2` executa o workflow gravado na tag (ainda lê
o secret) e o script pula `carimbar` e `carimbar-cli`, já presentes no índice.

### crates.io parou no meio

`scripts/publish-crates.sh` é idempotente e faz um plano antes de publicar:

- consulta o índice e **pula** o que já está publicado na versão atual;
- distingue **update** (crate já existe, precisa de publish-update) de **novo**
  (precisa de publish-new) e imprime a tabela no resumo do run;
- em `429` ("too many"), espera o horário indicado pelo crates.io (+30 s) e tenta
  de novo; se a espera passar de `MAX_ESPERA_S` (1800 s), para com mensagem clara
  em vez de dormir sem limite;
- em `401/403` (token OIDC expirado, trusted publishing ausente ou fora do
  environment `crates-io`, ou token à parte sem publish-new num crate novo),
  **para na hora**: repetir não resolve enquanto a causa continuar;
- em `5xx`/erro de rede, faz backoff exponencial (até 600 s);
- em "dependência ainda não visível no índice", espera 30 s e tenta de novo.

O job roda um `--dry-run` do plano antes de publicar. Num re-disparo da
`v0.0.2` o resumo marca `carimbar` e `carimbar-cli` como `pula`. Numa versão
seguinte os dois são update.

1. Corrija a causa, se houver.
2. Rode o workflow de novo **com a ref da tag** e `tag` = a tag existente.
3. Aprove o environment `crates-io`.

### PyPI parou no meio

O `pypa/gh-action-pypi-publish` usa `skip-existing: true`: arquivos já enviados
são ignorados e os restantes são publicados. Rode o workflow de novo com a ref
da tag, não a partir de `main`: o environment `pypi` só aceita tags `v*`, e um
`workflow_dispatch` de `main` tem o deploy recusado. Não é preciso apagar nem
renomear artefatos.

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

## Troca para OIDC

1. **Feito** em 09/11 19:20 BRT. Trusted publishing configurado em `carimbar` e
   `carimbar-cli`: owner `seatrunk`, repo `carimbar`, workflow `release.yml`,
   environment `crates-io`.
2. Mergear o job `crates-io` que obtém o token de 30 min com
   `rust-lang/crates-io-auth-action` v1.0.6, pinada por SHA
   `3268eb413133e2960fa4ae0e963ca76380c54cd9`, depois do dry-run e do plano.
3. Publicar uma vez via OIDC, numa tag nova cortada depois do merge. Versão
   nova exige ok do Ricardo. Os dois crates são publish-update e o OIDC cobre
   os dois.
4. Só então revogar o token de API e apagar o secret do environment. O token
   só sai depois dessa publicação ter dado certo.

O run 42705514774 já terminou, em failure: os wheels `linux-aarch64` e
`musllinux-x86_64` falharam (`yum` ausente nas imagens rust-cross) e o job
`pypi` ficou skipped. O PyPI `carimbar` segue ausente. Um re-disparo da tag
`v0.0.2` executa o workflow gravado nela, que ainda lê o secret, e o script
pula os dois crates.

O passo de auth fica imediatamente antes de `publish-crates.sh`. O dry-run
(`cargo publish --workspace --locked --dry-run`) e o plano (`--dry-run` do
script) não usam token. A aprovação do environment ocorre antes dos passos, então
a espera do reviewer não consome os 30 min.

`publish-crates.sh` pode dormir até `MAX_ESPERA_S` (1800 s) num 429. Essa espera
passa da validade do token: a tentativa seguinte recebe 401/403 e o script para.
Não há renovação no meio do script. Encurtar `MAX_ESPERA_S` para uma folga
abaixo de 30 min, ou renovar o token, fica para um PR à parte; a lógica do
script não muda aqui.

As `permissions` do job substituem as do workflow: `contents: read` (sem ela o
checkout falha) e `id-token: write`. Nenhum outro job ganha `id-token` nesta
mudança. O job `pypi` já publicava por OIDC.

O `workflow_dispatch` executa o YAML da ref escolhida. A `v0.0.2` usa o
workflow da tag (secret) e pula os dois crates. Tag posterior ao merge usa
OIDC e não tem fallback para o secret. Disparar a partir de `main` é recusado
pelos environments `crates-io` e `pypi`: os dois só aceitam tags `v*`. Não dá
para recuperar o PyPI disparando da `main`.

## PyPI

Trusted publisher já cadastrado: owner `seatrunk`, repo `carimbar`, workflow
`release.yml`, environment `pypi`. Não há token.

## Evidência da release

Para fechar a #6, registre no comentário da issue:

- URL do run e SHA do commit da tag;
- saída de `scripts/release-status.sh` mostrando `carimbar` e `carimbar-cli` na
  versão planejada, e o pacote Python no PyPI;
- a tabela do plano anexada pelo `--dry-run` do `publish-crates.sh` (na
  `v0.0.2` os dois crates são `pula`; numa versão nova os dois são update);
- lista dos wheels e do sdist gerados (nomes dos artefatos do run);
- resultado de `twine check`, dos smokes de instalação e do `pytest`;
- confirmação de que os environments exigiram aprovação.

O run do GitHub é a evidência primária. Não anexe certificados, senhas, `.env` ou
dados reais de empresas.
