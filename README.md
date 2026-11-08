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

Fundação implementada: tipos de domínio (CNPJ numérico e alfanumérico, chave de
acesso, ambiente e dinheiro), schemas oficiais com verificação de integridade,
infraestrutura de CI/release e validações básicas no binding Python. A leitura e
escrita de XML, a canonicalização e a validação XSD estão implementadas. Os
certificados A1 (senha, validade, titular por CNPJ e assinatura RSA-SHA256)
também estão implementados, assim como assinatura e verificação XMLDSig com
RSA-SHA256. O perfil RSA-SHA1 exigido pelo XSD de NF-e vendorizado está disponível
pela feature opcional `nfe-sha1`, com escolha explícita de perfil e backend OpenSSL.
O build padrão continua sem OpenSSL. A tabela de endpoints por UF, modelo e
ambiente está implementada com fonte e data de verificação (`carimbar-endpoints`).
O transporte HTTP/1.1 sobre mTLS com SOAP 1.2 e JSON está implementado
(`carimbar-transport`), sem raízes embutidas e sem reenvio cego de emissão.
Cofre, emissão, PDFs, CLI e MCP ainda são stubs; **não há emissão funcional de
notas fiscais**.

O [plano de implementação do MVP](docs/plano-mvp.md) organiza as etapas,
dependências e critérios de conclusão, acompanhados pela
[issue central #4](https://github.com/seatrunk/carimbar/issues/4). Os contratos
fiscais suportados, as fontes oficiais, as diferenças entre ambientes e as
decisões em aberto estão em
[contratos fiscais e arquitetura](docs/contratos-fiscais.md).
