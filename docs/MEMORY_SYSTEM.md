# Memory System

## Persistent Context

Context is stored in `data/context/<session_key>.json` and includes:

- conversation history
- facts
- active soul id
- active plan
- task summaries
- system state (onboarding profile, thread metadata)

## Onboarding Data

First-run onboarding stores:

- preferred user name
- system profile scan results
- profile confirmation/edit state
- timestamp of last scan

## Task and Thread State

- Task manager persists background task info (`data/tasks/tasks.json`).
- Thread metadata is stored in context system state.
- Thread-specific chat history uses thread session keys.

## Compaction

Runtime compactor is integrated to keep context size bounded.
Compaction summarizes older history when token/size thresholds are reached.

## Shutdown Safety

On graceful shutdown (`Ctrl+C`):

- running task snapshot is persisted
- context is saved before exit
