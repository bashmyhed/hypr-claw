# Agent Loop

## Purpose

The agent loop runs a deterministic cycle for each user request in the REPL.

## High-Level Flow

1. Persist user message into context/thread history.
2. Build or update a lightweight plan for the request.
3. Call provider with current system prompt and active tool schemas.
4. Execute returned tool calls via dispatcher.
5. Persist assistant response or structured error.
6. Update plan status (`completed` or `failed`).

## Tool-Only Enforcement

If user intent implies system action and the model returns explanation text without tool calls, runtime returns an error:

- `Tool invocation required but not performed`

This blocks silent non-action responses for actionable requests.

## Recovery Logic

When runtime errors occur, the app can:

- attempt auto soul switch for better tool fit
- retry with stricter prompt for tool execution
- narrow tool subset for provider argument-limit failures

## Completion

On success:

- assistant output is written to context history
- plan is marked complete
- active task snapshot is synced into context

On failure:

- plan is marked failed with error text
- error is shown in REPL
