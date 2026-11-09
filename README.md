# carimbar

**Emissão direta de NF-e e NFS-e Nacional para devs e agentes de IA, sem provedor intermediário.**

[![Licença: Apache-2.0](https://img.shields.io/badge/licen%C3%A7a-Apache--2.0-blue.svg)](LICENSE)
[![M8ven Score](https://m8ven.ai/badge/mcp/seatrunk-carimbar-14so9i)](https://m8ven.ai/mcp/seatrunk-carimbar-14so9i?s=readme)
![Rust](https://img.shields.io/badge/n%C3%BAcleo-Rust-b7410e)
![Python](https://img.shields.io/badge/binding-Python-3776ab)
![MCP](https://img.shields.io/badge/servidor-MCP-6f42c1)

> O nome vem do verbo: *carimbar* é o gesto que torna um documento oficial.

O núcleo é escrito em Rust e conversa direto com a SEFAZ e com o ADN (Ambiente de Dados Nacional). Você pode usá-lo como **CLI**, como **servidor MCP local** (para agentes de IA) ou como **biblioteca Python**.

> [!WARNING]
> **O MVP ainda não passou por homologação real** com certificado e empresa autorizados. Ele está implementado e coberto por testes locais, mas **não use esta versão para emitir notas fiscais em produção.**

## Sumário

- [Por que carimbar](#por-que-carimbar)
- [Funcionalidades](#funcionalidades)
- [Instalação](#instalação)
  - [Plataformas suportadas (wheels do PyPI)](#plataformas-suportadas-wheels-do-pypi)
- [Uso rápido](#uso-rápido)
  - [Python](#python)
  - [CLI](#cli)
  - [MCP](#mcp)
- [Segurança e produção](#segurança-e-produção)
- [Estado do projeto](#estado-do-projeto)
- [Roadmap](#roadmap)
- [Documentação](#documentação)
- [Contribuindo](#contribuindo)
- [Licenças](#licenças)

## Por que carimbar

Desde 01/11/2026, ME e EPP optantes pelo Simples Nacional são obrigadas a emitir NFS-e pelo Emissor Nacional (Resolução CGSN 191/2026). Isso empurra muita gente para integrações novas, e a maioria das opções disponíveis passa por um provedor intermediário.

O carimbar existe para quem quer emitir direto, com:

- **Sem intermediário:** a emissão vai direto para a SEFAZ e para o ADN.
- **Feito para automação:** CLI com códigos de saída estáveis, binding Python e servidor MCP para agentes.
- **Seguro por padrão:** produção bloqueada até promoção explícita, certificados criptografados em repouso e journal contra emissão duplicada.
- **Valores fiscais sem `float`:** `Dinheiro` entra e sai como texto.

## Funcionalidades

**NFS-e Nacional**
- DPS, emissão, consulta e cancelamento.
- Distribuição ADN por NSU.
- Parâmetros municipais.

**NF-e modelo 55**
- Leiaute 4.00 com campos de IBS e CBS (NT 2025.002-RTC, PL_010).
- Autorização, consulta, eventos, inutilização e contingência SVC.
- NFC-e fica para uma fase futura.

**Infraestrutura**
- Cofre multiempresa de certificados A1, com envelope encryption, rotação de chaves e auditoria encadeada.
- Fachada `Emissor` com journal local, idempotência, reconciliação e bloqueio explícito de produção.
- DANFSe e DANFE em PDF gerados a partir do XML autorizado.
- Transporte HTTP/1.1 sobre mTLS com SOAP 1.2 e JSON, sem raízes de confiança embutidas.

**Interfaces**
- CLI `carimbar`.
- Servidor MCP stdio em duas fases.
- Binding Python síncrono (PyO3/maturin).

## Instalação

`carimbar` 0.0.2 e `carimbar-cli` 0.0.2 já estão no crates.io. O pacote Python `carimbar` sai no PyPI a partir da 0.0.3. Para compilar a partir do repositório:

```bash
git clone https://github.com/seatrunk/carimbar.git
cd carimbar

# CLI e MCP com o perfil SHA-1 exigido pela NF-e (os dois binários de carimbar-cli)
cargo build --release -p carimbar-cli --bins --features nfe-sha1

# Binding Python (wheel abi3, com OpenSSL vendored para NF-e/DANFE)
cd crates/carimbar-py
maturin build --release --out dist
pip install dist/*.whl
```

**Requisitos**

| Item | Versão |
|------|--------|
| Rust | 1.89+ (o toolchain do repositório fixa 1.99.1) |
| Python (wheel) | 3.9+ |
| maturin | `>=1.9,<2` |

O build Rust padrão não usa OpenSSL. A feature `nfe-sha1` é opt-in, e o wheel Python já a habilita com OpenSSL vendored.

### Plataformas suportadas (wheels do PyPI)

A partir da 0.0.3 o PyPI recebe wheels **abi3** para Python ≥ 3.9, na matriz do job `wheels` de `.github/workflows/release.yml`:

- Linux x86_64 glibc (`manylinux: auto`). A tag real do wheel, confirmada no job `wheel` do CI (run [42718989553](https://github.com/seatrunk/carimbar/actions/runs/42718989553), imagem `quay.io/pypa/manylinux2014_x86_64`), é `manylinux2014_x86_64.manylinux_2_17_x86_64` (`manylinux_2_17` / manylinux2014).
- Linux aarch64 glibc `manylinux_2_28`.
- Linux x86_64 musl `musllinux_1_2`.
- macOS x86_64 (Intel).
- macOS arm64 (Apple Silicon).
- Windows x64.

Não há wheel musllinux aarch64 nem Windows arm64. Nessas plataformas não há wheel e a instalação cai no sdist.

O wheel linux-aarch64 é `manylinux_2_28` (glibc ≥ 2.28) e não instala no Amazon Linux 2 em Graviton (glibc 2.26) nem no Ubuntu 18.04 (glibc 2.27). Motivo: o `ring` 0.17 não compila com o gcc 4.8.5 da imagem manylinux2014.

Alternativas: instalar pela fonte (sdist, `pip install --no-binary carimbar carimbar` ou o fallback automático do pip; exige toolchain Rust ≥ 1.89 e Perl para o OpenSSL vendored) ou usar Amazon Linux 2023.

### Migração de crates (0.0.1 → 0.0.2)

Os crates `carimbar-types`, `carimbar-cert`, `carimbar-endpoints` e
`carimbar-danfe` 0.0.1 foram substituídos por `carimbar` com features: o que era
`carimbar_types::X` agora é `carimbar::types::X` (com `carimbar` ligado por
`default`), e assim por diante para `cert`, `endpoints`, `xml`, `dsig`,
`transport`, `vault`, `danfe`, `nfse` e `nfe`. Os binários `carimbar` e
`carimbar-mcp` agora saem de um único crate, `carimbar-cli`.

## Uso rápido

### Python

```python
from carimbar import Cofre, Emissor

# Use uma KEK real, com 64 caracteres hexadecimais.
kek = "07" * 32

cofre = Cofre("carimbar-cofre.db", kek)
with open("a1.p12", "rb") as arquivo:
    cofre.importar(
        "00000001000136",  # CNPJ
        "a1",              # id do certificado
        arquivo.read(),
        "senha-do-pfx",
    )

emissor = Emissor(
    "carimbar-cofre.db",
    "carimbar-journal.db",
    "00000001000136",
    kek,
    raizes="raizes-der",
)

with open("dps.xml", encoding="utf-8") as arquivo:
    dps_xml = arquivo.read()

resultado = emissor.emitir(dps_xml)

with open("danfse.pdf", "wb") as arquivo:
    arquivo.write(emissor.gerar_danfse(resultado["operacao"]))
```

- Operações bloqueantes liberam o GIL.
- As exceções distinguem validação, rejeição fiscal, transporte, produção, conflito e cofre.
- Use dados sintéticos nos testes e **nunca versione PFX, senha, KEK ou XML de empresa real.**

### CLI

```bash
export CARIMBAR_KEK=...        # 64 caracteres hexadecimais
export CARIMBAR_SENHA_PFX=...  # senha do A1 (nunca em argumento)
export CARIMBAR_RAIZES=...     # diretório com raízes de confiança em DER

# Importa o certificado A1 para o cofre
carimbar --empresa 00000001000136 \
  empresa importar --id a1 --pfx a1.p12

# Emite a NFS-e e gera o PDF
carimbar --empresa 00000001000136 \
  emitir --dps dps.xml --xml-saida nfse.xml --pdf-saida danfse.pdf
```

Com a feature `nfe-sha1`, a CLI também aceita `emitir-nfe`, `consultar-nfe` e `danfe-nfe`. A saída humana é o padrão; use `--json` para JSON.

**Códigos de saída**

| Código | Significado |
|:------:|-------------|
| `2` | Erro de uso |
| `3` | Rejeição fiscal |
| `4` | Falha de transporte ou resultado desconhecido |
| `5` | Produção bloqueada |
| `6` | Conflito de idempotência |

### MCP

Exemplo de configuração para um cliente MCP:

```json
{
  "mcpServers": {
    "carimbar": {
      "command": "carimbar-mcp",
      "env": {
        "CARIMBAR_KEK": "...",
        "CARIMBAR_COFRE": "carimbar-cofre.db",
        "CARIMBAR_JOURNAL": "carimbar-journal.db",
        "CARIMBAR_RAIZES": "raizes-der",
        "CARIMBAR_PRODUCAO": "0"
      }
    }
  }
}
```

O servidor expõe ferramentas `preparar_*` e `confirmar_*` em **duas fases**:

1. `preparar_*` valida e persiste a operação **sem enviar**, e devolve um token de uso único com validade de 300 segundos (ajustável em `CARIMBAR_MCP_TTL_SEGUNDOS`).
2. `confirmar_*` consome o token e envia a operação.

Importação de certificado, promoção de produção e segredos ficam exclusivamente na CLI, fora do alcance do agente.

## Segurança e produção

- Produção só é permitida com `CARIMBAR_PRODUCAO=1` (depois de `trim()`). Ausência, vazio, `0` e qualquer outro valor (`true`, `sim`, `2`, `off`, lixo) bloqueiam produção.
- Promover uma empresa exige chamada explícita com o CNPJ e o certificado. Não existe promoção automática nem valor padrão.
- O cofre cifra os segredos com uma DEK por empresa; a KEK fica fora do arquivo.
- O journal guarda XML, protocolo e estado da operação para evitar emissão duplicada.
- Testes de PR usam dados sintéticos e não acessam SEFAZ, Sefin ou ADN.
- Publicar e homologar são ações manuais, separadas da implementação.
- `scripts/check-no-certs.sh` bloqueia certificados e arquivos `.env` versionados.

## Estado do projeto

| Etapa | Situação |
|-------|----------|
| MVP implementado | Concluído, com testes locais |
| Homologação real | Pendente (manual e protegida) |
| Publicação `0.0.2` no crates.io | Publicada (`carimbar` e `carimbar-cli`) |
| Publicação `0.0.2` no PyPI | Pendente (`carimbar` ausente) |
| Uso em produção | Não recomendado |

Veja [release](docs/release.md) e [homologação](docs/homologacao.md) para os procedimentos.

## Roadmap

- **NFC-e:** fase futura.
- **Fase 2:** servidor self-hosted (HTTP + MCP remoto, fila, contingência e webhooks), construído sobre a mesma lib e licenciado sob AGPL-3.0.

O acompanhamento central está na [issue #4](https://github.com/seatrunk/carimbar/issues/4).

## Documentação

- [Plano de implementação do MVP](docs/plano-mvp.md)
- [Contratos fiscais, arquitetura e limites](docs/contratos-fiscais.md)
- [Release e publicação](docs/release.md)
- [Homologação manual protegida](docs/homologacao.md)

## Contribuindo

Contribuições são bem-vindas. Leia o [CONTRIBUTING.md](CONTRIBUTING.md) antes de abrir um PR e lembre-se: nunca inclua certificados, senhas ou XMLs reais em issues, PRs ou testes.

## Licenças

- A lib e o MCP usam a licença **Apache-2.0** (veja [LICENSE](LICENSE)).
- O futuro servidor self-hosted usará **AGPL-3.0**, num diretório próprio com LICENSE própria.
