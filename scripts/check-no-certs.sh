#!/usr/bin/env bash
# Falha se o repositório rastrear certificado, chave privada ou .env.
#
# Detecta:
#  1. por nome: .pfx .p12 .pem .key .crt .cer .der .jks .keystore e .env*;
#  2. por conteúdo PEM: marcador BEGIN ... PRIVATE KEY / CERTIFICATE em arquivo de texto;
#  3. por conteúdo PKCS#12 (qualquer extensão): DER que começa com SEQUENCE (0x30),
#     seguido de `version INTEGER 3` (02 01 03) e do OID pkcs7-data
#     1.2.840.113549.1.7.1 (06 09 2a 86 48 86 f7 0d 01 07 01) nos primeiros 64 bytes.
#
# Uso: check-no-certs.sh [raiz-do-repo]   (padrão: repo do diretório atual)
set -euo pipefail
cd "${1:-$(git rev-parse --show-toplevel)}"

por_nome=$(git ls-files -z | tr '\0' '\n' \
  | grep -iE '\.(pfx|p12|pem|key|crt|cer|der|jks|keystore)$|(^|/)\.env(\..*)?$' || true)

# PEM. Este script fica fora da busca porque contém o próprio marcador.
marcador='-----BEGIN [A-Z ]*(PRIVATE KEY|CERTIFICATE)-----'
por_pem=$(git ls-files -z -- ':!scripts/check-no-certs.sh' \
  | xargs -0 grep -lIE -- "$marcador" 2>/dev/null || true)

# Cabeçalho DER de um PFX: 30 <len> 02 01 03 30 <len> 06 09 <oid pkcs7-data>.
# <len> é curto (< 0x80), longo (0x81..0x84 + 1..4 bytes) ou indefinido (0x80, BER).
eh_pkcs12() {
  local hex
  hex=$(od -An -tx1 -N64 -v -- "$1" | tr -d ' \n')
  [[ ${hex:0:2} == 30 ]] || return 1
  local l=$((16#${hex:2:2})) pos=4
  if ((l > 0x80)); then
    ((l <= 0x84)) || return 1
    pos=$((4 + 2 * (l - 0x80)))
  fi
  [[ ${hex:pos:6} == 020103 ]] || return 1
  [[ $hex == *06092a864886f70d010701* ]]
}

por_p12=()
while IFS= read -r -d '' f; do
  case "$f" in
    # Texto conhecido: pula só se for texto de fato (grep -I), para um PFX
    # renomeado para .xsd/.rs não passar.
    *.xsd | *.xml | *.md | *.rs | *.toml | *.lock | *.yml | *.yaml | *.py | *.pyi | *.sh | */SHA256SUMS | LICENSE | .gitignore | .gitattributes)
      if grep -qI '' -- "$f" 2>/dev/null || [[ ! -s $f ]]; then continue; fi ;;
  esac
  [[ -f $f ]] || continue
  if eh_pkcs12 "$f"; then por_p12+=("$f"); fi
done < <(git ls-files -z)

if [[ -n "$por_nome$por_pem${por_p12[*]:-}" ]]; then
  echo "ERRO: arquivos de certificado/chave/segredo rastreados:" >&2
  printf '%s\n' $por_nome $por_pem "${por_p12[@]}" | sed '/^$/d' | sort -u >&2
  exit 1
fi
echo "ok: nenhum certificado, chave, PKCS#12 ou .env rastreado"
