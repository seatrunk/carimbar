# Release

## Versão em desenvolvimento

O workspace está em `0.0.2`, com as dependências internas fixadas na mesma
versão. Os crates `carimbar`, `carimbar-cert`, `carimbar-danfe`,
`carimbar-endpoints` e `carimbar-types` já têm `0.0.1` publicada e imutável;
`0.0.2` é a versão planejada para todos os 13 crates publicáveis. Os outros oito
(`carimbar-cli`, `carimbar-dsig`, `carimbar-mcp`, `carimbar-nfe`,
`carimbar-nfse`, `carimbar-transport`, `carimbar-vault`, `carimbar-xml`) nunca
foram publicados. O pacote Python `carimbar` ainda não existe no PyPI.

Este ajuste não publica pacotes nem cria tags. Publicar é irreversível: crates.io
e PyPI não apagam versões (no máximo yank).

`.github/workflows/release.yml` roda só em push de tag `v*` ou manualmente
(`workflow_dispatch`). Nada roda em branch nem em PR.

### Mudança de comportamento (breaking): kill switch de produção

Desde a #41, `KillSwitchAmbiente` só libera produção com `CARIMBAR_PRODUCAO=1`
(depois de `trim()`). Antes a ausência da variável liberava produção e apenas
`0`/`false`/`nao` bloqueavam. Agora ausência, vazio e qualquer valor diferente
de `1` bloqueiam produção (fail-closed).

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
| `crates-io` | `verificar` | `cargo publish --workspace --locked --dry-run` e depois `scripts/publish-crates.sh` com `CARGO_REGISTRY_TOKEN`, environment `crates-io` |

Os environments `pypi` e `crates-io` exigem aprovação do seatrunk e aceitam só tags `v*`.

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

Todos os do workspace, exceto `carimbar-py` (`publish = false`, vai pelo PyPI como
`carimbar`). A ordem topológica é calculada pelo `scripts/publish-crates.sh`, que
publica um crate por vez e espera o índice entre um e outro.

### Estado observado em 2026-11-09

| Situação | Crates |
|---|---|
| `0.0.1` publicado, `0.0.2` ausente | `carimbar`, `carimbar-cert`, `carimbar-danfe`, `carimbar-endpoints`, `carimbar-types` |
| nunca publicado | `carimbar-cli`, `carimbar-dsig`, `carimbar-mcp`, `carimbar-nfe`, `carimbar-nfse`, `carimbar-transport`, `carimbar-vault`, `carimbar-xml` |
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
4. Conferir https://crates.io/crates/carimbar e https://pypi.org/p/carimbar.

Não reuse `v0.0.1`: cinco crates já têm essa versão e o número é imutável.

## Recuperação de release parcial

Nunca crie uma tag nova para recuperar. O workflow aceita `workflow_dispatch` com
o nome da tag existente e republica só o que falta, a partir do commit do
workflow (os scripts vêm de `main`, não da tag).

### crates.io parou no meio

`scripts/publish-crates.sh` é idempotente: consulta o índice, pula o que já está
publicado e, em `429` ("too many new crates"), espera o horário indicado pelo
crates.io (+30 s) e tenta de novo. Um crate enviado antes da falha permanece
publicado; o próximo run continua do primeiro ausente.

1. Corrija a causa, se houver.
2. Rode Actions → Release → Run workflow com `tag` = a tag existente.
3. Aprove o environment `crates-io`.

### PyPI parou no meio

O `pypa/gh-action-pypi-publish` usa `skip-existing: true`: arquivos já enviados
são ignorados e os restantes são publicados. Rode o workflow de novo com a mesma
tag. Não é preciso apagar nem renomear artefatos.

### Wheels ou smokes falharam

Os artefatos ficam no run. Corrija o workflow em `main`, faça merge, e rode de
novo o `workflow_dispatch` com a tag existente. O job `verificar` continua
exigindo que a tag aponte para a versão do workspace; o código publicado é o da
tag, mas os scripts e a matriz de build vêm de `main`.

### Regras de environment

Os environments `crates-io` e `pypi` só aceitam tags `v*`. Para um
`workflow_dispatch` a partir de `main`, o dono precisa liberar `main`
temporariamente nas regras de deployment e remover depois. A aprovação manual do
environment continua valendo; não desligue a proteção de forma permanente.

### Ver o que falta sem publicar

Com a tag em checkout:

```sh
./scripts/release-status.sh
scripts/publish-crates.sh --dry-run
```

## Token publish-new → OIDC

O `CARGO_REGISTRY_TOKEN` atual tem escopo **publish-new** para `carimbar*` e
validade curta: serve para criar os crates que ainda não existem. Depois da
primeira publicação:

1. Em cada crate no crates.io (Settings → Trusted Publishing), adicionar o
   publisher GitHub: repo `seatrunk/carimbar`, workflow `release.yml`,
   environment `crates-io`.
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
3. Revogar o token publish-new no crates.io e apagar o secret do environment.

Enquanto o token publish-new existir, ele não serve para `publish-update` de
versões futuras; a transição para OIDC é obrigatória antes de `0.0.3`.

## PyPI

Trusted publisher já cadastrado: owner `seatrunk`, repo `carimbar`, workflow
`release.yml`, environment `pypi`. Não há token.

## Evidência da release

Para fechar a #6, registre no comentário da issue:

- URL do run e SHA do commit da tag;
- saída de `scripts/release-status.sh` mostrando os 13 crates na versão planejada
  e o pacote Python no PyPI;
- lista dos wheels e do sdist gerados (nomes dos artefatos do run);
- resultado de `twine check`, dos smokes de instalação e do `pytest`;
- confirmação de que os environments exigiram aprovação.

O run do GitHub é a evidência primária. Não anexe certificados, senhas, `.env` ou
dados reais de empresas.
