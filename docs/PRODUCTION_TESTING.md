# Production Testing Candidate

This document tracks readiness for production-style testing of Hypr-Claw (prompt-driven OS assistant for Arch + Hyprland). See `workleft.md` for full handoff context.

## Definition of Done (from workleft §10)

| Criterion | Status |
|-----------|--------|
| 1. TUI is stable and interruptible | **Done** – Stable panel layout, live refresh, `:interrupt` and `/interrupt` send real signal, queue/state visible |
| 2. App launch reliability is high for common apps on Arch | **Done** – `desktop.launch_app` uses aliases, desktop entries, flatpak/gtk-launch fallbacks |
| 3. Queue conflict handling is deterministic and user-visible | **Done** – Resource-aware conflicts, policy prompt (queue / run now / cancel bg), Task Log and queue panels show state |
| 4. Rate-limit behavior degrades gracefully | **Done** – Retry/backoff and 429 handling with fractional retry-after in LLM client |
| 5. End-to-end workflow tests pass for desktop tasks | **Partial** – Smoke E2E test exists; run with `--ignored` when binary + config present |
| 6. Manual smoke test passes for at least one full session with mixed tasks | **User** – Run manually |

## How to run tests

### Unit and integration (no config required)

From repo root:

```bash
cargo fmt --all
cargo check -p hypr_claw_tools -p hypr-claw-app -p hypr-claw-runtime
cargo test -p hypr_claw_tools --lib
cargo test -p hypr-claw-app --bin hypr-claw -- --nocapture
cargo test -p hypr-claw-runtime --lib
```

Or run all app tests (including integration):

```bash
cargo test -p hypr-claw-app
```

If `/tmp` is constrained:

```bash
mkdir -p .tmp
TMPDIR=/path/to/hypr-claw/.tmp cargo test -p hypr-claw-app
```

### E2E smoke test (optional, requires built binary + config)

Requires:

- Binary built: `cargo build --release` or `cargo build`
- Config present: `./data/config.yaml` (e.g. after `hypr-claw config reset` and setup)
- Run from **repo root** so `./data/config.yaml` is found

```bash
cargo test --test e2e_workflow_test -- --ignored
```

If the binary or config is missing, the test skips (no failure). With config and binary, it feeds `capabilities`, `queue status`, `exit` and asserts successful exit.

### Manual smoke run

```bash
./target/release/hypr-claw
# Then: capabilities, queue status, /tui, open vscode (or similar), exit
```

Or non-interactive:

```bash
printf "capabilities\nqueue status\nexit\n" | ./target/release/hypr-claw
```

## Remaining work (out of scope for “rest work”)

- **Trust/approval hardening** – Policy boundaries for full-auto (e.g. require confirmation for certain tools/souls)
- **Provider router** – Model fallback and quota-aware routing
- **Desktop workflow loops** – Mail/chat summarization, action retries
- **Broader E2E** – More workflows (e.g. “open app” under test env) and failure-injection tests

These are documented in `workleft.md` §5 and `docs/ROADMAP.md`.
