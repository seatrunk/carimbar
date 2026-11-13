# Rejeições da NFS-e Nacional

Este documento reúne as rejeições que aparecem com mais frequência na emissão
de NFS-e Nacional e o que fazer em cada uma. Ele complementa
[contratos fiscais](contratos-fiscais.md) e
[quem pode usar](quem-pode-usar.md).

A lista é uma seleção prática, não o catálogo completo. A fonte oficial é o
Anexo I e o Anexo II da
[documentação técnica do portal da NFS-e](https://www.gov.br/nfse/pt-br/biblioteca/documentacao-tecnica/documentacao-atual).
O Emissor Nacional pode incluir códigos novos sem aviso; consulte o Anexo antes
de tratar um código como definitivo.

## Como o carimbar devolve a rejeição

| Interface | Como chega | Código/ação |
|---|---|---|
| CLI | `carimbar emitir ...` falha com a mensagem da rejeição | código de saída `3` (`rejeição fiscal`) |
| Python | `ErroFiscal` (derivado de `ErroCarimbar`) com o texto `CÓDIGO: descrição` | trate a exceção e leia o código no início da mensagem |
| MCP | `confirmar_emissao` devolve `estado = "rejeitada"` e a lista `erros` | leia `erros[].codigo` e `erros[].descricao` |

Uma rejeição **não é** resultado desconhecido: o governo processou a DPS e
recusou. Corrija a causa e tente de novo. Com uma chave de idempotência (`id`,
`--idempotencia` ou `idempotencia=`), a nova tentativa reaproveita a operação e o
número da DPS quando possível.

## O que o carimbar barra antes de enviar

Estas regras são validadas localmente e evitam uma ida ao governo:

| Regra | O que o carimbar faz |
|---|---|
| E0010 | A série da DPS precisa estar entre 1 e 49999. `SerieDps::new_api`, `--pedido` e o MCP recusam fora dessa faixa. |
| E0712 | MEI só envia `indTotTrib = 0`; ME/EPP só envia `pTotTribSN`. |
| E0713 | Empresa fora do Simples não envia `indTotTrib` nem `pTotTribSN`; precisa de `vTotTrib` ou `pTotTrib`. |
| E0247 | O e-mail do tomador, do intermediário e do prestador é validado (um `@`, domínio com ponto e rótulos válidos). |

O `carimbar emitir --pedido` também gera `dhEmi` 60 segundos atrás do relógio
para reduzir o risco de a data de emissão ficar à frente do servidor do
governo.

## Município e convênio

| Código | O que quer dizer | O que fazer |
|---|---|---|
| E0037 | O município não está no cadastro de convênio do Sistema Nacional. Não se aplica ao MEI. | Confira `prestador.municipio`. Se estiver certo, a emissão depende da prefeitura. |
| E0038 | O convênio do município não está ativo. Não se aplica ao MEI. | Espere a prefeitura concluir a ativação. |
| E0039 | O município não usa os emissores nacionais. Não se aplica ao MEI nem, para competências a partir de 1º/11/2026, à ME/EPP com o ISS apurado pelo Simples. | Confira o regime e a competência. Fora dessas exceções, a nota sai pelo sistema da prefeitura. |
| E0016 | A competência é anterior à ativação do convênio do município. Não se aplica ao MEI. | Use uma competência posterior à ativação. |
| E0025 | A competência é anterior à autorização da empresa no município. | Peça à prefeitura para conferir a data de autorização no CNC. |
| E0041 | O município informado não é o do endereço do MEI no CNPJ. | Corrija `prestador.municipio`. |
| E0084 | O CNPJ não tem estabelecimento no município, pelo cadastro da Receita ou pelo CNC. | Corrija `prestador.municipio` ou peça a habilitação à prefeitura. |

## Cadastro e regime

| Código | O que quer dizer | O que fazer |
|---|---|---|
| E0116 | A prefeitura cadastrou a empresa no CNC e a DPS foi sem inscrição municipal. | Preencha `prestador.inscricao_municipal`. |
| E0119 | A inscrição municipal informada não está autorizada a emitir. | Fale com a prefeitura. |
| E0120 | A DPS foi com inscrição municipal, mas a empresa não está no CNC do município. | Remova `prestador.inscricao_municipal`. |
| E0160 | A situação no Simples Nacional não bate com o cadastro na data de competência. | Corrija `prestador.simples` (e `apuracao`, quando ME/EPP). |
| E0712 | Uma ME/EPP enviou `indTotTrib`, ou um MEI enviou `pTotTribSN`. | Use `total_tributos` compatível com o regime; veja a tabela acima. |
| E0713 | Uma empresa fora do Simples enviou `indTotTrib` ou `pTotTribSN`. | Informe `total_tributos` com `valores` ou `percentuais`. |
| E0010 | A série da DPS está fora da faixa da API. | Use uma série de 1 a 49999. |

## Serviço

| Código | O que quer dizer | O que fazer |
|---|---|---|
| E0310 | O código de tributação nacional não existe na lista nacional. | Corrija `servico` (6 dígitos). |
| E0312 | O município não tem o código parametrizado na competência. Não se aplica ao MEI. | Confira o código com o contador; se estiver certo, a correção é da prefeitura. |
| E0316 | O código NBS não existe na tabela. | Corrija `codigo_nbs` ou remova o campo. |
| E0370 | O serviço é de obra e exige o grupo `obra`. | O modelo ainda não cobre o grupo `obra`; não emita por este caminho. |
| E0390 | O serviço é do item 12 (eventos) e exige o grupo `atvEvento`. | O modelo ainda não cobre eventos na DPS. |
| E0532 | O código 99.01.01 só aceita "não incidência". | Use `trib_issqn = 4` (não incidência) quando for o caso. |
| E0540 | A prefeitura marcou o serviço como não incidente e a nota foi tributável. | Confira o código do serviço ou fale com a prefeitura. |
| E0234 | O serviço é devido no município do tomador e exige o endereço dele. | O modelo ainda não envia endereço do tomador; confira o código de serviço. |

## Tomador

| Código | O que quer dizer | O que fazer |
|---|---|---|
| E0190 | O CNPJ do tomador não existe no cadastro da Receita. | Corrija `tomador.cnpj`. |
| E0207 | O CPF do tomador não existe no cadastro da Receita. | Corrija `tomador.cpf`. |
| E0202 | O CNPJ do tomador é igual ao do prestador. | Uma empresa não emite nota para ela mesma. |
| E0247 | O e-mail do tomador é inválido. | Corrija `tomador.email` ou envie o pedido sem ele. |

## Cancelamento

| Código | O que quer dizer | O que fazer |
|---|---|---|
| E0822 | O prazo de cancelamento definido pela prefeitura acabou. | Peça análise fiscal pelo portal do Emissor Nacional ou fale com a prefeitura. |
| E0823 | O valor da nota passa do limite que a prefeitura aceita cancelar. | O mesmo caminho: análise fiscal ou prefeitura. |
| E0824 | A prefeitura não aceita cancelar nota sem tomador identificado. | O mesmo caminho. |
| E0827 | A prefeitura não aceita cancelar nota com tributos já recolhidos. | O mesmo caminho. |
| E0840 | Já existe um evento na nota que impede o cancelamento. | Espere a decisão da prefeitura. |

## Outras respostas

| Resposta | O que quer dizer | O que fazer |
|---|---|---|
| E0014 | O número da DPS já tem uma nota. | O carimbar avança automaticamente para o próximo número em `emitir --pedido`, `emitir_pedido` e no MCP. Com DPS em XML, escolha outro número e prepare de novo. |
| E1235 | A DPS não passou no esquema XML. | Costuma ser mudança de leiaute do governo; confira a versão do pacote em `schemas/` e abra uma issue. |
| `HTTP 403` | O governo recusou o certificado. | Confira se é um e-CNPJ ICP-Brasil do prestador e se está na validade. |
| `HTTP 409` | A DPS já foi processada. | O carimbar reconcilia pelo Id da DPS antes de reenviar; não force um novo envio. |

## Diagnóstico antes de emitir

`carimbar diagnostico` confere o certificado (validade e CNPJ), o kill switch e,
com `--municipio`, o convênio municipal. Ele sai com código diferente de zero se
alguma verificação falhar, o que permite usar em script de deploy.

```bash
carimbar --empresa 00000001000136 diagnostico --municipio 3550308
```
