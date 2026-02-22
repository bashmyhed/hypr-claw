# Hypr-Claw

Production-grade local autonomous AI operating layer for Linux with persistent memory, multi-step planning, and secure tool execution.

## Overview

Hypr-Claw is an agent runtime system designed for autonomous task execution with:

- Persistent memory across sessions
- Multi-step planning with progress tracking
- Environment-aware decision making
- Secure sandboxed tool execution
- Background task management
- Multiple LLM provider support (NVIDIA, Google, local models)

## Quick Start

### Prerequisites

- Rust 1.75+ (2021 edition)
- Linux (tested on Ubuntu/Arch)
- LLM provider (NVIDIA NIM, Google Gemini, or local model)

### Installation

```bash
# Clone repository
git clone https://github.com/yourusername/hypr-claw.git
cd hypr-claw

# Build release binary
cargo build --release

# Binary location
./target/release/hypr-claw
```

### First Run

```bash
./target/release/hypr-claw
```

You will be prompted for:
1. LLM base URL (e.g., `http://localhost:8080`)
2. Agent name (default: `default`)
3. User ID (default: `local_user`)
4. Task description

The system automatically creates:
- `./data/context/` - Persistent memory
- `./data/agents/` - Agent configurations
- `./sandbox/` - Sandboxed file operations
- `./souls/` - Soul profiles

## Architecture

### Core Components

```
crates/
├── core/          Agent engine with planning and metrics
├── memory/        Persistent context with automatic compaction
├── policy/        Soul configurations and permission system
├── executor/      Environment snapshot and command execution
├── tools/         Structured tool system (sandboxed)
├── providers/     LLM provider abstraction
├── interfaces/    Interface abstraction (terminal/widget/telegram)
└── tasks/         Background task manager
```

### Key Features

**Persistent Memory**
- Context survives restarts
- Automatic compaction (history > 50 entries or tokens > 100k)
- Token tracking and accounting
- Fact deduplication

**Multi-Step Planning**
- Iterative task execution
- Progress tracking (0-100%)
- Step-by-step validation
- Plan revision support

**Security**
- 4-tier permission system (Read/Write/Execute/SystemCritical)
- Rate limiting per tool and session
- Sandboxed file operations
- Command whitelist enforcement
- Blocked dangerous patterns

**Observability**
- Lock-free metrics collection
- Success rate tracking
- Compaction monitoring
- Comprehensive logging

## Usage

### Basic Task Execution

```bash
./target/release/hypr-claw
# Enter task: "Create a file named test.txt with hello world"
```

### Using Different Souls

Souls define agent behavior and permissions:

```bash
# Safe assistant (limited permissions, requires confirmation)
./target/release/hypr-claw --soul safe_assistant

# System admin (elevated privileges, verbose)
./target/release/hypr-claw --soul system_admin

# Automation agent (autonomous, minimal verbosity)
./target/release/hypr-claw --soul automation_agent

# Research agent (read-only focus)
./target/release/hypr-claw --soul research_agent
```

### Soul Configuration

Create custom soul in `./souls/mysoul.yaml`:

```yaml
id: mysoul
system_prompt: |
  You are a specialized assistant for data processing.
  Be efficient and precise.

config:
  allowed_tools:
    - echo
    - file_read
    - file_write
    - file_list
  autonomy_mode: confirm  # or 'auto'
  max_iterations: 20
  risk_tolerance: low     # low/medium/high
  verbosity: normal       # minimal/normal/verbose
```

### Available Tools

**System Tools**
- `echo` - Echo messages
- `system_info` - Get OS/architecture information

**File Tools** (sandboxed to `./sandbox/`)
- `file_read` - Read files
- `file_write` - Write files
- `file_list` - List directory contents

**Process Tools** (read-only)
- `process_list` - List running processes

## Configuration

### LLM Providers

**NVIDIA NIM**
```bash
# Base URL: https://integrate.api.nvidia.com/v1
# API Key: Required (set in config)
```

**Google Gemini**
```bash
# Base URL: https://generativelanguage.googleapis.com/v1beta
# API Key: Required (set in config)
```

**Local Models** (Ollama, LM Studio, etc.)
```bash
# Base URL: http://localhost:8080
# API Key: Not required
```

### Environment Variables

```bash
export RUST_LOG=info              # Logging level
export HYPR_CLAW_DATA_DIR=./data  # Data directory
```

## Development

### Build

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Check all crates
cargo check --workspace
```

### Testing

```bash
# Run all tests
cargo test --workspace

# Run specific crate tests
cargo test --package hypr-claw-core
cargo test --package hypr-claw-memory
cargo test --package hypr-claw-policy
```

### Code Quality

```bash
# Format code
cargo fmt

# Run clippy
cargo clippy --all-targets -- -D warnings
```

## Documentation

Comprehensive documentation available in `./docs/`:

- `ARCHITECTURE.md` - System design and data flow
- `AGENT_LOOP.md` - Execution flow and iteration logic
- `MEMORY_SYSTEM.md` - Context management and compaction
- `SECURITY_MODEL.md` - Multi-layer security architecture
- `ROADMAP.md` - Project roadmap and future phases

## Security

### Multi-Layer Protection

1. **Permission Engine** - Blocks dangerous patterns (rm -rf, dd, mkfs, etc.)
2. **Rate Limiter** - Prevents abuse with time-window based limits
3. **Sandbox** - File operations restricted to `./sandbox/`
4. **Whitelist** - Only approved commands allowed
5. **Approval Flow** - Critical operations require user confirmation

### Blocked Patterns

- `rm -rf` - Recursive delete
- `dd if=` - Disk operations
- `mkfs`, `format` - Filesystem operations
- `shutdown`, `reboot` - System control
- Fork bombs and similar attacks

## Performance

- Async/await throughout for non-blocking I/O
- Lock-free metrics collection
- Efficient context compaction
- Per-session locking minimizes contention
- Multiple sessions run in parallel

## Limitations

- Single-node deployment (no distributed locking yet)
- File-based session storage (no database backend)
- In-memory rate limiting (resets on restart)
- Linux-only (tested on Ubuntu/Arch)

## Future Plans

### Phase 13: Widget Interface
- GTK/Qt UI for visual interaction
- System tray integration
- Visual task progress
- Approval dialogs

### Phase 14: Telegram Bot
- Remote agent control
- Multi-user support
- Status notifications
- Secure authentication

### Phase 15: Distributed Architecture
- Multi-device coordination
- Distributed locking (etcd/Consul)
- Shared context store
- Load balancing

### Additional Enhancements
- Hyprland window management tools
- Wallpaper control tools
- Network access tools (with strict controls)
- Plugin system for third-party tools
- Vector embeddings for semantic memory
- LLM-based context summarization

## Contributing

Contributions welcome! Please ensure:

1. All tests pass: `cargo test --workspace`
2. No clippy warnings: `cargo clippy --all-targets -- -D warnings`
3. Code is formatted: `cargo fmt`
4. New features include tests
5. Documentation is updated

### Adding a New Tool

1. Implement the `Tool` trait in `crates/tools/src/`:

```rust
use async_trait::async_trait;
use crate::traits::{Tool, ToolResult, ToolError};

pub struct MyTool;

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str { "my_tool" }
    fn description(&self) -> &str { "Description" }
    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "param": {"type": "string"}
            },
            "required": ["param"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        // Implementation
        Ok(ToolResult {
            success: true,
            output: json!({"result": "value"}),
            error: None,
        })
    }
}
```

2. Register in tool registry
3. Add tests
4. Update documentation

## License

See LICENSE file for details.

## Support

- Issues: GitHub Issues
- Documentation: `./docs/`
- Examples: `./examples/` (coming soon)

## Acknowledgments

Built with Rust, Tokio, and the async ecosystem.

## Project Status

Production-ready. All 12 phases complete:

- Phase 1-3: Foundation (architecture, memory, environment)
- Phase 4-6: Intelligence (planning, tools, souls)
- Phase 7-9: Scale (tasks, integration, observability)
- Phase 10-12: Production (security, documentation, validation)

76 tests passing. Zero warnings. Ready for deployment.
