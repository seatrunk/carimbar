#!/usr/bin/env bash
# Publica no crates.io os crates do workspace que ainda não existem na versão atual,
# em ordem topológica, um por vez. Idempotente: pula o que já está no índice.
# Respeita o limite de crate novo do crates.io: no 429, dorme até o horário que a
# resposta indica (+30 s) e tenta de novo o mesmo crate.
# Uso: CARGO_REGISTRY_TOKEN=... scripts/publish-crates.sh [--dry-run]
set -euo pipefail

DRY_RUN=0
[[ "${1:-}" == "--dry-run" ]] && DRY_RUN=1
MAX_TENTATIVAS="${MAX_TENTATIVAS:-20}"

# Ordem topológica dos crates publicáveis (publish != false), calculada do cargo metadata.
mapfile -t ORDEM < <(cargo metadata --no-deps --format-version 1 --locked | python3 -c '
import json, sys
m = json.load(sys.stdin)
pk = {p["name"]: p for p in m["packages"] if p.get("publish") != []}
deps = {n: {d["name"] for d in p["dependencies"] if d["name"] in pk and d.get("kind") != "dev"} for n, p in pk.items()}
feito, ordem = set(), []
while len(ordem) < len(pk):
    prontos = sorted(n for n in pk if n not in feito and deps[n] <= feito)
    if not prontos:
        sys.exit("ciclo de dependências entre crates do workspace")
    for n in prontos:
        feito.add(n); ordem.append(n)
for n in ordem:
    print(n, pk[n]["version"])
')

if (( ${#ORDEM[@]} == 0 )); then
  echo "::error::não consegui calcular a ordem dos crates"
  exit 1
fi

# Caminho do crate no índice esparso (https://doc.rust-lang.org/cargo/reference/registry-index.html).
caminho_indice() {
  local n="${1,,}"
  case ${#n} in
    1) echo "1/$n" ;; 2) echo "2/$n" ;; 3) echo "3/${n:0:1}/$n" ;;
    *) echo "${n:0:2}/${n:2:2}/$n" ;;
  esac
}

ja_publicado() {
  local nome="$1" versao="$2" corpo
  corpo=$(curl -fsS -H 'Cache-Control: no-cache' "https://index.crates.io/$(caminho_indice "$nome")" 2>/dev/null) || return 1
  python3 -c 'import json,sys; v=sys.argv[1]; sys.exit(0 if any(json.loads(l)["vers"]==v for l in sys.stdin if l.strip()) else 1)' "$versao" <<<"$corpo"
}

for linha in "${ORDEM[@]}"; do
  read -r nome versao <<<"$linha"
  if ja_publicado "$nome" "$versao"; then
    echo "pula $nome $versao: já está no crates.io"
    continue
  fi
  if (( DRY_RUN )); then
    echo "publicaria $nome $versao"
    continue
  fi
  for ((t = 1; t <= MAX_TENTATIVAS; t++)); do
    echo "publica $nome $versao (tentativa $t)"
    set +e
    saida=$(cargo publish -p "$nome" --locked 2>&1)
    rc=$?
    set -e
    echo "$saida"
    (( rc == 0 )) && break
    if grep -q '429' <<<"$saida"; then
      quando=$(grep -oE 'try again after [A-Za-z]{3}, [0-9]{2} [A-Za-z]{3} [0-9]{4} [0-9:]{8} GMT' <<<"$saida" | sed 's/try again after //' | head -1)
      alvo=$(date -d "${quando:-now + 10 minutes}" +%s)
      espera=$(( alvo - $(date +%s) + 30 ))
      (( espera < 30 )) && espera=30
      echo "limite de crate novo; espera ${espera}s (até ${quando:-+10 min})"
      sleep "$espera"
      continue
    fi
    echo "::error::falha não recuperável ao publicar $nome"
    exit "$rc"
  done
  if (( t > MAX_TENTATIVAS )); then
    echo "::error::$nome não publicou em $MAX_TENTATIVAS tentativas"
    exit 1
  fi
done
echo "todos os crates publicáveis estão no crates.io"
