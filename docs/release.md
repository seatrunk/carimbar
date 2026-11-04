# Release

`.github/workflows/release.yml` roda só em push de tag `v*` ou manualmente
(`workflow_dispatch`). Nada roda em branch nem em PR. Hoje nada está publicado.

## O que configurar antes do primeiro release

1. **PyPI, trusted publisher (OIDC).** Em https://pypi.org/manage/account/publishing/
   (ou no projeto, se o nome já tiver sido reservado), adicionar um publisher GitHub:
   - Owner: `seatrunk`
   - Repository: `carimbar`
   - Workflow: `release.yml`
   - Environment: `pypi`
   Não há token: o job `pypi` pede um id-token ao GitHub (`id-token: write`, só nesse job).
2. **Environments no GitHub** (Settings → Environments):
   - `pypi`: revisor obrigatório (Ricardo) e, de preferência, restrição a tags `v*`.
   - `crates-io`: idem, quando o job for ativado.
3. **crates.io (desativado, `if: false`).** Criar a conta, decidir entre token
   (`CARGO_REGISTRY_TOKEN` como secret do environment `crates-io`) e trusted publishing,
   trocar `publish = false` no `Cargo.toml` e remover o `if: false`.

## Publicar

1. Atualizar `version` em `[workspace.package]` e criar a tag `vX.Y.Z` no commit da `main`.
2. Os jobs `wheels` (abi3: Linux x86_64/aarch64, musllinux, macOS universal2, Windows x64)
   e `sdist` rodam; o `pypi` espera a aprovação no environment e publica.
