# carimbar

**Emissão direta de NF-e e NFS-e Nacional para devs e agentes de IA, sem provedor intermediário.**

[![Licença: Apache-2.0](https://img.shields.io/badge/licen%C3%A7a-Apache--2.0-blue.svg)](LICENSE)
[![M8ven Score](https://m8ven.ai/badge/mcp/seatrunk-carimbar-14so9i?v=87bf47fd8efaf5443d30ca65ffc0782a)](https://m8ven.ai/mcp/seatrunk-carimbar-14so9i?s=readme)![Status: pré-homologação](https://img.shields.io/badge/status-pr%C3%A9--homologa%C3%A7%C3%A3o-orange) 
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

A publicação da versão `0.0.2` no crates.io e no PyPI ainda está pendente. Enquanto isso, compile a partir do repositório:

```bash
git clone https://github.com/seatrunk/carimbar.git
cd carimbar

# CLI e MCP com o perfil SHA-1 exigido pela NF-e
cargo build --release -p carimbar-cli --features nfe-sha1
cargo build --release -p carimbar-mcp --features nfe-sha1

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

- `CARIMBAR_PRODUCAO=0` bloqueia produção. A ausência da variável **não** promove a empresa.
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
| Publicação `0.0.2` (crates.io e PyPI) | Pendente (manual) |
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
