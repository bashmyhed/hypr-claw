# Hypr-Claw

Hypr-Claw is a local, tool-driven OS assistant for Linux with Hyprland-aware controls, persistent memory, and a REPL workflow.

## Current State

This repository now focuses on a cleaner production layout:

- Core docs are kept in `docs/`
- Root status/progress markdown files were removed
- Runtime-generated data and credentials are ignored by Git
- Main project docs are `README.md`, `CONTRIBUTING.md`, and `LICENSE`

## Key Capabilities

- Persistent terminal agent session (REPL)
- First-run onboarding (preferred name + system profile scan)
- Tool-only execution enforcement for action requests
- Structured OS tools for filesystem, process, desktop, Hyprland, wallpaper, and system info
- Approval flow for system-critical actions
- Multi-thread task chats (`/task new`, `/task switch`, `/task list`, `/task close`)
- Soul system with auto-routing and manual switching

## Repository Layout

- `hypr-claw-app/`: binary entrypoint, REPL, onboarding, soul/thread controls
- `hypr-claw-runtime/`: agent loop, tool call flow, compaction integration
- `hypr-claw-tools/`: structured tool schemas and OS capability wrappers
- `hypr-claw-infra/`: audit logger, permission engine, lock/session infrastructure
- `crates/`: modular workspace crates (core, memory, policy, providers, tasks, etc.)
- `souls/`: soul profiles and prompt files
- `docs/`: maintained technical documentation

## Requirements

Base:

- Rust 1.75+
- Arch Linux (target environment)
- Hyprland and `hyprctl` for workspace/window controls

Recommended tool dependencies for full desktop automation:

- `swww` (wallpaper)
- `wtype` and/or `ydotool` (typing, key/mouse input)
- `wlrctl` (pointer control fallback)
- `grim` and/or `hyprshot` (screenshots)
- `tesseract` and `tesseract-data-eng` (OCR tools)

Example install on Arch:

```bash
sudo pacman -S --needed swww wtype ydotool wlrctl grim hyprshot tesseract tesseract-data-eng
```

## Build and Run

```bash
cargo build --release
./target/release/hypr-claw
```

## First Run Flow

On first startup the agent will:

1. Ask for provider selection and credentials/config
2. Ask what to call the user
3. Ask permission for first-time system profile scan
4. Show profile summary and allow correction
5. Start persistent REPL session

## Provider Notes

Agent-mode tool execution requires function-calling support.

Supported in agent mode:

- NVIDIA
- Google
- Local OpenAI-compatible endpoint

Configured but currently blocked for tool-calling agent mode:

- Antigravity
- Gemini CLI
- Codex

## REPL Commands

Core commands:

- `help`
- `status`
- `tasks`
- `profile`
- `scan`
- `clear`
- `exit`

Soul commands:

- `soul list`
- `soul switch <id>`
- `soul auto on|off`

Task thread commands:

- `/task new <title>`
- `/task list`
- `/task switch <id>`
- `/task close <id>`

## Security and Safety Model

- Every tool has a JSON schema
- Permission tiers are enforced per tool
- Audit logging records tool execution
- System-critical actions require explicit approval
- Tool invocation is required when user intent implies action

## Runtime Data

Runtime files are intentionally excluded from version control and recreated at runtime:

- `data/context/`
- `data/sessions/`
- `data/credentials/`
- `data/tasks/`
- `data/audit.log`

## Development

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

See `docs/` for architecture and runtime behavior details.
