# Contributing to Hypr-Claw

## Development Requirements

- Rust 1.75+
- Linux environment (Arch/Hyprland preferred)
- Git

## Setup

```bash
git clone https://github.com/yourusername/hypr-claw.git
cd hypr-claw
cargo build
```

## Validation Before PR

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Contribution Rules

1. Keep behavior deterministic and tool-driven.
2. Do not bypass permission and audit layers.
3. Keep docs aligned with real runtime behavior.
4. Add tests for any new behavior.
5. Keep changes scoped and reviewable.

## Documentation Policy

- Root docs are limited to `README.md`, `CONTRIBUTING.md`, and `LICENSE`.
- Technical documentation belongs in `docs/`.
- Remove stale status/progress files instead of accumulating them.

## Pull Request Checklist

1. All tests pass locally.
2. Lint and format pass.
3. User-facing behavior is documented.
4. No credentials or runtime state are committed.
5. `.gitignore` still protects generated artifacts.

## Security Reporting

Do not open a public issue for vulnerabilities.
Report security issues privately to project maintainers.
