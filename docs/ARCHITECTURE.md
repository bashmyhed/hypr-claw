# Architecture

## Overview

Hypr-Claw is a **production-grade local autonomous AI operating layer** designed for persistent, stateful agent execution with memory, planning, policy enforcement, and OS-level control.

This is **not** a chatbot or CLI wrapper. It's an agent runtime designed for:
- Multi-step autonomous task execution
- Persistent memory across sessions
- Environment-aware decision making
- Secure OS-level capability execution
- Multiple interface adapters (terminal, widget, Telegram)

## Layered Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Interfaces                               │
│  Terminal │ Widget (future) │ Telegram (future)                 │
└─────────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────────┐
│                           Core                                   │
│  - AgentEngine: Soul-agnostic execution engine                  │
│  - PlanningLoop: Multi-step task planning                       │
│  - Circuit breaker, concurrency control                          │
└─────────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        │                     │                     │
┌───────▼────────┐   ┌────────▼────────┐   ┌───────▼────────┐
│    Memory      │   │     Policy      │   │   Providers    │
│                │   │                 │   │                │
│ - Context      │   │ - Souls         │   │ - OpenAI       │
│ - Compaction   │   │ - Permissions   │   │ - NVIDIA       │
│ - Persistence  │   │ - Risk tiers    │   │ - Google       │
└────────────────┘   └─────────────────┘   └────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        │                     │                     │
┌───────▼────────┐   ┌────────▼────────┐
│     Tools      │   │    Executor     │
│                │   │                 │
│ - File ops     │   │ - Environment   │
│ - System tools │   │ - Commands      │
│ - Hyprland     │   │ - Whitelist     │
└────────────────┘   └─────────────────┘
```

## Core Principles

### 1. Soul-Agnostic Engine

The core engine has **zero knowledge** of soul logic. Souls are configuration files that define:
- System prompt
- Allowed tools
- Autonomy mode (auto/confirm)
- Max iterations
- Risk tolerance
- Verbosity level

The engine receives an `AgentContext` containing the soul config, but never interprets it directly.

### 2. Persistent Memory

Every session maintains:
- **Recent history**: Last N messages with token tracking
- **Long-term summary**: Compacted older conversations
- **Facts**: Learned information about the system
- **Active tasks**: Background task states
- **Tool stats**: Usage patterns and failures
- **Environment snapshot**: Last known system state

Memory is automatically compacted when:
- History exceeds 50 entries
- Total tokens exceed 100,000
- Completed tasks are older than 24 hours

### 3. Environment Awareness

Before every LLM call, the system captures:
- Current workspace
- Running processes (top 20)
- Memory usage
- Disk usage
- Battery level (if available)
- System uptime

This snapshot is injected into the system prompt as structured context.

### 4. Multi-Step Planning

The agent loop:
1. Load persistent context
2. Capture environment snapshot
3. Generate LLM response with tools
4. Execute tool calls
5. Update memory
6. Check completion
7. Repeat until goal achieved or max iterations

No single-pass execution. The agent **plans and iterates**.

### 5. Structured Tools

Tools are **categorized and validated**:
- `SystemTools`: echo, info
- `FileTools`: read, write (sandboxed)
- `HyprlandTools`: window management (future)
- `WallpaperTools`: wallpaper control (future)
- `ProcessTools`: process management (future)

Each tool:
- Has strict JSON schema
- Returns structured JSON output
- Never allows arbitrary shell execution
- Validates all arguments

### 6. Permission System

Four-tier permission model:
- **Read**: Safe read operations
- **Write**: File modifications
- **Execute**: Command execution
- **SystemCritical**: Requires approval

Blocked patterns:
- `rm -rf`
- `dd if=`
- `mkfs`, `format`
- `shutdown`, `reboot`
- Fork bombs

### 7. Interface Abstraction

The `Interface` trait decouples the engine from I/O:

```rust
trait Interface {
    async fn receive_input() -> Option<String>;
    async fn send_output(message: &str);
    async fn request_approval(action: &str) -> bool;
    async fn show_status(status: &str);
}
```

Implementations:
- `TerminalInterface`: Current (stdin/stdout)
- `WidgetInterface`: Future (GTK/Qt)
- `TelegramInterface`: Future (bot API)

## Data Flow

### Execution Flow

```
User Input
    │
    ▼
Interface.receive_input()
    │
    ▼
ContextManager.load(session_id)
    │
    ▼
EnvironmentSnapshot.capture()
    │
    ▼
AgentEngine.execute_task()
    │
    ├─► LLMProvider.generate()
    │       │
    │       ▼
    │   Tool calls?
    │       │
    │       ├─► Yes: ToolExecutor.execute()
    │       │       │
    │       │       ▼
    │       │   PermissionEngine.check()
    │       │       │
    │       │       ▼
    │       │   Tool.execute()
    │       │       │
    │       │       ▼
    │       │   Update memory
    │       │       │
    │       │       └─► Continue loop
    │       │
    │       └─► No: Check completion
    │               │
    │               ├─► Complete: Return result
    │               └─► Incomplete: Continue loop
    │
    ▼
ContextManager.save()
    │
    ▼
Interface.send_output()
```

### Memory Compaction Flow

```
History > 50 entries?
    │
    ├─► Yes: Summarize oldest 20
    │       │
    │       ▼
    │   Append to long_term_summary
    │       │
    │       ▼
    │   Remove from recent_history
    │
    └─► No: Check token count
            │
            ├─► > 100k tokens?
            │       │
            │       ▼
            │   Summarize half
            │
            └─► No: Continue
```

## Crate Responsibilities

### `hypr-claw-core`
- Agent execution engine
- Planning loop (Phase 4)
- Circuit breaker
- Concurrency control
- **No I/O, no soul logic, no tool implementation**

### `hypr-claw-memory`
- Context persistence (JSON files)
- Automatic compaction
- Token tracking
- Fact deduplication
- Task pruning

### `hypr-claw-policy`
- Soul configuration (YAML)
- Permission engine
- Risk tier evaluation
- Blocked pattern matching

### `hypr-claw-executor`
- Environment snapshot capture
- Command execution with whitelist
- System information gathering

### `hypr-claw-tools`
- Tool trait definition
- Tool registry
- Structured tool implementations
- Sandboxed file operations

### `hypr-claw-providers`
- LLM provider trait
- OpenAI-compatible implementation
- Supports NVIDIA, Google, local models

### `hypr-claw-interfaces`
- Interface trait
- Terminal implementation
- Future: Widget, Telegram

### `hypr-claw-app`
- Composition root
- System initialization
- Wiring all components

## Security Model

### Sandboxing
- All file operations restricted to `./sandbox/`
- Path traversal prevention
- Symlink escape prevention

### Whitelisting
- Only approved commands allowed
- No arbitrary shell execution
- Command arguments validated

### Permission Tiers
- Read operations: Auto-approved
- Write operations: Auto-approved (within sandbox)
- Execute operations: Whitelist-checked
- System-critical: Requires user approval

### Audit Trail
- All tool executions logged
- Hash-chained for tamper detection
- Append-only log file

## Configuration

### Soul Example

`./souls/safe_assistant.yaml`:

```yaml
id: safe_assistant
system_prompt: |
  You are a helpful assistant with limited system access.
  You can read and write files in the sandbox directory.
  Always explain what you're doing.
config:
  allowed_tools:
    - echo
    - file_read
    - file_write
  autonomy_mode: confirm
  max_iterations: 10
  risk_tolerance: low
  verbosity: normal
```

### Directory Structure

```
./data/
  ├── context/           # Persistent memory
  │   └── <session_id>.json
  ├── agents/            # Legacy (will migrate to souls/)
  ├── sessions/          # Legacy session logs
  └── credentials/       # Encrypted credentials

./souls/                 # Soul configurations
  ├── safe_assistant.yaml
  ├── system_admin.yaml
  └── automation_agent.yaml

./sandbox/               # Sandboxed file operations
```

## Future Phases

### Phase 4: Multi-Step Planning Engine
- Explicit plan generation
- Step tracking
- Plan revision
- Goal decomposition

### Phase 7: Background Task Manager
- Async task spawning
- Progress tracking
- Task cancellation
- Status reporting

### Phase 8: Widget Interface
- GTK/Qt UI
- Visual task progress
- Approval dialogs
- System tray integration

### Phase 9: Telegram Interface
- Bot API integration
- Multi-user support
- Remote task execution
- Status notifications

## Design Decisions

### Why not database?
- Simplicity: JSON files are human-readable
- Portability: No database setup required
- Debugging: Easy to inspect and modify
- Single-node: No distributed requirements yet

### Why not REPL?
- Task-oriented: One task per execution
- Stateful: Context persists across runs
- Production-ready: Designed for automation

### Why trait-based?
- Testability: Easy to mock components
- Flexibility: Swap implementations
- Separation: Clear boundaries between layers

### Why async?
- Concurrency: Multiple sessions in parallel
- I/O efficiency: Non-blocking file/network ops
- Future-proof: Ready for distributed execution

## Testing Strategy

- **Unit tests**: Each crate independently
- **Integration tests**: Cross-crate interactions
- **Stress tests**: 1000+ concurrent sessions
- **Failure simulation**: Network errors, disk full, etc.
- **Security tests**: Path traversal, command injection

## Observability

### Metrics (Phase 9)
- `llm_request_latency`: LLM call duration
- `tool_execution_latency`: Tool execution time
- `session_duration`: Total session time
- `compaction_count`: Memory compactions
- `permission_denials`: Blocked operations

### Logging
- Structured logging with `tracing`
- Log levels: ERROR, WARN, INFO, DEBUG, TRACE
- Context propagation across async boundaries

## Migration Path

Current system → New architecture:

1. **Phase 1-3** (Complete): New crates, memory, environment
2. **Phase 4-6**: Planning, tools, souls
3. **Phase 7-8**: Tasks, interfaces
4. **Phase 9-11**: Hardening, security, docs
5. **Phase 12**: Validation, widget integration

Legacy crates remain functional during migration.
