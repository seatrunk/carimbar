# Changelog

## 0.0.3

- Primeira versão no PyPI (pacote `carimbar`: wheels abi3 para Python ≥ 3.9 e sdist).
- O crates.io publica `carimbar` e `carimbar-cli` por trusted publishing (OIDC, #44).
- Wheels linux-aarch64 e musllinux-x86_64 corrigidos (#45).

### Plataformas suportadas (wheels do PyPI)

Wheels **abi3** para Python ≥ 3.9, na matriz do job `wheels` de `.github/workflows/release.yml`:

- Linux x86_64 glibc (`manylinux: auto`). A tag real do wheel, confirmada no job `wheel` do CI (run [42718989553](https://github.com/seatrunk/carimbar/actions/runs/42718989553), imagem `quay.io/pypa/manylinux2014_x86_64`), é `manylinux2014_x86_64.manylinux_2_17_x86_64` (`manylinux_2_17` / manylinux2014).
- Linux aarch64 glibc `manylinux_2_28`.
- Linux x86_64 musl `musllinux_1_2`.
- macOS x86_64 (Intel).
- macOS arm64 (Apple Silicon).
- Windows x64.

Não há wheel musllinux aarch64 nem Windows arm64. Nessas plataformas não há wheel e a instalação cai no sdist.

O wheel linux-aarch64 é `manylinux_2_28` (glibc ≥ 2.28) e não instala no Amazon Linux 2 em Graviton (glibc 2.26) nem no Ubuntu 18.04 (glibc 2.27). Motivo: o `ring` 0.17 não compila com o gcc 4.8.5 da imagem manylinux2014.

Alternativas: instalar pela fonte (sdist, `pip install --no-binary carimbar carimbar` ou o fallback automático do pip; exige toolchain Rust ≥ 1.89 e Perl para o OpenSSL vendored) ou usar Amazon Linux 2023.

## 0.0.2

- `carimbar` e `carimbar-cli` publicados no crates.io. O pacote Python ainda não existia no PyPI.
