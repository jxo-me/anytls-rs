#!/usr/bin/env bash
set -euo pipefail

CONFIG=${CONFIG:-$(dirname "${BASH_SOURCE[0]}")/outbound-anytls.local.jsonc}
SINGBOX_BIN=${SINGBOX_BIN:-sing-box}

if [[ ! -f "${CONFIG}" ]]; then
  echo "[ERROR] Config not found: ${CONFIG}"
  exit 1
fi

exec "${SINGBOX_BIN}" run --config "${CONFIG}"

