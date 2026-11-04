# carimbar

O nome vem do verbo: carimbar é o gesto que torna um documento oficial.

## Objetivo

Facilitar a integração da emissão de notas fiscais a qualquer SaaS, tanto para devs quanto para agentes de IA (via MCP).

## Escopo do MVP

- NFS-e Nacional (Emissor Nacional / ADN) e NF-e modelo 55. A NFC-e fica para uma fase futura.
- Emissão direta na SEFAZ e no ADN, sem provedor intermediário.
- Núcleo em Rust, com binding Python (PyO3/maturin).
- Servidor MCP local.
- Cofre multiempresa de certificados A1, criptografados em repouso.
- Geração de DANFE e DANFSe em PDF.
- Campos de IBS e CBS da reforma tributária (NT 2025.002-RTC, PL_010).

## Fase 2

Um servidor self-hosted (HTTP + MCP remoto, fila, contingência e webhooks), construído sobre a mesma lib e licenciado sob AGPL-3.0.

## Licenças

- A lib e o MCP usam a licença Apache-2.0 (veja [LICENSE](LICENSE)).
- O futuro servidor usará AGPL-3.0 e ficará num diretório próprio, com LICENSE própria.

## Contexto

Desde 01/11/2026, ME e EPP optantes pelo Simples Nacional são obrigadas a emitir NFS-e pelo Emissor Nacional (Resolução CGSN 191/2026).

## Status

Em planejamento. Ainda não há código.
