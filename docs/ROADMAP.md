# Hypr-Claw Roadmap

## Vision

Transform Hypr-Claw into a **production-grade local autonomous AI operating layer** for Linux + Hyprland with persistent memory, multi-step planning, policy enforcement, and multiple interface adapters.

**Not a chatbot. Not a CLI wrapper. An agent runtime.**

---

## Phase 1: Layered Architecture ✅ COMPLETE

**Goal**: Restructure project into clean, separated layers.

### Deliverables
- ✅ 7 new crates with single responsibilities
- ✅ Zero cross-layer leakage
- ✅ Clear dependency hierarchy
- ✅ All crates compile independently

### Structure
```
crates/
├── core/          # Agent engine
├── memory/        # Persistent context
├── policy/        # Souls + permissions
├── executor/      # Environment + commands
├── tools/         # Structured tools
├── providers/     # LLM abstraction
└── interfaces/    # Interface abstraction
```

---

## Phase 2: Persistent Context System ✅ COMPLETE

**Goal**: Implement memory that survives restarts.

### Deliverables
- ✅ `ContextManager` with JSON persistence
- ✅ Automatic compaction (history, tokens, facts, tasks)
- ✅ Token tracking
- ✅ Atomic writes
- ✅ Session listing

### Storage
```
./data/context/<session_id>.json
{
  "system_state": {},
  "facts": [],
  "recent_history": [],
  "long_term_summary": "",
  "active_tasks": [],
  "tool_stats": {},
  "last_known_environment": {},
  "token_usage": {}
}
```

---

## Phase 3: Environment Awareness ✅ COMPLETE

**Goal**: Inject system state into every LLM call.

### Deliverables
- ✅ `EnvironmentSnapshot` capture
- ✅ Process list (top 20)
- ✅ Memory usage
- ✅ Disk usage
- ✅ Battery level (Linux)
- ✅ Concise string formatting

### Captured Data
- Workspace path
- Running processes with PIDs
- Memory: used/total MB
- Disk usage percentage
- Battery percentage
- System uptime

---

## Phase 4: Multi-Step Planning Engine ⏳ NEXT

**Goal**: Replace single-pass execution with iterative planning.

### Tasks
- [ ] Implement explicit plan generation
- [ ] Add step tracking
- [ ] Support plan revision
- [ ] Goal decomposition
- [ ] Progress reporting

### Agent Loop
```
1. Load context
2. Capture environment
3. Generate plan
4. Execute step
5. Update memory
6. Check completion
7. Revise plan if needed
8. Repeat until goal achieved
```

### Deliverables
- `PlanningLoop` implementation
- Plan state persistence
- Step validation
- Iteration guards
- Completion detection

---

## Phase 5: Structured Tool Architecture ⏳ NEXT

**Goal**: Remove generic shell_exec, add categorized tools.

### Tool Categories
- [ ] `SystemTools`: echo, info, status
- [ ] `FileTools`: read, write, list, delete (sandboxed)
- [ ] `HyprlandTools`: window management, workspace control
- [ ] `WallpaperTools`: set wallpaper, list themes
- [ ] `NotificationTools`: send notifications
- [ ] `ProcessTools`: list, monitor (read-only)

### Requirements
- Strict JSON schema per tool
- Structured JSON output only
- No free text responses
- Argument validation
- Permission tier per tool

### Security
- Remove `shell_exec` entirely
- Whitelist commands in `CommandExecutor`
- Sandbox all file operations
- Validate all paths

---

## Phase 6: Soul System Integration ⏳ NEXT

**Goal**: Separate soul configs from engine logic.

### Tasks
- [ ] Create `./souls/` directory
- [ ] Migrate from `./data/agents/`
- [ ] Define soul profiles
- [ ] Load souls at runtime
- [ ] Validate soul configs

### Soul Profiles
```yaml
# ./souls/safe_assistant.yaml
id: safe_assistant
system_prompt: |
  You are a helpful assistant with limited system access.
config:
  allowed_tools: [echo, file_read, file_write]
  autonomy_mode: confirm
  max_iterations: 10
  risk_tolerance: low
  verbosity: normal
```

### Profiles to Create
- `safe_assistant.yaml` - Minimal permissions
- `system_admin.yaml` - Full system access
- `automation_agent.yaml` - Background tasks
- `research_agent.yaml` - Read-only analysis

---

## Phase 7: Background Task Manager

**Goal**: Enable async task execution with progress tracking.

### Tasks
- [ ] Implement `TaskManager`
- [ ] Spawn async tasks
- [ ] Track progress
- [ ] Cancel tasks
- [ ] Persist task state
- [ ] Report status

### API
```rust
pub struct TaskManager {
    pub async fn spawn_task(&self, task: Task) -> TaskId;
    pub async fn get_status(&self, id: TaskId) -> TaskStatus;
    pub async fn cancel_task(&self, id: TaskId) -> Result<()>;
    pub async fn list_tasks(&self) -> Vec<TaskInfo>;
}
```

### Storage
```
./data/tasks/<task_id>.json
{
  "id": "task_001",
  "description": "Monitor system resources",
  "status": "Running",
  "progress": 0.5,
  "created_at": 1708654000,
  "updated_at": 1708654321,
  "result": null
}
```

---

## Phase 8: Interface Abstraction

**Goal**: Decouple engine from I/O for widget/Telegram support.

### Tasks
- [ ] Finalize `Interface` trait
- [ ] Update `TerminalInterface`
- [ ] Create `WidgetInterface` stub
- [ ] Create `TelegramInterface` stub
- [ ] Update `AgentEngine` to use trait

### Interface Trait
```rust
#[async_trait]
pub trait Interface: Send + Sync {
    async fn receive_input(&self) -> Option<String>;
    async fn send_output(&self, message: &str);
    async fn request_approval(&self, action: &str) -> bool;
    async fn show_status(&self, status: &str);
}
```

### Implementations
- ✅ `TerminalInterface` - stdin/stdout
- ⏳ `WidgetInterface` - GTK/Qt UI
- ⏳ `TelegramInterface` - Bot API

---

## Phase 9: Observability & Hardening

**Goal**: Production-grade logging, metrics, and error handling.

### Tasks
- [ ] Structured logging with `tracing`
- [ ] Metrics collection
- [ ] Token accounting
- [ ] Error classification
- [ ] Rate limit backoff
- [ ] Crash recovery
- [ ] Corrupted context recovery
- [ ] Safe shutdown handler

### Metrics
- `llm_request_latency` - Histogram
- `tool_execution_latency` - Histogram
- `session_duration` - Histogram
- `compaction_count` - Counter
- `permission_denials` - Counter
- `task_spawns` - Counter

### Logging
- ERROR: Critical failures
- WARN: Recoverable issues
- INFO: Key events
- DEBUG: Detailed flow
- TRACE: Full execution trace

---

## Phase 10: Security Model Upgrade

**Goal**: Multi-tier permission system with approval flow.

### Tasks
- [ ] Implement permission tiers
- [ ] Add approval flow
- [ ] Audit all tool permissions
- [ ] Add rate limiting per tool
- [ ] Implement tool timeout
- [ ] Add execution sandboxing

### Permission Tiers
- `Read` - Auto-approved
- `Write` - Auto-approved (sandboxed)
- `Execute` - Whitelist-checked
- `SystemCritical` - Requires approval

### Approval Flow
```
Tool call → Check tier → SystemCritical?
                              │
                    ┌─────────┴─────────┐
                    │                   │
                   Yes                 No
                    │                   │
            Request approval      Execute directly
                    │
            ┌───────┴───────┐
            │               │
        Approved        Denied
            │               │
        Execute         Return error
```

---

## Phase 11: Documentation

**Goal**: Professional-grade documentation for all components.

### Documents to Create
- ✅ `ARCHITECTURE.md` - System design
- [ ] `AGENT_LOOP.md` - Execution flow
- [ ] `MEMORY_SYSTEM.md` - Context management
- [ ] `SECURITY_MODEL.md` - Permission system
- [ ] `TOOL_DEVELOPMENT.md` - Creating tools
- [ ] `SOUL_GUIDE.md` - Soul configuration
- [ ] `API_REFERENCE.md` - Public APIs
- [ ] `DEPLOYMENT.md` - Production setup

### Quality Standards
- Clear diagrams
- Code examples
- Security considerations
- Performance characteristics
- Troubleshooting guides

---

## Phase 12: Production Validation

**Goal**: Verify system is ready for widget integration.

### Checklist
- [ ] Multi-step planning works
- [ ] Persistent context works
- [ ] Environment snapshot works
- [ ] Tool restrictions enforced
- [ ] Background tasks stable
- [ ] No raw shell execution
- [ ] Interface abstraction complete
- [ ] Memory compaction prevents overflow
- [ ] Token tracking implemented
- [ ] Crash-safe state recovery

### Testing
- [ ] Unit tests: All crates
- [ ] Integration tests: Cross-crate
- [ ] Stress tests: 1000+ sessions
- [ ] Failure simulation: Network, disk, etc.
- [ ] Security tests: Path traversal, injection
- [ ] Performance tests: Latency, throughput

---

## Phase 13: Widget Interface (Future)

**Goal**: GTK/Qt UI for visual interaction.

### Features
- Visual task progress
- Approval dialogs
- System tray integration
- Notification support
- Settings panel
- Session history viewer

### Technology
- GTK4 or Qt6
- Async event loop
- D-Bus integration
- Hyprland IPC

---

## Phase 14: Telegram Interface (Future)

**Goal**: Remote agent control via Telegram bot.

### Features
- Multi-user support
- Remote task execution
- Status notifications
- Approval requests
- Session management
- Secure authentication

### Technology
- `teloxide` crate
- Webhook or long polling
- End-to-end encryption
- Rate limiting per user

---

## Phase 15: Distributed Architecture (Future)

**Goal**: Multi-device agent coordination.

### Features
- Distributed locking
- Shared context store
- Task distribution
- Load balancing
- Fault tolerance

### Technology
- etcd or Consul
- gRPC communication
- Raft consensus
- PostgreSQL backend

---

## Current Status

### Completed (Phase 1-3)
- ✅ Layered architecture
- ✅ Persistent context system
- ✅ Environment awareness
- ✅ Memory compaction
- ✅ Soul configuration
- ✅ Permission engine
- ✅ LLM provider abstraction
- ✅ Tool trait system
- ✅ Interface abstraction
- ✅ Comprehensive tests
- ✅ Architecture documentation

### In Progress (Phase 4-6)
- ⏳ Multi-step planning engine
- ⏳ Structured tool categories
- ⏳ Soul system integration

### Next Up (Phase 7-9)
- ⏳ Background task manager
- ⏳ Widget interface stub
- ⏳ Observability & hardening

### Future (Phase 10+)
- ⏳ Security model upgrade
- ⏳ Complete documentation
- ⏳ Production validation
- ⏳ Widget integration
- ⏳ Telegram integration
- ⏳ Distributed architecture

---

## Timeline Estimate

- **Phase 4-6**: 1-2 weeks (Planning + Tools + Souls)
- **Phase 7-9**: 1-2 weeks (Tasks + Interfaces + Hardening)
- **Phase 10-12**: 1 week (Security + Docs + Validation)
- **Phase 13**: 2-3 weeks (Widget development)
- **Phase 14**: 1-2 weeks (Telegram bot)
- **Phase 15**: 3-4 weeks (Distributed system)

**Total to widget-ready**: 3-5 weeks
**Total to Telegram-ready**: 4-7 weeks
**Total to distributed**: 7-11 weeks

---

## Success Criteria

### Widget-Ready
- Multi-step planning functional
- All tools categorized and validated
- Soul system integrated
- Background tasks working
- Interface abstraction complete
- Security model enforced
- Documentation complete
- All tests passing

### Production-Ready
- 1000+ concurrent sessions tested
- Crash recovery validated
- Memory leaks eliminated
- Security audit passed
- Performance benchmarks met
- Monitoring integrated
- Deployment guide complete

---

## Design Principles

1. **No hacks** - Clean, maintainable code
2. **No shortcuts** - Proper abstractions
3. **No cross-layer leakage** - Strict boundaries
4. **No arbitrary shell** - Whitelisted commands only
5. **No soul logic in engine** - Configuration-driven
6. **No single-pass execution** - Iterative planning
7. **No token explosion** - Automatic compaction
8. **No state loss** - Persistent context

---

## What We're Building

**Local Autonomous AI Operating Layer for Linux + Hyprland**

Not:
- ❌ Chatbot
- ❌ CLI wrapper
- ❌ LLM demo
- ❌ Terminal agent

But:
- ✅ Agent runtime
- ✅ Operating system layer
- ✅ Persistent intelligence
- ✅ Multi-interface platform
- ✅ Production-grade system

---

## Questions?

See:
- `ARCHITECTURE.md` - System design
- `PHASE_1_3_COMPLETE.md` - Current status
- `README.md` - Quick start guide
