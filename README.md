# Hypr-Claw

A production-grade multi-crate Rust system for agent runtime execution with tool dispatch, sandboxing, and infrastructure management.

**Status**: Production-ready terminal agent with comprehensive hardening and testing (352 tests passing).

## Quick Start

```bash
# Build
cargo build --release

# Run
./target/release/hypr-claw

# Follow interactive prompts:
# 1. Enter LLM base URL (e.g., http://localhost:8080)
# 2. Enter agent name (default: default)
# 3. Enter user ID (default: local_user)
# 4. Enter your task
```

## Architecture

Three-layer architecture with clean separation of concerns:

```
┌─────────────────────────────────────────────────────────────────┐
│                     hypr-claw-runtime                            │
│                     (Runtime Core)                               │
│  - Agent execution loop                                          │
│  - LLM client integration                                        │
│  - Message compaction                                            │
│  - Session management                                            │
│  - Circuit breaker                                               │
│  - Concurrency control                                           │
│  - Metrics instrumentation                                       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                     hypr_claw_tools                              │
│                     (Tool Execution Layer)                       │
│  - Tool dispatcher with permission checks                        │
│  - Sandboxed tool execution                                      │
│  - Command and path validation                                   │
│  - Built-in tools (file ops, shell exec)                         │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                     hypr_claw                                    │
│                     (Infrastructure Layer)                       │
│  - Session persistence (file-based)                              │
│  - Lock management (per-session)                                 │
│  - Permission engine                                             │
│  - Audit logging (chained)                                       │
│  - Credential storage (encrypted)                                │
│  - Rate limiting                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Features

### Runtime Core
- **Async-first design** - Full async/await support with tokio
- **Agent loop** - Deterministic execution with max iteration limits
- **LLM integration** - HTTP client with retry logic and circuit breaker
- **Message compaction** - Token-aware history management
- **Session isolation** - Per-session locking and state
- **Concurrency control** - Semaphore-based session limiting (default: 100)
- **Circuit breaker** - Prevents cascading LLM failures (5 failure threshold, 30s cooldown)
- **Metrics** - Comprehensive observability with optional Prometheus exporter
- **Schema versioning** - Protocol safety with version validation

### Tool Execution
- **Sandboxed execution** - Path and command validation
- **Permission system** - Trait-based permission checks
- **Audit logging** - Fire-and-forget async logging
- **Timeout protection** - Per-tool execution timeouts
- **Panic isolation** - Tools run in isolated tasks

### Infrastructure
- **Session store** - JSONL-based persistence with atomic writes
- **Lock manager** - Timeout-based session locking with RAII
- **Permission engine** - Pattern-based blocking
- **Audit logger** - Append-only with hash chain integrity
- **Credential store** - AES-256-GCM encrypted storage
- **Rate limiter** - Token bucket per-session/per-tool/global

## Quick Start

### Prerequisites

- Rust 1.75+ (2021 edition)
- Tokio runtime

### Build

```bash
# Build workspace
cargo build --release

# Run tests
cargo test

# Run clippy
cargo clippy --all-targets -- -D warnings
```

### Run

```bash
# Run the application
./target/release/hypr-claw
```

**Interactive Prompts**:
1. **LLM base URL**: Your LLM service endpoint (e.g., `http://localhost:8080`)
2. **Agent name**: Agent to use (press Enter for `default`)
3. **User ID**: Your user identifier (press Enter for `local_user`)
4. **Task**: What you want the agent to do

**Example Output**:
```
╔══════════════════════════════════════════════════════════════════╗
║              Hypr-Claw Terminal Agent                            ║
╚══════════════════════════════════════════════════════════════════╝

Enter LLM base URL: http://localhost:8080
Enter agent name [default]: 
Enter user ID [local_user]: 
Enter task: echo hello world

🔧 Initializing system...
✅ System initialized

🤖 Executing task...

╔══════════════════════════════════════════════════════════════════╗
║                         Response                                 ║
╚══════════════════════════════════════════════════════════════════╝
Hello world

✅ Task completed successfully
```

### First Run Setup

On first run, the application automatically creates:
- `./data/sessions/` - Session history
- `./data/credentials/` - Encrypted credentials
- `./data/agents/` - Agent configurations
- `./data/audit.log` - Audit trail
- `./sandbox/` - Sandboxed file operations
- Default agent config (`./data/agents/default.yaml`)

## Project Structure

```
hypr-claw/
├── hypr-claw-app/             # Binary entrypoint
│   └── src/main.rs            # System wiring and CLI
│
├── hypr-claw-runtime/         # Runtime core (async)
│   └── src/
│       ├── agent_loop.rs      # Core agent execution
│       ├── llm_client.rs      # LLM HTTP client with circuit breaker
│       ├── compactor.rs       # Message compaction
│       ├── runtime_controller.rs  # Main entry point with concurrency control
│       ├── metrics.rs         # Observability metrics
│       └── types.rs           # Core types with schema versioning
│
├── hypr-claw-tools/           # Tool execution layer (async)
│   └── src/
│       ├── dispatcher.rs      # Tool dispatch with protection
│       ├── registry.rs        # Tool registry
│       ├── sandbox/           # Sandboxing components
│       └── tools/             # Built-in tools
│
└── hypr-claw-infra/           # Infrastructure layer (sync)
    └── src/infra/
        ├── session_store.rs   # File-based session persistence
        ├── lock_manager.rs    # Session locking
        ├── permission_engine.rs  # Permission checking
        ├── audit_logger.rs    # Audit logging
        └── credential_store.rs   # Encrypted credentials
```

## Production Hardening

The runtime includes comprehensive production hardening:

### Concurrency Control
- Semaphore-based session limiting (default: 100 concurrent sessions)
- Prevents resource exhaustion
- Configurable per RuntimeController instance

### Circuit Breaker
- Prevents cascading LLM failures
- Failure threshold: 5 consecutive failures
- Cooldown window: 30 seconds
- Automatic recovery with trial requests

### Metrics & Observability
- `llm_request_latency` - Histogram of LLM call durations
- `tool_execution_latency` - Histogram of tool execution times
- `session_duration` - Histogram of session durations
- `lock_wait_duration` - Histogram of lock wait times
- `compaction_count` - Counter of message compactions
- Optional Prometheus exporter (enable with `prometheus` feature)

### Schema Versioning
- Protocol version: 1
- Backward compatible defaults
- Version validation on Message and LLMResponse
- Clear error messages on version mismatch

### Testing
- 352 tests including unit, integration, fuzz, and stress tests
- Session persistence validation tests
- Failure simulation tests for reliability
- Load tests with 1000 concurrent sessions
- Zero warnings with strict clippy checks

### Terminal Agent Features
- **Professional CLI** - Clean banner and formatted output
- **Interactive prompts** - With sensible defaults
- **Comprehensive error handling** - Helpful tips for recovery
- **Status indicators** - Visual feedback with emojis (🔧, ✅, 🤖, ❌)
- **Automatic initialization** - Creates directories and default configs
- **Session persistence** - Conversations saved across restarts
- **Single execution mode** - One task per run (no REPL)

## Available Tools

1. **echo** - Echo back input
2. **file_read** - Read files from sandbox
3. **file_write** - Write files to sandbox
4. **file_list** - List files in sandbox
5. **shell_exec** - Execute whitelisted shell commands

## Creating Custom Agents

Create agent config: `./data/agents/myagent.yaml`

```yaml
id: myagent
soul: myagent_soul.md
tools:
  - echo
  - file_read
  - file_write
```

Create soul file: `./data/agents/myagent_soul.md`

```markdown
You are a helpful assistant specialized in file operations.
```

## Development

### Code Quality

```bash
# Format code
cargo fmt

# Run clippy (strict)
cargo clippy --all-targets -- -D warnings

# Check without building
cargo check
```

### Adding a New Tool

1. Implement the `Tool` trait:

```rust
use async_trait::async_trait;
use hypr_claw_tools::{Tool, ToolResult, ExecutionContext, ToolError};

pub struct MyTool;

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &'static str { "my_tool" }
    fn description(&self) -> &'static str { "Description" }
    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "param": {"type": "string"}
            }
        })
    }

    async fn execute(
        &self,
        ctx: ExecutionContext,
        input: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        Ok(ToolResult {
            success: true,
            output: Some(json!({"result": "value"})),
            error: None,
        })
    }
}
```

2. Register in `hypr-claw-app/src/main.rs`:

```rust
registry.register(Arc::new(MyTool));
```

## Performance Characteristics

- **Lock contention**: Per-session locks minimize contention
- **I/O**: Async I/O for tools, sync I/O wrapped in `spawn_blocking` for infrastructure
- **Memory**: Message compaction prevents unbounded growth
- **Concurrency**: Multiple sessions execute in parallel (up to configured limit)
- **Circuit breaker**: Minimal overhead (atomic operations only)
- **Metrics**: Fire-and-forget, non-blocking

## Security Features

- **Sandboxing**: All file and command operations validated
- **Whitelisting**: Only approved commands allowed
- **Path validation**: Prevents traversal and symlink escapes
- **Encryption**: Credentials encrypted at rest (AES-256-GCM)
- **Audit trail**: Tamper-evident hash chain logging
- **Circuit breaker**: Prevents DoS via cascading failures
- **Concurrency limits**: Prevents resource exhaustion attacks

## Limitations

- Single-node deployment (no distributed locking)
- File-based session storage (no database backend)
- In-memory rate limiting (resets on restart)
- No built-in observability UI (metrics export only)

## Documentation

- **HARDENING_SUMMARY.md** - Detailed production hardening documentation
- **LICENSE** - Project license

## License

See LICENSE file.

## Contributing

Contributions welcome! Please ensure:
- All tests pass (`cargo test`)
- No clippy warnings (`cargo clippy --all-targets -- -D warnings`)
- Code is formatted (`cargo fmt`)
- New features include tests
