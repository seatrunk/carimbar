# Homologação manual protegida

A issue [#21](https://github.com/seatrunk/carimbar/issues/21) exige evidência real
de NFS-e e NF-e em ambiente de teste. Isso **não roda em PR** e não pode ser
simulado no CI comum: depende de certificado A1 autorizado, raízes ICP-Brasil e
dados de uma empresa real. Este documento descreve o procedimento e o harness
protegido.

## Regras de segurança

- O workflow é apenas `workflow_dispatch`; não há gatilho de `push` ou `pull_request`.
- O job usa o environment `homologacao`, que exige aprovação do responsável.
- `CARIMBAR_PRODUCAO=0` é definido no workflow e no script; qualquer operação de
  produção é bloqueada pelo kill switch.
- Segredos ficam no environment, nunca no repositório. O script não imprime PFX,
  senha, KEK, XML, PDF nem a chave completa.
- O diretório de trabalho com certificado e XMLs é temporário e apagado no fim.
- O único artefato publicado é `resumo.json`, com dados redigidos. PDFs e XMLs
  completos ficam fora do artefato público.
- O script exige a string `HOMOLOGAR` no input e recusa execução com
  `CARIMBAR_PRODUCAO` diferente de `0`.

## Segredos do environment `homologacao`

| Segredo | Conteúdo |
|---|---|
| `CARIMBAR_EMPRESA` | CNPJ da empresa sintética ou autorizada no ambiente de teste |
| `CARIMBAR_PFX_B64` | PKCS#12 A1 em base64 |
| `CARIMBAR_SENHA_PFX` | Senha do PFX |
| `CARIMBAR_KEK` | 64 caracteres hexadecimais da KEK do cofre |
| `CARIMBAR_RAIZES_B64` | tar.gz em base64 com raízes de confiança em DER |
| `CARIMBAR_DPS_XML_B64` | DPS sintética/autorizada em base64, quando o modelo inclui NFS-e |
| `CARIMBAR_NFE_XML_B64` | NF-e em base64, quando o modelo inclui NF-e |
| `CARIMBAR_CANCELAR_CHAVE` | Opcional: chave de NFS-e para exercitar cancelamento |

O CNPJ, o município e os códigos de serviço da DPS/NF-e precisam corresponder ao
ambiente de teste e ao certificado. Não use dados de produção.

## Execução

1. Actions → **Homologação manual protegida** → Run workflow.
2. Escolha `nfse`, `nfe` ou `ambos`.
3. Digite `HOMOLOGAR`.
4. Aprove o environment `homologacao`.
5. Aguarde o job. O script compila a CLI com `nfe-sha1`, importa o A1 no cofre
   temporário, executa o fluxo escolhido e valida que os PDFs começam com `%PDF`.

O script executa:

- importação e listagem do certificado;
- `emitir` (NFS-e) com XML autorizado e DANFSe;
- `consultar` da NFS-e emitida;
- cancelamento, se `CARIMBAR_CANCELAR_CHAVE` estiver definido;
- `emitir-nfe` e `danfe-nfe`, quando o modelo inclui NF-e;
- `adn sincronizar --limite-paginas 2`;
- resumo redigido em `resumo.json`.

## Evidência para a #21

A issue só fecha com evidência verificável. Registre no comentário da issue:

- URL do run e SHA do commit;
- data/hora e ambiente (produção restrita/homologação);
- modelos exercitados e códigos/estados retornados;
- confirmação de que os PDFs foram gerados;
- confirmação de que o resumo não contém dados sensíveis;
- limitações encontradas.

Exemplo de resumo aceitável:

```json
{
  "modelos": [
    {"estado": "autorizada", "operacao": 1, "chave": "***1234"}
  ],
  "consultas": [
    {"chave": "***1234", "xml": null}
  ],
  "adn": {"documentos_novos": 0, "paginas": 1}
}
```

Não anexe XML ou PDF completos de empresa real. Se precisar guardar o material,
faça fora do GitHub e apenas referencie o hash/identificador redigido.

## Limitações

- A primeira execução real depende de certificado e dados externos ao repositório;
  até lá a #21 permanece aberta.
- O workflow não automatiza a inutilização de numeração nem CC-e; esses caminhos
  estão no cliente/fachada e podem ser exercitados por um script adicional.
- O ambiente de teste pode mudar URLs e regras; reconfira a tabela de endpoints e
  as notas técnicas antes de rodar.
