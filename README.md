# Hypr-Claw

A production-grade multi-crate Rust system for agent runtime execution with tool dispatch, sandboxing, and infrastructure management.

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
- **LLM integration** - HTTP client with retry logic
- **Message compaction** - Token-aware history management
- **Session isolation** - Per-session locking and state

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
# Clone repository
git clone <repo-url>
cd hypr-claw

# Build workspace
cargo build --release

# Run tests
cargo test

# Run clippy
cargo clippy --all-targets
```

### Usage Example

```rust
use std::sync::Arc;
use std::time::Duration;
use hypr_claw::infra::{SessionStore, LockManager, PermissionEngine, AuditLogger};
use hypr_claw_tools::{ToolRegistryImpl, ToolDispatcherImpl, tools::EchoTool};
use hypr_claw_runtime::{AsyncSessionStore, AsyncLockManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize infrastructure
    let session_store = Arc::new(SessionStore::new("./sessions")?);
    let lock_manager = Arc::new(LockManager::new(Duration::from_secs(30)));
    let permission_engine = Arc::new(PermissionEngine::new());
    let audit_logger = Arc::new(AuditLogger::new("./audit.log")?);

    // Create async adapters
    let async_session = Arc::new(AsyncSessionStore::new(session_store));
    let async_locks = Arc::new(AsyncLockManager::new(lock_manager));

    // Setup tools
    let mut registry = ToolRegistryImpl::new();
    registry.register(Arc::new(EchoTool));

    let dispatcher = Arc::new(ToolDispatcherImpl::new(
        Arc::new(registry),
        permission_engine as Arc<dyn hypr_claw_tools::PermissionEngine>,
        audit_logger as Arc<dyn hypr_claw_tools::AuditLogger>,
        5000,
    ));

    // Use the system
    // (RuntimeController setup would go here)

    Ok(())
}
```

## Project Structure

```
hypr-claw/
├── hypr-claw-infra/           # Infrastructure layer (sync)
│   └── src/infra/
│       ├── session_store.rs   # File-based session persistence
│       ├── lock_manager.rs    # Session locking with timeout
│       ├── permission_engine.rs # Permission checking
│       ├── audit_logger.rs    # Audit logging
│       ├── credential_store.rs # Encrypted credentials
│       └── rate_limiter.rs    # Rate limiting
│
├── hypr-claw-tools/           # Tool execution layer (async)
│   └── src/
│       ├── dispatcher.rs      # Tool dispatch with protection
│       ├── registry.rs        # Tool registry
│       ├── sandbox/           # Sandboxing components
│       │   ├── command_guard.rs # Command validation
│       │   └── path_guard.rs  # Path validation
│       └── tools/             # Built-in tools
│           ├── echo.rs
│           ├── file_read.rs
│           ├── file_write.rs
│           ├── file_list.rs
│           └── shell_exec.rs
│
├── hypr-claw-agents/
│   └── hypr-claw-runtime/     # Runtime core (async)
│       └── src/
│           ├── agent_loop.rs  # Core agent execution
│           ├── llm_client.rs  # LLM HTTP client
│           ├── compactor.rs   # Message compaction
│           ├── interfaces.rs  # Runtime traits
│           └── async_adapters.rs # Sync-to-async adapters
│
└── hypr-claw-app/             # Composition root
    └── src/lib.rs             # Application wiring
```

## Key Design Decisions

### Async Architecture
- Runtime and tools layers are fully async
- Infrastructure layer is sync (blocking I/O)
- Async adapters bridge the gap using `tokio::task::spawn_blocking`

### Trait-Based Integration
- Traits defined in tools layer (`PermissionEngine`, `AuditLogger`)
- Traits defined in runtime layer (`SessionStore`, `LockManager`)
- Infrastructure implements traits via adapter pattern
- Enables clean dependency injection and testing

### Security
- **Sandboxing**: All file and command operations validated
- **Whitelisting**: Only approved commands allowed
- **Path validation**: Prevents traversal and symlink escapes
- **Encryption**: Credentials encrypted at rest (AES-256-GCM)
- **Audit trail**: Tamper-evident hash chain logging

### Reliability
- **Lock timeouts**: Prevents deadlocks
- **Panic isolation**: Tool failures don't crash runtime
- **Atomic writes**: Session data written atomically
- **Retry logic**: LLM calls retry on failure
- **Graceful degradation**: Audit failures don't block execution

## Testing

```bash
# Run all tests
cargo test

# Run specific crate tests
cargo test -p hypr_claw
cargo test -p hypr_claw_tools
cargo test -p hypr-claw-runtime

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_name
```

## Development

### Code Quality

```bash
# Format code
cargo fmt

# Run clippy
cargo clippy --all-targets

# Strict clippy (zero warnings)
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
    fn name(&self) -> &'static str {
        "my_tool"
    }

    fn description(&self) -> &'static str {
        "Description of my tool"
    }

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
        // Implementation
        Ok(ToolResult {
            success: true,
            output: Some(json!({"result": "value"})),
            error: None,
        })
    }
}
```

2. Register the tool:

```rust
registry.register(Arc::new(MyTool));
```

## Performance Characteristics

- **Lock contention**: Per-session locks minimize contention
- **I/O**: Async I/O for tools, sync I/O wrapped in `spawn_blocking` for infrastructure
- **Memory**: Message compaction prevents unbounded growth
- **Concurrency**: Multiple sessions execute in parallel

## Limitations

- Single-node deployment (no distributed locking yet)
- File-based session storage (no database backend)
- In-memory rate limiting (resets on restart)
- No built-in observability (metrics/tracing setup required)

## License

See LICENSE file.

## Contributing

See CONTRIBUTING.md for guidelines.
