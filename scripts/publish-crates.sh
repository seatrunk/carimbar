#!/usr/bin/env bash
# Publica no crates.io os crates do workspace que ainda não existem na versão atual,
# em ordem topológica, um por vez. Idempotente: pula o que já está no índice.
# Respeita os limites do crates.io (https://crates.io/docs/rate-limits):
#   - crate novo: rajada de 5 por conta, depois 1 a cada 10 min;
#   - versão nova de crate existente: rajada de 30, depois 1 por minuto.
# No 429, dorme até o horário que a resposta indica (+30 s) e tenta de novo o
# mesmo crate. Em 5xx/erro de rede, backoff exponencial. Em 401/403 (token sem o
# escopo publish-new/publish-update, expirado ou fora do padrão carimbar*), para
# na hora: repetir não resolve.
#
# Estado atual: dois crates publicáveis.
#   carimbar     0.0.2 -> update (0.0.1 já existe)
#   carimbar-cli 0.0.2 -> novo
#
# Uso: CARGO_REGISTRY_TOKEN=... scripts/publish-crates.sh [--dry-run]
# Variáveis: MAX_TENTATIVAS (20), MAX_ESPERA_S (1800), INTERVALO_NOVO_S (0: só
# reage ao 429; >0 espaça crates novos depois do 5º, para publicação manual).
set -euo pipefail

DRY_RUN=0
[[ "${1:-}" == "--dry-run" ]] && DRY_RUN=1
MAX_TENTATIVAS="${MAX_TENTATIVAS:-20}"
MAX_ESPERA_S="${MAX_ESPERA_S:-1800}"
INTERVALO_NOVO_S="${INTERVALO_NOVO_S:-0}"
RESUMO="${GITHUB_STEP_SUMMARY:-/dev/null}"

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

# 0 = o crate já existe no crates.io (qualquer versão): publicar é publish-update.
crate_existe() {
  curl -fsS -o /dev/null -H 'Cache-Control: no-cache' "https://index.crates.io/$(caminho_indice "$1")" 2>/dev/null
}

# A publicação anterior pode demorar a aparecer no índice; sem isso o próximo
# crate falha ao resolver a dependência recém-publicada.
esperar_indice() {
  local nome="$1" versao="$2"
  for _ in $(seq 1 12); do
    ja_publicado "$nome" "$versao" && return 0
    sleep 10
  done
  return 1
}

# Plano antes de publicar: o que pula, o que é crate novo e o que é versão nova.
declare -A TIPO=()
novos=0; updates=0
{
  echo "| Crate | Versão | Ação |"
  echo "|---|---|---|"
} >>"$RESUMO"
for linha in "${ORDEM[@]}"; do
  read -r nome versao <<<"$linha"
  if ja_publicado "$nome" "$versao"; then
    TIPO[$nome]=pula
  elif crate_existe "$nome"; then
    TIPO[$nome]=update; updates=$((updates + 1))
  else
    TIPO[$nome]=novo; novos=$((novos + 1))
  fi
  echo "plano: $nome $versao -> ${TIPO[$nome]}"
  echo "| $nome | $versao | ${TIPO[$nome]} |" >>"$RESUMO"
done
extra=$(( novos > 5 ? (novos - 5) * 10 : 0 ))
echo "crates novos: $novos (publish-new); versões novas: $updates (publish-update); espera estimada pelo limite: ~${extra} min"
if (( updates > 0 )); then
  echo "::notice::há $updates crate(s) existentes: o token precisa do escopo publish-update (ou use trusted publishing)"
fi

publicados_novos=0
for linha in "${ORDEM[@]}"; do
  read -r nome versao <<<"$linha"
  if [[ "${TIPO[$nome]}" == pula ]] || ja_publicado "$nome" "$versao"; then
    echo "pula $nome $versao: já está no crates.io"
    continue
  fi
  if (( DRY_RUN )); then
    echo "publicaria $nome $versao (${TIPO[$nome]})"
    continue
  fi
  if [[ "${TIPO[$nome]}" == novo ]] && (( INTERVALO_NOVO_S > 0 && publicados_novos >= 5 )); then
    echo "espaçamento de crate novo: ${INTERVALO_NOVO_S}s antes de $nome"
    sleep "$INTERVALO_NOVO_S"
  fi
  backoff=30
  for ((t = 1; t <= MAX_TENTATIVAS; t++)); do
    echo "publica $nome $versao (tentativa $t)"
    set +e
    saida=$(cargo publish -p "$nome" --locked 2>&1)
    rc=$?
    set -e
    echo "$saida"
    if (( rc == 0 )); then
      [[ "${TIPO[$nome]}" == novo ]] && publicados_novos=$((publicados_novos + 1))
      esperar_indice "$nome" "$versao" \
        || echo "::warning::$nome $versao publicado, mas o índice ainda não confirmou"
      break
    fi
    if grep -Eiq 'already (been )?(uploaded|published)|already exists' <<<"$saida"; then
      echo "pula $nome $versao: já foi publicado"
      break
    fi
    if grep -Eiq 'status 40[13]|required permissions|not authorized|invalid authentication|token.*expired|expired.*token' <<<"$saida"; then
      echo "::error::crates.io recusou o token ao publicar $nome (${TIPO[$nome]}). Crate novo exige publish-new; crate existente exige publish-update; o padrão de crates deve cobrir carimbar*. Repetir não resolve."
      exit "$rc"
    fi
    if grep -Eiq 'status 429|too many' <<<"$saida"; then
      quando=$(grep -oE 'try again after [A-Za-z]{3}, [0-9]{2} [A-Za-z]{3} [0-9]{4} [0-9:]{8} GMT' <<<"$saida" | sed 's/try again after //' | head -1)
      alvo=$(date -d "${quando:-now + 10 minutes}" +%s)
      espera=$(( alvo - $(date +%s) + 30 ))
      (( espera < 30 )) && espera=30
      if (( espera > MAX_ESPERA_S )); then
        echo "::error::crates.io pede espera de ${espera}s (até $quando), acima de MAX_ESPERA_S=${MAX_ESPERA_S}; rode de novo depois desse horário"
        exit 1
      fi
      echo "limite do crates.io (${TIPO[$nome]}); espera ${espera}s (até ${quando:-+10 min})"
      sleep "$espera"
      continue
    fi
    if grep -Eiq 'status 5[0-9]{2}|timed out|connection (reset|refused)|could not resolve host|SSL' <<<"$saida"; then
      echo "erro transitório; backoff ${backoff}s"
      sleep "$backoff"
      backoff=$(( backoff * 2 > 600 ? 600 : backoff * 2 ))
      continue
    fi
    if grep -Eq 'failed to select a version|no matching package named' <<<"$saida"; then
      echo "dependência ainda não visível no índice; espera 30s e tenta de novo"
      sleep 30
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
if (( DRY_RUN )); then
  echo "dry-run: os crates acima seriam publicados"
else
  echo "todos os crates publicáveis estão no crates.io"
fi
