#!/usr/bin/env bash
# Mostra o estado de publicação da versão atual do workspace, sem publicar nada.
# Usado pelo job `verificar` do release e por auditoria manual:
#   ./scripts/release-status.sh
#
# A saída é uma tabela Markdown. Crates ausentes não são erro: a #6 é justamente
# publicar o que falta. Só falha se `cargo metadata` falhar.
set -euo pipefail

meta=$(cargo metadata --no-deps --format-version 1 --locked)
mapfile -t pacotes < <(python3 -c '
import json, sys
m = json.load(sys.stdin)
for p in sorted(m["packages"], key=lambda p: p["name"]):
    if p.get("publish") == []:
        continue
    print(p["name"], p["version"])
' <<<"$meta")

if (( ${#pacotes[@]} == 0 )); then
  echo "::error::nenhum crate publicável encontrado no workspace"
  exit 1
fi

# Caminho do crate no índice esparso (mesmo cálculo de publish-crates.sh).
caminho_indice() {
  local n="${1,,}"
  case ${#n} in
    1) echo "1/$n" ;; 2) echo "2/$n" ;; 3) echo "3/${n:0:1}/$n" ;;
    *) echo "${n:0:2}/${n:2:2}/$n" ;;
  esac
}

# Imprime "<publicado|ausente>|<últimas versões>" para a versão planejada.
estado_crate() {
  local nome="$1" versao="$2" corpo
  corpo=$(curl -fsS -H 'Cache-Control: no-cache' \
    "https://index.crates.io/$(caminho_indice "$nome")" 2>/dev/null) || {
    echo "ausente|—"
    return
  }
  python3 -c '
import json, sys
versao = sys.argv[1]
versoes = list(dict.fromkeys(json.loads(l)["vers"] for l in sys.stdin if l.strip()))
estado = "publicado" if versao in versoes else "ausente"
print(estado + "|" + (", ".join(versoes[-3:]) or "—"))
' "$versao" <<<"$corpo"
}

echo "| Pacote | Versão planejada | crates.io | Versões no crates.io | PyPI |"
echo "|---|---|---|---|---|"

for linha in "${pacotes[@]}"; do
  read -r nome versao <<<"$linha"
  resultado=$(estado_crate "$nome" "$versao")
  estado="${resultado%%|*}"
  versoes="${resultado#*|}"
  if [[ "$nome" == "carimbar" ]]; then
    if curl -fsS -o /dev/null "https://pypi.org/pypi/carimbar/$versao/json" 2>/dev/null; then
      pypi="publicado"
    else
      pypi="ausente"
    fi
    echo "| $nome | $versao | $estado | $versoes | $pypi (pacote Python) |"
  else
    echo "| $nome | $versao | $estado | $versoes | — |"
  fi
done

echo
echo "Crates publicáveis: ${#pacotes[@]}. Ausentes não bloqueiam o PR; são publicados só no Release."
