#!/usr/bin/env bash
# Testes do check-no-certs.sh. Gera material sintético num diretório temporário
# (nunca dentro do repo) e roda o script contra repos git descartáveis.
set -euo pipefail
script="$(cd "$(dirname "$0")" && pwd)/check-no-certs.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

falhas=0
caso() { # nome esperado(0|1) arquivo-origem destino
  local nome=$1 esperado=$2 origem=$3 destino=$4 repo
  repo=$(mktemp -d -p "$tmp")
  git -C "$repo" init -q
  cp "$origem" "$repo/$destino"
  git -C "$repo" add -f -- "$destino"
  local obtido=0
  "$script" "$repo" >/dev/null 2>&1 || obtido=1
  if [[ $obtido == "$esperado" ]]; then
    echo "ok   $nome"
  else
    echo "FAIL $nome (esperado exit $esperado, obtido $obtido)"
    falhas=$((falhas + 1))
  fi
}

# PKCS#12 sintético: chave e certificado autoassinado gerados agora, CN fictício.
openssl req -x509 -newkey rsa:2048 -nodes -days 1 -subj "/CN=carimbar-teste-sintetico" \
  -keyout "$tmp/k.pem" -out "$tmp/c.pem" 2>/dev/null
openssl pkcs12 -export -inkey "$tmp/k.pem" -in "$tmp/c.pem" -passout pass:teste \
  -out "$tmp/sintetico.p12" 2>/dev/null
openssl pkcs12 -export -inkey "$tmp/k.pem" -in "$tmp/c.pem" -passout pass:teste \
  -legacy -out "$tmp/legado.p12" 2>/dev/null || cp "$tmp/sintetico.p12" "$tmp/legado.p12"

# Binários que não são PKCS#12.
head -c 4096 /dev/urandom >"$tmp/aleatorio.bin"
openssl x509 -in "$tmp/c.pem" -outform DER -out "$tmp/cert.der" # SEQUENCE sem version 3
printf '\x30\x82\x01\x00\x02\x01\x03\x30\x00' >"$tmp/quase.bin"   # sem OID pkcs7-data
printf 'texto qualquer\n' >"$tmp/texto.txt"
printf '%s\n' "-----BEGIN PRIVATE KEY-----" >"$tmp/pem.txt"

caso "p12 renomeado .bin"            1 "$tmp/sintetico.p12" dado.bin
caso "p12 renomeado .txt"            1 "$tmp/sintetico.p12" notas.txt
caso "p12 legado (RC2/3DES) sem ext" 1 "$tmp/legado.p12"    blob
caso "p12 com extensão .p12"         1 "$tmp/sintetico.p12" a.p12
caso "p12 renomeado .xsd"            1 "$tmp/sintetico.p12" leiaute.xsd
caso "PEM disfarçado .txt"           1 "$tmp/pem.txt"       x.txt
caso "binário aleatório"             0 "$tmp/aleatorio.bin" r.bin
caso "certificado DER como .bin"     0 "$tmp/cert.der"      c.bin
caso "SEQUENCE+v3 sem OID"           0 "$tmp/quase.bin"     q.bin
caso "XSD real do repo"              0 "$(dirname "$script")/../schemas/PL_010b_v1.30/nfe_v4.00.xsd" n.xsd
caso "texto comum"                   0 "$tmp/texto.txt"     t.txt

echo "falhas: $falhas"
((falhas == 0))
