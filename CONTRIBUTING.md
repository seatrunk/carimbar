# Contribuindo

## Regras duras

- Nunca commite certificado (`.pfx`, `.p12`, `.pem`, `.key`...), senha, `.env` ou dado real de empresa.
  O CI roda `scripts/check-no-certs.sh` e falha se encontrar algum.
- Em testes, use só CNPJs e chaves sintéticos (base arbitrária + DV calculado). Nunca CNPJ de empresa real.
- Homologação e produção restrita rodam fora de PR, em job manual com environment protegido.
- Valores monetários usam `Dinheiro`/`Decimal`. `f64` em valor fiscal é bug.

## Antes do PR

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check
(cd crates/carimbar-py && maturin build)
./scripts/check-no-certs.sh
./scripts/test-check-no-certs.sh
```

Toolchain fixado em `rust-toolchain.toml`; MSRV declarado em `Cargo.toml` (`rust-version`).
`unsafe_code` é `forbid` em todos os crates, exceto `carimbar-py` (macros do PyO3).

## Schemas

XSD oficiais ficam em `schemas/<pacote>/`, sem edição, com `SHA256SUMS` e origem em
`schemas/README.md`. Atualizar = pasta nova, nunca sobrescrever.

## Commits e PR

Commits pequenos, mensagem em português no imperativo/descritivo. Preencha o template de PR
com a saída real dos comandos (PROVA) e o run do CI.
