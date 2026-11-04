#!/usr/bin/env bash
# Falha se o repositório rastrear certificado, chave privada ou .env.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

por_nome=$(git ls-files -z | tr '\0' '\n' \
  | grep -iE '\.(pfx|p12|pem|key|crt|cer|der|jks|keystore)$|(^|/)\.env(\..*)?$' || true)

# Conteúdo PEM/PKCS#12 com extensão disfarçada. Este script fica fora da busca.
marcador='-----BEGIN [A-Z ]*(PRIVATE KEY|CERTIFICATE)-----'
por_conteudo=$(git ls-files -z -- ':!scripts/check-no-certs.sh' \
  | xargs -0 grep -lIE -- "$marcador" 2>/dev/null || true)

if [[ -n "$por_nome$por_conteudo" ]]; then
  echo "ERRO: arquivos de certificado/chave/segredo rastreados:" >&2
  printf '%s\n' $por_nome $por_conteudo | sort -u >&2
  exit 1
fi
echo "ok: nenhum certificado, chave ou .env rastreado"
