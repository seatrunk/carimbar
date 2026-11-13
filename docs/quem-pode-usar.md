# Quem pode usar

O carimbar fala direto com a Sefin Nacional e com o ADN. Ele não é um provedor e
não intermedeia a nota: a empresa precisa ter certificado, cadastro e regime
compatíveis com o que a NFS-e Nacional aceita.

> **Aviso:** o MVP ainda não passou por homologação real com uma empresa
> autorizada. Use em produção restrita até concluir o procedimento de
> [homologação](homologacao.md). Leia também as
> [rejeições comuns](rejeicoes.md).

## Regimes atendidos

| Regime | NFS-e Nacional | Observação |
|---|---|---|
| MEI | Sim | `indTotTrib = 0`. Não depende de convênio municipal na maioria dos casos. |
| ME/EPP optante pelo Simples | Sim, com o ISS apurado pelo Simples | `pTotTribSN`. Desde 1º/11/2026 a emissão pelo Emissor Nacional é obrigatória (Resolução CGSN 191/2026). |
| ME/EPP com o ISS fora do Simples | Depende do município | O carimbar monta a DPS, mas o município precisa usar o ambiente nacional. |
| Empresa fora do Simples | Depende do município e do serviço | O `totTrib` precisa ser detalhado (`vTotTrib` ou `pTotTrib`), porque `indTotTrib` e `pTotTribSN` são recusados (E0713). |
| Pessoa física | Não | O MVP emite com CNPJ do prestador. |

## O que o carimbar cobre hoje

- Emissão de NFS-e a partir de uma DPS em XML ou de um pedido JSON simples.
- Consulta de NFS-e por chave e consulta de DPS.
- Cancelamento (`e101101`).
- Distribuição do ADN por NSU.
- Parâmetros municipais: convênio, alíquotas, retenções e benefícios.
- DANFSe em PDF a partir do XML autorizado.
- Numeração automática da DPS por empresa, ambiente e série.
- Idempotência por chave do cliente (por exemplo, o id da venda).
- Validações de série (E0010), `totTrib` por regime (E0712/E0713) e e-mail
  (E0247) antes do envio.

## O que ainda não está modelado

Estes grupos existem no leiaute, mas não são montados pelo modelo `Dps` atual:

- endereço e telefone do prestador e do tomador;
- comércio exterior e tomador no exterior;
- obra (`obra`) e evento (`atvEvento`) na DPS;
- benefício municipal, dedução, redução e desconto condicionado;
- retenção de tributos federais (PIS, COFINS, CSLL, IRRF e previdência);
- crédito presumido, diferimento, imóveis e bens móveis no grupo `IBSCBS`;
- `regApIBSCBSSN` e `cAtvSN` da NT 009, ainda sem regra em produção;
- NFC-e (modelo 65) e contingência além da SVC da NF-e.

Para esses casos, a DPS pode ser montada por fora e enviada em XML. O carimbar
ainda valida Id, ambiente, CNPJ, assinatura e regras de negócio cobertas pelo
modelo.

## Municípios

O Emissor Nacional não substitui o cadastro municipal. Antes de emitir, confira:

1. o município adere ao ambiente nacional (E0037/E0038/E0039);
2. a empresa está autorizada no CNC, quando o município exigir (E0025/E0116);
3. o código de serviço está parametrizado como tributável (E0310/E0312/E0540);
4. a competência é posterior à ativação do convênio (E0016).

Use `carimbar diagnostico --municipio <IBGE>` para consultar o convênio e
`carimbar parametros convenio --municipio <IBGE>` para ver os campos retornados.

## Requisitos por instalação

| Item | O que precisa |
|---|---|
| Certificado | A1 (PKCS#12) e-CNPJ ICP-Brasil, dentro da validade, do mesmo CNPJ do prestador |
| Município | Código IBGE de 7 dígitos, com convênio ativo quando aplicável |
| Regime | `simples` = 1 (não optante), 2 (MEI) ou 3 (ME/EPP); `apuracao` para ME/EPP |
| Série | 1 a 49999 (E0010) |
| Competência | `AAAA-MM-DD`; se omitida, a data do `dhEmi` |
| Valor | Em centavos (`valor_centavos`) ou texto decimal (`valor`); nunca `float` |
| Produção | `CARIMBAR_PRODUCAO=1` e promoção explícita da empresa |

## Serviços

O carimbar não mantém uma lista própria de códigos de serviço. O código de
tributação nacional (6 dígitos) vem do pedido e é validado pelo governo
(E0310). O município define quais códigos aceita e em que competência
(E0312). Serviços de obra, eventos e mão de obra com ISSQN devido no município
do tomador ainda não são montados pelo modelo.
