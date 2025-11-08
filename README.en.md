# AnyTLS-RS

[![Version](https://img.shields.io/badge/version-0.4.0-blue.svg)](https://github.com/jxo-me/anytls-rs)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org)
[![Edition](https://img.shields.io/badge/edition-2024-blue.svg)](https://doc.rust-lang.org/edition-guide/)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

High-performance and observable AnyTLS implementation in Rust, designed to mitigate TLS-in-TLS fingerprinting and interoperate with sing-box outbound → anytls-rs server setups.

[中文版](README.md)

---

## ✨ Highlights

- **Multi-protocol proxy**: built-in SOCKS5 plus new HTTP CONNECT/plain proxy (`anytls-client -H/--http-listen`)
- **Session pooling**: configurable idle check/timeout/warm-up via short flags (`-I/-T/-M`) and env vars
- **UDP-over-TCP**: interoperable with sing-box v1.2, sends SYNACK immediately, covered by loopback tests
- **TLS management**: load existing PEM certs or auto-generate `anytls.local` self-signed pair (scripts handle it)
- **Automation**: `scripts/dev-up.sh` for the fastest spin-up, `scripts/dev-verify.sh` for local regression
- **Documentation**: project radar, developer quickstart, MVP plan, FAQ, ADR, troubleshooting, and more
- **Observability**: structured `tracing`, session/stream identifiers, planned span coverage for critical paths

---

## 🚀 Quick Start

### 1. Requirements

- Rust 1.70+ / cargo (Rustup recommended)
- Optional: `openssl` when importing external certificates
- macOS/Linux: ensure scripts are executable (`chmod +x scripts/*.sh`)

### 2. One-command experience

```bash
# Fire up server + client (SOCKS5 on 127.0.0.1:1080 by default)
./scripts/dev-up.sh

# Run smoke verification (SOCKS5 + HTTP probes) and tear down cleanly
./scripts/dev-verify.sh
```

Both scripts rely on `examples/singbox/certs/anytls.local.{crt,key}`. Override ports/passwords via `SERVER_ADDR`, `CLIENT_ADDR`, `HTTP_ADDR`, `PASSWORD`, etc.

### 3. Manual walkthrough (two terminals)

```bash
# Terminal A: anytls-server (self-signed by default, or use your own cert)
cargo run --release --bin anytls-server -- \
  -l 0.0.0.0:8443 \
  -p your_password \
  --cert ./examples/singbox/certs/anytls.local.crt \
  --key  ./examples/singbox/certs/anytls.local.key \
  -I 30 -T 120 -M 1

# Terminal B: anytls-client (SOCKS5 + HTTP proxy)
cargo run --release --bin anytls-client -- \
  -l 127.0.0.1:1080 \
  -s 127.0.0.1:8443 \
  -p your_password \
  -I 30 -T 120 -M 1 \
  -H 127.0.0.1:8080

# Terminal C: probe traffic
curl --socks5-hostname 127.0.0.1:1080 http://httpbin.org/get
curl -x http://127.0.0.1:8080 http://httpbin.org/get
```

---

## 🧩 sing-box Integration

- Template config: `examples/singbox/outbound-anytls.jsonc`
- Guide & checklist: `examples/singbox/README.md`
- Quick validation: `sing-box check -c examples/singbox/outbound-anytls.jsonc`

| sing-box field | anytls-rs mapping | Notes |
| --- | --- | --- |
| `password` | `anytls-{server,client} -p` | Must match |
| `idle_session_check_interval` | `-I / IDLE_SESSION_CHECK_INTERVAL` | Seconds |
| `idle_session_timeout` | `-T / IDLE_SESSION_TIMEOUT` | Seconds |
| `min_idle_session` | `-M / MIN_IDLE_SESSION` | Warm-up session count |
| `tls.certificate_path` | `--cert` / `CERT_PATH` | Accepts self-signed cert |

---

## 🗺️ Project Layout

```
anytls-rs/
├── docs/                       # Architectural notes, quickstarts, FAQ, ADR, troubleshooting
├── examples/singbox/           # sing-box outbound integration samples
├── scripts/                    # Local bootstrap & verification utilities
├── src/
│   ├── bin/                    # CLI binaries (anytls-server/client)
│   ├── client/                 # Client core (SOCKS5/HTTP/session pool/UDP-over-TCP)
│   ├── server/                 # Server core (TCP/UDP handlers)
│   ├── protocol/               # Frame definitions & codec
│   ├── session/                # Session & stream multiplexing
│   └── util/                   # TLS, auth, error handling, helpers
├── tests/                      # Integration tests (including UDP roundtrip)
└── benches/                    # Criterion benchmarks
```

For a responsibility-oriented overview, see `docs/00-project-radar.md`.

---

## ⚙️ CLI Reference

### anytls-server

| Option | Description |
| --- | --- |
| `-l, --listen <ADDR>` | Listen address (default `0.0.0.0:8443`) |
| `-p, --password <PASSWORD>` | Shared password (required) |
| `--cert <FILE>` / `--key <FILE>` | PEM certificate/private key (optional) |
| `-I, --idle-session-check-interval <SECS>` | Hint for clients (recommended check interval) |
| `-T, --idle-session-timeout <SECS>` | Hint for idle timeout |
| `-M, --min-idle-session <COUNT>` | Hint for minimum warm idle sessions |

### anytls-client

| Option | Description |
| --- | --- |
| `-l, --listen <ADDR>` | SOCKS5 bind (default `127.0.0.1:1080`) |
| `-s, --server <ADDR>` | Server address (default `127.0.0.1:8443`) |
| `-p, --password <PASSWORD>` | Shared password (required) |
| `-H, --http-listen <ADDR>` | HTTP proxy bind (optional) |
| `-I, --idle-session-check-interval <SECS>` | Session check interval (default 30) |
| `-T, --idle-session-timeout <SECS>` | Idle session timeout (default 60) |
| `-M, --min-idle-session <COUNT>` | Warm idle sessions (default 1) |

Environment variable shortcuts (see `docs/01-dev-quickstart.md` and `scripts/dev-up.sh`):
`IDLE_SESSION_CHECK_INTERVAL`, `IDLE_SESSION_TIMEOUT`, `MIN_IDLE_SESSION`, `HTTP_ADDR`, etc.

---

## ✅ Testing & Benchmarks

- Unit tests: frame codec, padding, error mapping, consistency assertions
- Integration tests: SOCKS5 + HTTP proxy, UDP-over-TCP loopback (`tests/udp_roundtrip.rs`)
- Benchmarks: session reuse concurrency (1/10/100 streams), p50/p95 latency, throughput
- Smoke automation: `./scripts/dev-verify.sh`

For the proposed minimum observability/test suite, check `docs/03-test-and-observability.md`.

---

## 📚 Recommended Reading

- `docs/00-project-radar.md` – project radar, risk matrix, code map
- `docs/01-dev-quickstart.md` – developer quickstart, pitfalls, script cheatsheet
- `docs/02-feature-mvp-plan.md` – sing-box MVP incremental plan
- `docs/adr/0001-singbox-anytls-e2e.md` – ADR for outbound ↔ server integration
- `docs/FAQ.md` – parameter alignment, cert handling, HTTP proxy Q&A
- `docs/TROUBLESHOOTING.md` – common failure modes & recovery steps

---

## 🛠️ Development

```bash
# Formatting & lint
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings

# Tests
cargo test

# Benchmarks
cargo bench
```

Please include tests, documentation updates, and ensure lint/test checks pass before opening a PR.

---

## 🔐 Security Notes

- TLS built on `rustls`, supports TLS 1.2/1.3, works with self-signed or CA-issued certs
- Authentication uses SHA256 with configurable padding schemes
- Session pool reduces reconnection overhead; parameters configurable per deployment
- Observability via `tracing`; suggestion: `RUST_LOG=info,anytls=debug` for richer spans

---

## 📦 License

MIT License – see [LICENSE](LICENSE).

---

## 🙏 Acknowledgements

- [anytls-go](https://github.com/anytls/anytls-go) – reference implementation
- [sing-box](https://github.com/SagerNet/sing-box) – outbound protocol alignment
- All contributors and community members

---

**Like the project? Consider starring ⭐ the repository!**

