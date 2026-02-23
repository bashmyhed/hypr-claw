# Hypr-Claw Work Handoff

Last updated: 2026-02-24
Branch: `main`
Base commit when handoff was prepared: `8b5ae1d`

## 1. Product Direction (Do Not Drift)

This product is a prompt-driven local OS assistant for Arch + Hyprland.
Keep it close to OpenClaw behavior:

1. Prompt-first autonomy, not rigid workflow scripting.
2. Tool-driven execution (no explanation-only fallback when action is required).
3. Persistent session memory and capability learning.
4. Practical desktop control reliability for real workflows (mail/chat/editor/browser).
5. TUI-first package now; widget package later.

User preference explicitly stated:

1. Keep prompt dependency high (OpenClaw-like).
2. Improve reliability via better tooling/context, not by over-caging model behavior.
3. First package focus is TUI agent OS assistant.

## 2. Current State Summary

Core working:

1. Persistent REPL loop with onboarding, souls, task threads.
2. Capability registry persisted at `data/capabilities/<user>.json`.
3. Prompt augmentation includes runtime context and capability hints.
4. Autonomy modes: `prompt_first` and `guarded`.
5. Tool execution feed and runtime dashboard in REPL/TUI.
6. Structured OS tools for filesystem/process/hyprland/desktop/system operations.

Recent reliability upgrades already implemented:

1. LLM retry policy improved for rate limits and non-retryable failures.
2. Provider 429 handling includes retry-delay parsing for fractional seconds.
3. App command handling hardened for slash/colon command routing.
4. TUI behavior improved (Enter refresh, width clamping).
5. `desktop.launch_app` now resolves apps via aliases + desktop entries + flatpak/gtk-launch fallbacks.
6. Soul auto-routing adjusted so Telegram-like intents route to communication before mail.
7. Capability registry includes `preferred_launchers` and injects them into prompt context.
8. Supervisor tasks now store inferred resource tags (desktop_input/filesystem/network/compute/general).
9. Queue start now blocks if another supervisor task is already running.
10. Startup reconciliation now marks stale `running` supervisor tasks as `failed` to avoid queue deadlock after restart.
11. TUI now supports operational shortcuts (`:status`, `:dash`, `:tasks`, `:queue-*`, `:caps`, `:interrupt`, `:repl`).
12. `interrupt` / `/interrupt` now emits a real interrupt signal (`notify_waiters`) instead of hint-only text.
13. TUI now has view paging controls for feed/messages (`:feed-up/down/top/new`, `:msg-up/down/top/new`) with `/tui ...` command equivalents.
14. TUI shows window ranges for decision feed and recent messages (e.g., `21-30 / 85`).
15. TUI live mode is now available: `/tui live on|off` and `:live-on/:live-off` with timeout-driven refresh (Linux `poll()`; no extra crate needed).
16. TUI now has dedicated Task Log panel with paging controls (`/tui log up|down|top|latest`, `:log-up/:log-down/:log-top/:log-new`).
17. Supervisor/background lifecycle now emits explicit task-event telemetry into Task Log (`queued`, `running`, `blocked`, `completed`, `failed`, background cancellation/reconcile updates).
18. Conflict handling for desktop-exclusive prompts now offers explicit policy choice: `queue`, `run now`, or `cancel running background tasks and run`.
19. `scan` command now uses review workflow: run scan -> show profile summary + capability diff -> optional edit -> explicit apply/discard before persisting.
20. Queue scheduler now uses resource-aware conflict checks instead of blanket running-task blocking:
   - shared resources: `network`, `compute`
   - exclusive resources: `desktop_input`, `filesystem`, `general` (and unknown tags by default)
21. Background conflict detection is now resource-aware and reports explicit overlap reason (instead of simple desktop-exclusive heuristic).
22. Added isolated background execution lane for supervisor tasks with non-exclusive resources:
   - tasks with only shared resources (`network`, `compute`) can auto-run in background
   - each background supervisor task uses its own `AgentLoop` + tool-registry scope
   - session key isolation: `<session>::sup::<task_id>`
   - supervisor state is reconciled from background task completion/failure.
23. Supervisor state now persists `background_task_id` linkage for running background supervisor tasks so reconciliation can survive process restarts cleanly.
24. Added control commands for supervisor/background runs:
   - `queue stop <sup_id|bg_id>` cancels queued tasks or cancels running background tasks by supervisor ID or background task ID
   - `queue retry <sup_id>` re-enqueues completed/failed/cancelled tasks
25. TUI now exposes queue control shortcuts with arguments:
   - `:queue-stop <id>`
   - `:queue-retry <id>`
   and shows supervisor `background_task_id` in queue panels.
26. Added `queue status` command for compact supervisor summary:
   - counts by state
   - running rows with background IDs
   - retry candidates list.
27. TUI now shows explicit `Actionable IDs` panel (copy-friendly stop/retry targets) and supports `:queue-status`.
28. Added bulk operator controls:
   - `queue stop all` to cancel all queued/running supervisor tasks (including background task cancellation)
   - `queue retry failed` to re-enqueue all failed/cancelled supervisor tasks.
29. Foreground stop semantics hardened: if user-cancelled, later interrupt/failure completion paths no longer overwrite supervisor state to `failed`.
30. TUI shortcuts extended:
   - `:queue-stop-all`
   - `:queue-retry-failed`
31. Extended retry scopes:
   - `queue retry completed`
   - `queue retry all`
   and alias handling for `queue retry cancelled` (same behavior as failed/cancelled bucket).
32. TUI shortcuts added:
   - `:queue-retry-completed`
   - `:queue-retry-all`
33. Actionable-ID panel now includes command templates for:
   - `queue stop all`
   - `queue retry failed`
   - `queue retry completed`
   - `queue retry all`.
34. Added queue hygiene command:
   - `queue prune [N]` keeps all active tasks and prunes old terminal tasks, default keeping latest 200 terminal entries.
35. Added retry aliases/scopes:
   - `queue retry completed`
   - `queue retry all`
   - `queue retry cancelled` (same scope as failed/cancelled retry bucket).
36. TUI queue controls extended:
   - `:queue-prune [N]`
   - `:queue-retry-completed`
   - `:queue-retry-all`

## 3. Working Tree Changes (Uncommitted)

Modified files:

1. `hypr-claw-app/src/main.rs`
2. `hypr-claw-app/src/tui.rs`
3. `hypr-claw-runtime/src/llm_client.rs`
4. `hypr-claw-tools/src/os_capabilities/desktop.rs`
5. `workleft.md`

These changes were compiled and tested successfully in this state.

## 4. Validation Already Run

Executed successfully:

1. `cargo fmt --all`
2. `cargo check -p hypr_claw_tools -p hypr-claw-app -p hypr-claw-runtime`
3. `cargo test -p hypr_claw_tools --lib -- --nocapture`
4. `cargo test -p hypr-claw-app --bin hypr-claw -- --nocapture`
5. `cargo test -p hypr-claw-runtime --lib -- --nocapture`
6. `cargo build --release`
7. Smoke run:
`printf "capabilities\nexit\n" | ./target/release/hypr-claw`
8. Incremental check after queue-resource update:
`cargo check -p hypr-claw-app`
9. Incremental tests after queue reconciliation update:
`cargo test -p hypr-claw-app --bin hypr-claw -- --nocapture`
10. TUI shortcut tests:
included under `cargo test -p hypr-claw-app -- --nocapture` (`tui::tests::tui_shortcuts_map_to_repl_commands`)
11. Pagination helper coverage:
`reliability_policy_tests::tail_window_slice_paginates_from_latest`
12. Live mode + paging compiled/tested under:
`cargo test -p hypr-claw-app -- --nocapture`
13. Task-log helper coverage:
`reliability_policy_tests::task_log_lines_include_background_and_supervisor_entries`
14. Post-scan-review and supervisor-event updates compiled and validated with:
`TMPDIR=/home/rick/hypr-claw/.tmp cargo check -p hypr-claw-app`
and
`TMPDIR=/home/rick/hypr-claw/.tmp cargo test -p hypr-claw-app -- --nocapture`
15. Resource-aware scheduling updates validated with:
`TMPDIR=/home/rick/hypr-claw/.tmp cargo test -p hypr-claw-app -- --nocapture` (22 tests passed)
16. Background supervisor lane update validated with:
`TMPDIR=/home/rick/hypr-claw/.tmp cargo check -p hypr-claw-app`
and
`TMPDIR=/home/rick/hypr-claw/.tmp cargo test -p hypr-claw-app -- --nocapture` (22 tests passed)
17. Queue stop/retry + background linkage persistence updates validated with:
`cargo fmt --all`
and
`TMPDIR=/home/rick/hypr-claw/.tmp cargo test -p hypr-claw-app -- --nocapture` (23 tests passed)
18. Queue status + actionable ID panel updates validated with:
`cargo fmt --all`
`TMPDIR=/home/rick/hypr-claw/.tmp cargo check -p hypr-claw-app`
`TMPDIR=/home/rick/hypr-claw/.tmp cargo test -p hypr-claw-app -- --nocapture` (23 tests passed)
19. Bulk queue control + cancel-state hardening validated with:
`cargo fmt --all`
`TMPDIR=/home/rick/hypr-claw/.tmp cargo check -p hypr-claw-app`
`TMPDIR=/home/rick/hypr-claw/.tmp cargo test -p hypr-claw-app -- --nocapture` (23 tests passed)
20. Retry-scope and TUI template extension validated with:
`cargo fmt --all`
`TMPDIR=/home/rick/hypr-claw/.tmp cargo check -p hypr-claw-app`
`TMPDIR=/home/rick/hypr-claw/.tmp cargo test -p hypr-claw-app -- --nocapture` (23 tests passed)
21. Queue prune and extended retry scope updates validated with:
`cargo fmt --all`
`TMPDIR=/home/rick/hypr-claw/.tmp cargo check -p hypr-claw-app`
`TMPDIR=/home/rick/hypr-claw/.tmp cargo test -p hypr-claw-app -- --nocapture` (24 tests passed)
`TMPDIR=/home/rick/hypr-claw/.tmp cargo build --release`

Known warning class left:

1. Unused constants/functions in Codex provider module (`crates/providers/src/codex/*`). *(Fixed: dead_code allow + drop guard.)*

**Rest work completed:** E2E smoke test (`tests/e2e_workflow_test.rs`, run with `--ignored`), REPL/queue unit test (`start_next_returns_empty_when_no_queued_tasks`), Production testing checklist (`docs/PRODUCTION_TESTING.md`).

## 5. What Is Left (Production Testing Target)

Estimated major blocks remaining: 4-5

1. TUI v2 live UX
2. Queue scheduler conflict handling and parallel-task policy (background parallel lane implemented for non-exclusive resource tasks; desktop/filesystem-exclusive tasks remain foreground/serialized)
3. Capability learning engine v2 (scan/diff/review/merge lifecycle)
4. Robust desktop workflow loops (mail/chat summarization and action retries)
5. Trust/approval hardening (policy boundaries for full-auto)
6. Provider router improvements (model fallback and quota-aware routing)
7. E2E and regression harness for real user workflows

## 6. Immediate Next Coding Queue (Recommended Order)

### Q1. TUI v2 live mode (highest user-visible impact)

Deliver:

1. Non-blocking render loop with periodic refresh is partially implemented via TUI live mode (`/tui live on`).
2. Stable panel widths/heights with small-terminal fallback.
3. Interrupt key handling inside TUI (`Ctrl+C`, `/interrupt`, queue stop controls) is partially improved via `:interrupt` shortcut + real interrupt signal.
4. Expand decision feed and task logs with paging/scroll (feed/messages/task-log done; remaining work is richer per-task output detail source).

## Build Note (Sandbox)

In constrained sandboxes, Rust temp writes may hit `/tmp` quota. Workaround:

1. `mkdir -p .tmp`
2. run cargo commands with `TMPDIR=/home/rick/hypr-claw/.tmp`

Acceptance:

1. TUI does not break alignment across common terminal sizes.
2. User can interrupt a running request from TUI without session corruption.
3. Queue and active run state are always visible.

### Q2. Queue conflict controller

Deliver:

1. Task resource tagging (`desktop_input`, `filesystem`, `network`, `compute`, `general`) is implemented.
2. User conflict policy prompt is implemented for desktop-exclusive conflicts (`queue` / `run now` / `cancel running bg tasks and run`).
3. Resource-level conflict policy is now implemented in queue start + running background conflict checks.
4. Add true parallel non-conflicting supervisor execution (current loop remains single foreground run; scheduler is now conflict-aware but still serial for active agent runs).

Acceptance:

1. Conflicting tasks do not race on desktop input.
2. Non-conflicting tasks can run in parallel where safe (still pending).

### Q3. Capability registry refresh UX

Deliver:

1. `scan` now produces a readable diff summary before persisting.
2. User can now accept/edit/reject discovered capabilities in scan flow.
3. Persist approved capability deltas with timestamp (partial: current implementation persists full approved registry snapshot; delta history still pending).

Acceptance:

1. Registry changes are explicit and inspectable.
2. Prompt context reflects approved capabilities only.

## 7. Important Technical Constraints

1. Keep one Tokio runtime only.
2. Keep tool-only action enforcement.
3. Keep prompt-first autonomy as default.
4. Do not reintroduce generic unrestricted shell fallback in default souls.
5. Preserve context, audit, and permission engine behavior.

## 8. Files To Read First Before Continuing

1. `hypr-claw-app/src/main.rs`
2. `hypr-claw-app/src/tui.rs`
3. `hypr-claw-runtime/src/llm_client.rs`
4. `hypr-claw-tools/src/os_capabilities/desktop.rs`
5. `docs/ROADMAP.md`

## 9. Quick Start For Next Agent

1. Open repo and run:
`cargo fmt --all && cargo check -p hypr_claw_tools -p hypr-claw-app -p hypr-claw-runtime`
2. Run tests:
`cargo test -p hypr_claw_tools --lib && cargo test -p hypr-claw-app --bin hypr-claw && cargo test -p hypr-claw-runtime --lib`
3. Start binary:
`./target/release/hypr-claw`
4. Test key prompts:
`open vscode`, `open telegram`, `open gmail and summarize`, `capabilities`, `/tui`

## 10. Definition of Done For “Production Testing Candidate”

Candidate is ready when:

1. TUI is stable and interruptible.
2. App launch reliability is high for common apps on Arch.
3. Queue conflict handling is deterministic and user-visible.
4. Rate-limit behavior degrades gracefully.
5. End-to-end workflow tests pass for desktop tasks.
6. Manual smoke test passes for at least one full session with mixed tasks.
