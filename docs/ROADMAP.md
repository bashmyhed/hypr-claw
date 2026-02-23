# Roadmap

## Near-Term Priorities

1. Improve provider resilience
- add request backoff and quota-aware retries
- add clearer degraded-mode behavior for rate-limit events

2. Strengthen planner depth
- move from lightweight plan template to explicit multi-step planning
- track step-level tool outcomes across retries

3. Background task UX
- improve task lifecycle commands and status reporting
- support richer task output logs in REPL

4. Hyprland desktop reliability
- improve capability checks and clearer dependency diagnostics
- add safer defaults for workspace/window targeting

5. Test coverage expansion
- add end-to-end REPL command tests
- add failure-injection tests for provider and tool-call edge cases

## Mid-Term Goals

1. TUI upgrade with better task/thread navigation
2. Better soul auto-routing accuracy and explainability
3. Widget interface integration on top of existing runtime core
