# Security Model

## Principles

- Structured tools only
- Strict input schema validation
- Permission tier enforcement
- Audit logging for actions
- Explicit approval for critical operations

## Permission Tiers

- `Read`: data retrieval actions
- `Write`: file and state mutations
- `Execute`: process and desktop actions
- `SystemCritical`: high-impact actions (for example shutdown/reboot)

## Approval Flow

For `SystemCritical` tools, execution pauses for explicit user approval in REPL.
Denied approvals are logged and action is not executed.

## Input Validation

OS capability modules validate:

- workspace IDs and window selectors
- command string control characters
- URL format
- key/mouse token formats
- coordinate bounds

## Audit Trail

Tool dispatch records actions to the audit logger, including denied and failed calls.
This provides traceability for sensitive operations.
