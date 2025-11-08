#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

PASSWORD=${PASSWORD:-testpass}
SERVER_ADDR=${SERVER_ADDR:-127.0.0.1:8443}
CLIENT_ADDR=${CLIENT_ADDR:-127.0.0.1:1080}
CERT_PATH=${CERT_PATH:-${ROOT_DIR}/examples/singbox/certs/anytls.local.crt}
KEY_PATH=${KEY_PATH:-${ROOT_DIR}/examples/singbox/certs/anytls.local.key}
CURL_TARGET=${CURL_TARGET:-http://httpbin.org/get}
SERVER_LOG=${SERVER_LOG:-info,anytls=debug}
CLIENT_LOG=${CLIENT_LOG:-info,anytls=debug}
SERVER_IDLE_SESSION_CHECK_INTERVAL=${SERVER_IDLE_SESSION_CHECK_INTERVAL:-}
SERVER_IDLE_SESSION_TIMEOUT=${SERVER_IDLE_SESSION_TIMEOUT:-}
SERVER_MIN_IDLE_SESSION=${SERVER_MIN_IDLE_SESSION:-}

USE_CERT=false
if [[ -f "${CERT_PATH}" && -f "${KEY_PATH}" ]]; then
  USE_CERT=true
fi

server_pid=""
client_pid=""
response_file="$(mktemp)"

cleanup() {
  if [[ -n "${client_pid}" ]]; then
    kill "${client_pid}" 2>/dev/null || true
  fi
  if [[ -n "${server_pid}" ]]; then
    kill "${server_pid}" 2>/dev/null || true
  fi
  rm -f "${response_file}"
}
trap cleanup EXIT

wait_for_port() {
  local addr=$1
  local timeout=${2:-15}
  python3 - "$addr" "$timeout" <<'PY'
import socket, sys, time
host_port, timeout = sys.argv[1], float(sys.argv[2])
host, port = host_port.rsplit(":", 1)
port = int(port)
deadline = time.time() + timeout
while time.time() < deadline:
    sock = socket.socket()
    sock.settimeout(1)
    try:
        sock.connect((host, port))
    except Exception:
        time.sleep(0.25)
    else:
        sock.close()
        sys.exit(0)
sys.exit(f"timeout waiting for {host_port}")
PY
}

free_port() {
  local addr=$1
  local port="${addr##*:}"
  if command -v lsof >/dev/null 2>&1; then
    local pids
    pids=$(lsof -ti tcp:"${port}" || true)
    if [[ -n "${pids}" ]]; then
      echo "[dev-verify] Port ${port} busy; terminating processes: ${pids}"
      kill ${pids} 2>/dev/null || true
      sleep 1
    fi
  fi
}

free_port "${SERVER_ADDR}"
free_port "${CLIENT_ADDR}"

echo "[dev-verify] Starting anytls-server @ ${SERVER_ADDR}"
(
  cd "${ROOT_DIR}"
  cargo build --release --bin anytls-server --bin anytls-client >/dev/null
)

SERVER_BIN="${ROOT_DIR}/target/release/anytls-server"
CLIENT_BIN="${ROOT_DIR}/target/release/anytls-client"

SERVER_CMD=("${SERVER_BIN}" -l "${SERVER_ADDR}" -p "${PASSWORD}")
if [[ "${USE_CERT}" == "true" ]]; then
  echo "[dev-verify] Using TLS cert: ${CERT_PATH}"
  SERVER_CMD+=(--cert "${CERT_PATH}" --key "${KEY_PATH}")
else
  echo "[dev-verify] No certificate provided; server will generate self-signed certificate"
fi
if [[ -n "${SERVER_IDLE_SESSION_CHECK_INTERVAL}" ]]; then
  SERVER_CMD+=(--idle-session-check-interval "${SERVER_IDLE_SESSION_CHECK_INTERVAL}")
fi
if [[ -n "${SERVER_IDLE_SESSION_TIMEOUT}" ]]; then
  SERVER_CMD+=(--idle-session-timeout "${SERVER_IDLE_SESSION_TIMEOUT}")
fi
if [[ -n "${SERVER_MIN_IDLE_SESSION}" ]]; then
  SERVER_CMD+=(--min-idle-session "${SERVER_MIN_IDLE_SESSION}")
fi

RUST_LOG="${SERVER_LOG}" "${SERVER_CMD[@]}" &
server_pid=$!

wait_for_port "${SERVER_ADDR}" 20

echo "[dev-verify] Starting anytls-client @ ${CLIENT_ADDR}"
RUST_LOG="${CLIENT_LOG}" "${CLIENT_BIN}" \
  -l "${CLIENT_ADDR}" \
  -s "${SERVER_ADDR}" \
  -p "${PASSWORD}" &
client_pid=$!

wait_for_port "${CLIENT_ADDR}" 20

echo "[dev-verify] Running curl probe via SOCKS5 (${CURL_TARGET})"
curl --socks5-hostname "${CLIENT_ADDR}" --fail --silent --show-error \
  "${CURL_TARGET}" > "${response_file}"

python3 - "${response_file}" <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
with path.open() as fp:
    data = json.load(fp)
url = data.get("url")
if not url:
    raise SystemExit("[dev-verify] ❌ Unexpected response payload")
print(f"[dev-verify] ✅ Proxy check succeeded: {url}")
PY

echo "[dev-verify] Success. Tearing down..."

