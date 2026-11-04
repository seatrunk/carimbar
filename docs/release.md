# Release

`.github/workflows/release.yml` roda só em push de tag `v*` ou manualmente
(`workflow_dispatch`). Nada roda em branch nem em PR. Publicar é irreversível:
crates.io e PyPI não apagam versões (no máximo yank).

## Jobs

| Job | Depende de | O que faz |
|---|---|---|
| `verificar` | — | falha se a ref não for a tag `v<versão do workspace>`; fmt, clippy `-D warnings`, testes, check de certificados |
| `wheels`, `sdist` | `verificar` | maturin (abi3): Linux x86_64/aarch64, musllinux, macOS universal2, Windows x64 + sdist |
| `pypi` | `wheels`, `sdist` | trusted publishing (OIDC), environment `pypi` |
| `crates-io` | `verificar` | `cargo publish --workspace --locked` com `CARGO_REGISTRY_TOKEN`, environment `crates-io` |

Os environments `pypi` e `crates-io` exigem aprovação do seatrunk e aceitam só tags `v*`.

## Crates publicados

Todos os do workspace, exceto `carimbar-py` (`publish = false`, vai pelo PyPI como
`carimbar`). Ordem topológica calculada pelo `cargo publish --workspace` (Cargo ≥ 1.90),
que espera o índice entre um crate e outro. Pacotes sem XSD nem testes de integração.

## Primeira publicação (0.0.1)

1. Conferir que a `main` está verde no CI e que `version` em `[workspace.package]`
   é a desejada (hoje `0.0.1`). Versão de `pyproject` sai da mesma fonte.
2. Criar e enviar a tag no commit da `main`:
   `git tag -a v0.0.1 -m "carimbar 0.0.1" && git push origin v0.0.1`.
3. Em Actions → Release, aprovar os deploys `crates-io` e `pypi` quando pedidos
   (só depois de `verificar` passar).
4. Conferir https://crates.io/crates/carimbar e https://pypi.org/p/carimbar.

Se o `crates-io` falhar no meio, os crates já enviados ficam publicados. Corrigir a
causa e publicar o resto à mão, na ordem, com `cargo publish -p <crate> --locked`
(o token publish-new ainda vale para nomes novos), ou subir uma versão nova.

## Token publish-new

O `CARGO_REGISTRY_TOKEN` atual tem escopo **publish-new** para `carimbar*` e validade
curta: serve só para criar os crates. Versões seguintes precisam de trusted
publishing (abaixo) ou de outro token com `publish-update`.

## Depois da primeira publicação

1. Em cada crate no crates.io (Settings → Trusted Publishing), adicionar o publisher
   GitHub: repo `seatrunk/carimbar`, workflow `release.yml`, environment `crates-io`.
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

## PyPI

Trusted publisher já cadastrado: owner `seatrunk`, repo `carimbar`, workflow
`release.yml`, environment `pypi`. Não há token.

## Recuperação de release parada no meio

Se o `crates-io` parar (ex.: 429 "too many new crates"), não crie tag nova.
`scripts/publish-crates.sh` é idempotente: pula o que já está no índice e, no 429,
espera o horário que o crates.io indica. Rode o Release de novo com
`workflow_dispatch` a partir de `main` e `tag` = a tag existente (ex.: `v0.0.1`).
Os environments `crates-io` e `pypi` só aceitam tags `v*`; para esse disparo o
dono precisa liberar `main` temporariamente nas regras de deployment e remover
depois. A aprovação manual continua valendo.

Para ver o que falta sem publicar: `scripts/publish-crates.sh --dry-run`
(com a tag em checkout).
