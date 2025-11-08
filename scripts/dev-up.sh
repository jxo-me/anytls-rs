#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

PASSWORD=${PASSWORD:-testpass}
SERVER_ADDR=${SERVER_ADDR:-127.0.0.1:8443}
CLIENT_ADDR=${CLIENT_ADDR:-127.0.0.1:1080}
CERT_PATH=${CERT_PATH:-${ROOT_DIR}/examples/singbox/certs/anytls.local.crt}
KEY_PATH=${KEY_PATH:-${ROOT_DIR}/examples/singbox/certs/anytls.local.key}
USE_CERT="false"
HTTP_ADDR=${HTTP_ADDR:-}
IDLE_SESSION_CHECK_INTERVAL=${IDLE_SESSION_CHECK_INTERVAL:-}
IDLE_SESSION_TIMEOUT=${IDLE_SESSION_TIMEOUT:-}
MIN_IDLE_SESSION=${MIN_IDLE_SESSION:-}
SERVER_IDLE_SESSION_CHECK_INTERVAL=${SERVER_IDLE_SESSION_CHECK_INTERVAL:-}
SERVER_IDLE_SESSION_TIMEOUT=${SERVER_IDLE_SESSION_TIMEOUT:-}
SERVER_MIN_IDLE_SESSION=${SERVER_MIN_IDLE_SESSION:-}

if [[ -f "${CERT_PATH}" && -f "${KEY_PATH}" ]]; then
  USE_CERT="true"
fi

SERVER_LOG=${SERVER_LOG:-info,anytls=debug}
CLIENT_LOG=${CLIENT_LOG:-info,anytls=debug}

server_pid=""
client_pid=""

cleanup() {
  if [[ -n "${client_pid}" ]]; then
    kill "${client_pid}" 2>/dev/null || true
  fi
  if [[ -n "${server_pid}" ]]; then
    kill "${server_pid}" 2>/dev/null || true
  fi
}
trap cleanup EXIT

echo "[dev-up] Starting anytls-server on ${SERVER_ADDR}"
SERVER_CMD=(cargo run --release --bin anytls-server -- -l "${SERVER_ADDR}" -p "${PASSWORD}")
if [[ "${USE_CERT}" == "true" ]]; then
  echo "[dev-up] Using TLS cert: ${CERT_PATH}"
  SERVER_CMD+=(--cert "${CERT_PATH}" --key "${KEY_PATH}")
else
  echo "[dev-up] No TLS cert provided; server will generate self-signed certificate"
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

(
  cd "${ROOT_DIR}"
  RUST_LOG="${SERVER_LOG}" "${SERVER_CMD[@]}"
) &
server_pid=$!

sleep 1

echo "[dev-up] Starting anytls-client SOCKS5 proxy on ${CLIENT_ADDR}"
CLIENT_CMD=(cargo run --release --bin anytls-client -- \
  -l "${CLIENT_ADDR}" \
  -s "${SERVER_ADDR}" \
  -p "${PASSWORD}")

if [[ -n "${HTTP_ADDR}" ]]; then
  echo "[dev-up] HTTP proxy enabled on ${HTTP_ADDR}"
  CLIENT_CMD+=(--http-listen "${HTTP_ADDR}")
fi
if [[ -n "${IDLE_SESSION_CHECK_INTERVAL}" ]]; then
  CLIENT_CMD+=(--idle-session-check-interval "${IDLE_SESSION_CHECK_INTERVAL}")
fi
if [[ -n "${IDLE_SESSION_TIMEOUT}" ]]; then
  CLIENT_CMD+=(--idle-session-timeout "${IDLE_SESSION_TIMEOUT}")
fi
if [[ -n "${MIN_IDLE_SESSION}" ]]; then
  CLIENT_CMD+=(--min-idle-session "${MIN_IDLE_SESSION}")
fi

(
  cd "${ROOT_DIR}"
  RUST_LOG="${CLIENT_LOG}" "${CLIENT_CMD[@]}"
) &
client_pid=$!

echo "[dev-up] anytls-server pid=${server_pid}"
echo "[dev-up] anytls-client pid=${client_pid}"
echo "[dev-up] Validate via: curl --socks5-hostname ${CLIENT_ADDR} http://httpbin.org/get"
if [[ -n "${HTTP_ADDR}" ]]; then
  echo "[dev-up] HTTP test: curl --proxy http://${HTTP_ADDR} http://httpbin.org/get"
fi

wait "${server_pid}" "${client_pid}"

