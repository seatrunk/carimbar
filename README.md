# carimbar

O nome vem do verbo: carimbar é o gesto que torna um documento oficial.

Emissão direta de NF-e e NFS-e Nacional para devs e agentes de IA, sem provedor
intermediário. O núcleo é em Rust, com CLI, servidor MCP local e binding Python.

## Estado

O MVP está implementado e coberto por testes locais, mas **ainda não há
homologação real** com certificado e empresa autorizados. Não use esta versão
para emitir notas fiscais em produção.

- NFS-e Nacional: DPS, emissão, consulta, cancelamento, distribuição ADN por NSU
  e parâmetros municipais.
- NF-e modelo 55: leiaute 4.00, IBS/CBS, autorização, consulta, eventos,
  inutilização e contingência SVC.
- Cofre multiempresa de certificados A1, com envelope encryption, rotação de
  chaves e auditoria encadeada.
- Fachada `Emissor` com journal local, idempotência, reconciliação e bloqueio
  explícito de produção.
- DANFSe e DANFE em PDF a partir do XML autorizado.
- CLI `carimbar`, servidor MCP stdio em duas fases e binding Python síncrono.
- Transporte HTTP/1.1 sobre mTLS com SOAP 1.2 e JSON, sem raízes embutidas.

A publicação `0.0.2` no crates.io e no PyPI é uma ação manual e ainda está
pendente; o procedimento e a recuperação estão em
[release](docs/release.md). A homologação real também é manual e protegida
([homologação](docs/homologacao.md)).

## Escopo do MVP

- NFS-e Nacional (Emissor Nacional / ADN) e NF-e modelo 55. NFC-e fica para uma
  fase futura.
- Emissão direta na SEFAZ e no ADN, sem provedor intermediário.
- Núcleo em Rust, com binding Python (PyO3/maturin).
- Servidor MCP local.
- Cofre multiempresa de certificados A1, criptografados em repouso.
- Geração de DANFE e DANFSe em PDF.
- Campos de IBS e CBS da reforma tributária (NT 2025.002-RTC, PL_010).

## Instalação

Enquanto a publicação `0.0.2` não conclui, compile do repositório:

```sh
git clone https://github.com/seatrunk/carimbar.git
cd carimbar

# CLI e MCP com o perfil SHA-1 exigido pela NF-e.
cargo build --release -p carimbar-cli --features nfe-sha1
cargo build --release -p carimbar-mcp --features nfe-sha1

# Binding Python (wheel abi3, com OpenSSL vendored para NF-e/DANFE).
cd crates/carimbar-py
maturin build --release --out dist
pip install dist/*.whl
```

Requisitos: Rust 1.89 ou mais novo (o toolchain do repositório fixa 1.99.1),
Python 3.9 ou mais novo para o wheel e `maturin>=1.9,<2` para compilar o binding.
O build Rust padrão não usa OpenSSL; a feature `nfe-sha1` é opt-in e o wheel
Python já a habilita com OpenSSL vendored.

## Uso rápido

### Python

```python
from carimbar import Cofre, Emissor

# Exemplo: use uma KEK real, com 64 caracteres hexadecimais.
kek = "07" * 32

cofre = Cofre("carimbar-cofre.db", kek)
with open("a1.p12", "rb") as arquivo:
    cofre.importar(
        "00000001000136",
        "a1",
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

`Dinheiro` entra e sai como texto; valores fiscais nunca passam por `float`.
As operações bloqueantes liberam o GIL e as exceções distinguem validação,
rejeição fiscal, transporte, produção, conflito e cofre. Use dados sintéticos
em testes e nunca versione PFX, senha, KEK ou XML de empresa real.

### CLI

```sh
export CARIMBAR_KEK=...        # 64 caracteres hexadecimais
export CARIMBAR_SENHA_PFX=...  # senha do A1, nunca em argumento
export CARIMBAR_RAIZES=...     # diretório com raízes de confiança em DER

carimbar --empresa 00000001000136 \
  empresa importar --id a1 --pfx a1.p12

carimbar --empresa 00000001000136 \
  emitir --dps dps.xml --xml-saida nfse.xml --pdf-saida danfse.pdf
```

Com a feature `nfe-sha1`, a CLI também aceita `emitir-nfe`, `consultar-nfe` e
`danfe-nfe`. A saída humana é o padrão; `--json` retorna JSON. Códigos de saída
são estáveis (`2` uso, `3` rejeição fiscal, `4` transporte/resultado
desconhecido, `5` produção bloqueada, `6` conflito de idempotência).

### MCP

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

O servidor expõe `preparar_*` e `confirmar_*` em duas fases: a preparação valida
e persiste a operação sem enviar, e devolve um token de uso único com validade
de 300 segundos (configurável em `CARIMBAR_MCP_TTL_SEGUNDOS`). Importação de
certificado, promoção de produção e segredos ficam na CLI.

## Segurança e produção

- `CARIMBAR_PRODUCAO=0` bloqueia produção. A ausência da variável não promove a
  empresa: a promoção exige uma chamada explícita com o CNPJ e o certificado.
- Promover uma empresa exige a confirmação explícita com o CNPJ. Não existe
  promoção automática nem valor padrão.
- O cofre cifra os segredos com DEK por empresa e KEK fora do arquivo. O
  journal guarda XML, protocolo e estado da operação para evitar emissão
  duplicada.
- Testes de PR usam dados sintéticos e não acessam SEFAZ, Sefin ou ADN.
- Publicar e homologar são ações explícitas, separadas da implementação;
  `scripts/check-no-certs.sh` bloqueia certificados e `.env` versionados.

## Fase 2

Um servidor self-hosted (HTTP + MCP remoto, fila, contingência e webhooks),
construído sobre a mesma lib e licenciado sob AGPL-3.0.

## Licenças

- A lib e o MCP usam a licença Apache-2.0 (veja [LICENSE](LICENSE)).
- O futuro servidor usará AGPL-3.0 e ficará num diretório próprio, com LICENSE
  própria.

## Documentação

- [Plano de implementação do MVP](docs/plano-mvp.md)
- [Contratos fiscais, arquitetura e limites](docs/contratos-fiscais.md)
- [Release e publicação](docs/release.md)
- [Homologação manual protegida](docs/homologacao.md)
- [Como contribuir](CONTRIBUTING.md)
- [Issue central #4](https://github.com/seatrunk/carimbar/issues/4)

## Contexto

Desde 01/11/2026, ME e EPP optantes pelo Simples Nacional são obrigadas a emitir
NFS-e pelo Emissor Nacional (Resolução CGSN 191/2026).
