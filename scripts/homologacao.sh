#!/usr/bin/env bash
# Homologação manual protegida. NÃO roda em PR.
#
# Requer:
# - CARIMBAR_HOMOLOGACAO_CONFIRMAR=HOMOLOGAR
# - CARIMBAR_PRODUCAO=0 (kill switch)
# - CARIMBAR_EMPRESA
# - CARIMBAR_PFX_B64, CARIMBAR_SENHA_PFX, CARIMBAR_KEK
# - CARIMBAR_RAIZES_B64 (tar.gz com raízes DER)
# - CARIMBAR_DPS_XML_B64 e/ou CARIMBAR_NFE_XML_B64 conforme CARIMBAR_MODELO
#
# Não imprime XML, PDF, PFX, senha, KEK ou chave completa. O diretório de
# trabalho é temporário e apagado ao sair.
set -euo pipefail

falhar() {
  echo "::error::$*" >&2
  exit 1
}

[[ "${CARIMBAR_HOMOLOGACAO_CONFIRMAR:-}" == "HOMOLOGAR" ]] \
  || falhar "defina CARIMBAR_HOMOLOGACAO_CONFIRMAR=HOMOLOGAR"
[[ "${CARIMBAR_PRODUCAO:-0}" == "0" ]] \
  || falhar "produção deve permanecer bloqueada (CARIMBAR_PRODUCAO=0)"

: "${CARIMBAR_EMPRESA:?defina CARIMBAR_EMPRESA}"
: "${CARIMBAR_PFX_B64:?defina CARIMBAR_PFX_B64}"
: "${CARIMBAR_SENHA_PFX:?defina CARIMBAR_SENHA_PFX}"
: "${CARIMBAR_KEK:?defina CARIMBAR_KEK}"
: "${CARIMBAR_RAIZES_B64:?defina CARIMBAR_RAIZES_B64}"

modelo="${CARIMBAR_MODELO:-ambos}"
[[ "$modelo" == "nfse" || "$modelo" == "nfe" || "$modelo" == "ambos" ]] \
  || falhar "CARIMBAR_MODELO inválido: $modelo"

bin="${CARIMBAR_BIN:-target/release/carimbar}"
[[ -x "$bin" ]] || falhar "binário não encontrado: $bin"

trabalho="$(mktemp -d)"
trap 'rm -rf "$trabalho"' EXIT

printf '%s' "$CARIMBAR_PFX_B64" | base64 -d > "$trabalho/a1.p12"
printf '%s' "$CARIMBAR_RAIZES_B64" | base64 -d > "$trabalho/raizes.tar.gz"
mkdir -p "$trabalho/raizes"
tar -xzf "$trabalho/raizes.tar.gz" -C "$trabalho/raizes"

export CARIMBAR_COFRE="$trabalho/cofre.db"
export CARIMBAR_JOURNAL="$trabalho/journal.db"
export CARIMBAR_RAIZES="$trabalho/raizes"
export CARIMBAR_SENHA_PFX
export CARIMBAR_KEK
export CARIMBAR_PRODUCAO=0

id_cert="${CARIMBAR_CERTIFICADO_ID:-a1}"
"$bin" --empresa "$CARIMBAR_EMPRESA" empresa importar \
  --id "$id_cert" --pfx "$trabalho/a1.p12" >/dev/null
"$bin" --empresa "$CARIMBAR_EMPRESA" empresa listar >/dev/null

resumo="${CARIMBAR_RESUMO:-resumo.json}"
python3 - "$resumo" <<'PY'
import json, sys
json.dump({"modelos": [], "consultas": [], "artefatos": []}, open(sys.argv[1], "w"))
PY

redigir_chave() {
  python3 -c 'import json,sys; d=json.load(sys.stdin); c=d.get("chave"); d["chave"]=("***"+c[-4:]) if c else None; d.pop("xml",None); d.pop("xml_nfse",None); print(json.dumps(d,ensure_ascii=False))'
}

registrar() {
  python3 - "$resumo" "$1" "$2" <<'PY'
import json, sys
caminho, secao, valor = sys.argv[1], sys.argv[2], json.loads(sys.argv[3])
d = json.load(open(caminho))
d.setdefault(secao, []).append(valor)
json.dump(d, open(caminho, "w"), ensure_ascii=False, indent=2)
PY
}

if [[ "$modelo" == "nfse" || "$modelo" == "ambos" ]]; then
  : "${CARIMBAR_DPS_XML_B64:?defina CARIMBAR_DPS_XML_B64 para NFS-e}"
  printf '%s' "$CARIMBAR_DPS_XML_B64" | base64 -d > "$trabalho/dps.xml"
  saida="$("$bin" --empresa "$CARIMBAR_EMPRESA" --json emitir \
    --dps "$trabalho/dps.xml" \
    --xml-saida "$trabalho/nfse.xml" \
    --pdf-saida "$trabalho/danfse.pdf")"
  registrar "$resumo" "modelos" "$(printf '%s' "$saida" | redigir_chave)"
  head -c 4 "$trabalho/danfse.pdf" | grep -q '%PDF' || falhar "DANFSe inválido"
  chave="$(printf '%s' "$saida" | python3 -c 'import json,sys; print(json.load(sys.stdin)["chave"])')"
  consulta="$("$bin" --empresa "$CARIMBAR_EMPRESA" --json consultar --chave "$chave")"
  registrar "$resumo" "consultas" "$(printf '%s' "$consulta" | redigir_chave)"
  if [[ -n "${CARIMBAR_CANCELAR_CHAVE:-}" ]]; then
    cancelamento="$("$bin" --empresa "$CARIMBAR_EMPRESA" --json cancelar \
      --chave "$CARIMBAR_CANCELAR_CHAVE" --motivo 1 \
      --descricao "Cancelamento de homologacao protegida")"
    registrar "$resumo" "eventos" "$(printf '%s' "$cancelamento" | redigir_chave)"
  fi
fi

if [[ "$modelo" == "nfe" || "$modelo" == "ambos" ]]; then
  : "${CARIMBAR_NFE_XML_B64:?defina CARIMBAR_NFE_XML_B64 para NF-e}"
  printf '%s' "$CARIMBAR_NFE_XML_B64" | base64 -d > "$trabalho/nfe.xml"
  saida="$("$bin" --empresa "$CARIMBAR_EMPRESA" --json emitir-nfe \
    --nfe "$trabalho/nfe.xml" --saida "$trabalho/nfe-autorizada.xml")"
  registrar "$resumo" "modelos" "$(printf '%s' "$saida" | redigir_chave)"
  operacao="$(printf '%s' "$saida" | python3 -c 'import json,sys; print(json.load(sys.stdin)["operacao"])')"
  "$bin" --empresa "$CARIMBAR_EMPRESA" danfe-nfe \
    --operacao "$operacao" --saida "$trabalho/danfe.pdf" >/dev/null
  head -c 4 "$trabalho/danfe.pdf" | grep -q '%PDF' || falhar "DANFE inválido"
fi

sincronizacao="$("$bin" --empresa "$CARIMBAR_EMPRESA" --json adn sincronizar --limite-paginas 2)"
registrar "$resumo" "adn" "$sincronizacao"

python3 - "$resumo" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print(json.dumps(d, ensure_ascii=False, indent=2))
PY
