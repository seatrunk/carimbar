# schemas

XSD oficiais vendorizados sem alteração. Cada pacote tem `SHA256SUMS`; o teste
`carimbar-xml/tests/schemas_sha256.rs` falha se algum arquivo mudar, sumir ou aparecer.

| Pasta | Origem | SHA-256 do zip | Baixado em |
|---|---|---|---|
| `PL_010b_v1.30/` | https://dfe-portal.svrs.rs.gov.br/NFE/DownloadArquivoEstatico/?sistema=NFE&tipoArquivo=2&nomeArquivo=PL_010b_NT2025_002_v1.30.zip (NT 2025.002 v1.30) | `2deaa8d430d0acb47deae06b9b6d1202dbb436baf3d2d3aa834a534a3d6fa1b8` | 2026-11-04 |
| `nfse_v1.01_20261020/` | https://www.gov.br/nfse/pt-br/biblioteca/documentacao-tecnica/producao-restrita/esquemas_prod_restrita_20261020.zip (produção restrita, v1.01) | `25fdcecc651a67a216496ab10d7566631e6ed10f721e37d62c7845dadb0ea267` | 2026-11-04 |
| `nfse_v1.01_20260209/` | https://www.gov.br/nfse/pt-br/biblioteca/documentacao-tecnica/documentacao-atual/nfse-esquemas_xsd-v1-01-20260209.zip (produção, v1.01; arquivos de `Schemas/1.01/`) | `e7935cbd9470527c6cc32984c1b2263e614183bf0139ce2733eaaed2de9a8072` | 2026-11-08 |

Para atualizar: baixe o novo pacote numa pasta nova, gere `SHA256SUMS` com
`sha256sum *.xsd | sort -k2 > SHA256SUMS` e registre a origem nesta tabela.
