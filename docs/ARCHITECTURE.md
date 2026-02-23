# Architecture

## Overview

Hypr-Claw is a local OS agent runtime built as a Rust workspace with clear separation between app, runtime, tools, policy, memory, and infrastructure.

## Main Layers

- `hypr-claw-app`: startup, onboarding, REPL, soul/thread command handling
- `hypr-claw-runtime`: agent loop, LLM interaction, tool-call orchestration, compaction hooks
- `hypr-claw-tools`: structured tool interfaces and capability adapters
- `hypr-claw-infra`: permission engine, audit logger, session store, lock manager
- `crates/*`: modular implementations for core interfaces and shared types

## Execution Path

1. App initializes directories and provider configuration.
2. Context is loaded by session key.
3. Soul profile is loaded and tool access is filtered.
4. User input enters REPL loop.
5. Agent loop calls provider with tool schemas.
6. Tool calls pass through permission and audit layers.
7. Results are persisted back to context/session/task state.

## Soul and Tool Filtering

- Soul profile defines `allowed_tools` and `max_iterations`.
- Agent config tool set is intersected with soul tools.
- Runtime registry exposes only the active subset to the model.
- Optional auto-soul routing switches profiles by request intent.

## Threads and Sessions

- Base session key: `<user_id>:<agent_name>`
- Thread session key: `<base>::thread::<thread_id>`
- Each thread keeps independent chat flow while sharing user-level context state.
