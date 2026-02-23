# HYPR-CLAW ARCHITECTURE REWRITE PLAN
## From Fragmented CLI to Production OS Agent

**Status**: Planning Phase  
**Target**: Jarvis-level local autonomous OS assistant  
**Philosophy**: Open-Claw aligned (local-first, tool-driven, memory-persistent)

---

## CURRENT STATE ANALYSIS

### ✅ What Works
- Persistent context system with compaction
- Multi-crate architecture (7 specialized crates)
- Tool registry and dispatcher
- Permission engine with blocked patterns
- Audit logging
- Lock manager for session isolation
- REPL loop structure exists
- Hyprland tool integration started
- Task manager foundation
- Multiple LLM provider support

### ❌ Critical Issues
1. **Runtime Nesting**: `block_in_place` workaround needed (FIXED)
2. **Tool Message Format**: Gemini API expects string content for tool role (FIXED)
3. **Silent Fallback**: LLM can respond with text instead of tool calls
4. **No Tool Enforcement**: Agent doesn't validate tool usage
5. **Provider Inconsistency**: Codex web backend unreliable for function calling
6. **No Approval UX**: SystemCritical operations not prompting user
7. **Incomplete Soul Integration**: Souls exist but not fully wired
8. **No Background Task Persistence**: Tasks don't survive restart

---

## PHASE 1: CORE RUNTIME FIXES [PRIORITY: CRITICAL]

### 1.1 Eliminate Runtime Nesting ✅ DONE
- [x] Fixed `block_in_place` usage in RuntimeDispatcherAdapter
- [x] Verified single `#[tokio::main]` at binary entry
- [ ] Audit entire codebase for remaining `Runtime::new()` or `block_on()`

### 1.2 Fix Tool Message Serialization ✅ DONE
- [x] Tool role messages now serialize content as JSON string
- [x] Gemini API compatibility restored

### 1.3 Enforce Tool-Only Execution
**Location**: `hypr-claw-runtime/src/agent_loop.rs`

**Current Behavior**:
```rust
// LLM can return content without tool calls
if response.content.is_some() && response.tool_calls.is_none() {
    // Agent just explains instead of acting
}
```

**Required Behavior**:
```rust
// If user intent requires action AND no tool called
if requires_action(&user_message) && response.tool_calls.is_none() {
    return Err(RuntimeError::ToolRequired(
        "Action required but no tool invoked"
    ));
}
```

**Implementation**:
- Add intent classifier (simple keyword matching initially)
- Keywords: "create", "delete", "move", "switch", "set", "run", "kill", etc.
- If intent detected + no tool call → error
- Agent must either call tool OR explicitly refuse with permission reason

---

## PHASE 2: STRUCTURED OS CAPABILITY LAYER [PRIORITY: HIGH]

### 2.1 Current Tool Status

**Implemented** (in `hypr-claw-tools/src/os_tools.rs`):
- ✅ `fs_create_dir`
- ✅ `fs_delete`
- ✅ `fs_move`
- ✅ `fs_copy`
- ✅ `hypr_workspace_switch`
- ✅ `hypr_exec`
- ✅ `proc_spawn`
- ✅ `proc_list`
- ✅ `wallpaper_set`

**Sandboxed Tools** (in `hypr-claw-tools/src/tools/`):
- ✅ `file.read` (sandbox only)
- ✅ `file.write` (sandbox only)
- ✅ `file.list` (sandbox only)

**Missing Critical Tools**:
- ❌ `proc.kill` - Kill process by PID
- ❌ `hypr.window.focus` - Focus window
- ❌ `hypr.window.close` - Close window
- ❌ `hypr.window.move` - Move window to workspace
- ❌ `system.shutdown` - Shutdown system (SystemCritical)
- ❌ `system.reboot` - Reboot system (SystemCritical)
- ❌ `system.battery` - Get battery status
- ❌ `system.memory` - Get memory info

### 2.2 Remove Generic Shell Tool
**Action**: Remove `shell_exec` from default soul
**Reason**: Too dangerous, allows arbitrary command injection
**Alternative**: High-risk soul can have it with explicit approval

### 2.3 Tool Permission Tiers

| Tool | Tier | Auto-Approve | Audit |
|------|------|--------------|-------|
| `file.read` (sandbox) | Read | ✅ | ✅ |
| `file.write` (sandbox) | Write | ✅ | ✅ |
| `fs_create_dir` | Write | ✅ | ✅ |
| `fs_delete` | Write | ⚠️ Confirm | ✅ |
| `proc_spawn` | Execute | ⚠️ Confirm | ✅ |
| `proc_kill` | Execute | ⚠️ Confirm | ✅ |
| `hypr_exec` | Execute | ✅ | ✅ |
| `system.shutdown` | SystemCritical | ❌ Require | ✅ |
| `system.reboot` | SystemCritical | ❌ Require | ✅ |

---

## PHASE 3: HYPRLAND DEEP INTEGRATION [PRIORITY: MEDIUM]

### 3.1 Workspace Control
**Status**: Basic `workspace_switch` implemented

**Needed**:
```rust
// hypr-claw-tools/src/os_capabilities/hyprland.rs
pub async fn workspace_list() -> Result<Vec<WorkspaceInfo>, OsError>;
pub async fn workspace_get_active() -> Result<u32, OsError>;
pub async fn workspace_move_window(window_id: String, workspace: u32) -> Result<(), OsError>;
```

### 3.2 Window Management
**Status**: Not implemented

**Needed**:
```rust
pub async fn window_list() -> Result<Vec<WindowInfo>, OsError>;
pub async fn window_focus(window_id: String) -> Result<(), OsError>;
pub async fn window_close(window_id: String) -> Result<(), OsError>;
pub async fn window_move(window_id: String, x: i32, y: i32) -> Result<(), OsError>;
```

### 3.3 Example Workflow
```
User: "Open Firefox on workspace 3 and maximize it"

Agent Plan:
1. hypr_exec("firefox") → PID
2. Wait 2s for window spawn
3. window_list() → find Firefox window
4. workspace_move_window(window_id, 3)
5. workspace_switch(3)
6. window_maximize(window_id)
```

---

## PHASE 4: BACKGROUND TASK MANAGER [PRIORITY: MEDIUM]

### 4.1 Current State
**Location**: `crates/tasks/src/lib.rs`

**Implemented**:
- ✅ `TaskManager::spawn_task()`
- ✅ `TaskManager::list_tasks()`
- ✅ `TaskManager::cancel_task()`
- ✅ In-memory task tracking

**Missing**:
- ❌ Task persistence across restarts
- ❌ Task progress updates
- ❌ Task result storage
- ❌ Task resumption after crash

### 4.2 Persistence Strategy

**Add to `ContextData`**:
```rust
pub struct ContextData {
    // ... existing fields
    pub background_tasks: Vec<BackgroundTask>,
}

pub struct BackgroundTask {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub progress: f32,
    pub created_at: String,
    pub updated_at: String,
    pub result: Option<serde_json::Value>,
}
```

**On Startup**:
```rust
// Load context
let context = context_manager.load(&session_id).await?;

// Resume incomplete tasks
for task in context.background_tasks {
    if task.status == TaskStatus::Running {
        task_manager.resume_task(task).await?;
    }
}
```

---

## PHASE 5: APPROVAL FLOW [PRIORITY: HIGH]

### 5.1 Current State
**Permission Engine**: Exists in `hypr-claw-infra/src/infra/permission_engine.rs`
**Approval UX**: Not implemented

### 5.2 Implementation

**Add to `TerminalInterface`**:
```rust
// crates/interfaces/src/terminal.rs
impl TerminalInterface {
    pub async fn request_approval(&self, request: ApprovalRequest) -> bool {
        println!("\n⚠️  APPROVAL REQUIRED");
        println!("Tool: {}", request.tool_name);
        println!("Action: {}", request.description);
        println!("Risk: {:?}", request.risk_tier);
        println!("\nApprove? [y/N]: ");
        
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        
        matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
    }
}
```

**Integrate in `ToolDispatcher`**:
```rust
// hypr-claw-tools/src/dispatcher.rs
async fn dispatch(&self, ...) -> Result<ToolResult, ToolError> {
    let decision = self.permission_engine.check(&request);
    
    if decision.requires_approval {
        let approved = self.interface.request_approval(request).await;
        if !approved {
            return Err(ToolError::PermissionDenied("User denied approval"));
        }
    }
    
    // Execute tool
}
```

### 5.3 Approval History
**Add to `ContextData`**:
```rust
pub struct ContextData {
    // ... existing fields
    pub approval_history: Vec<ApprovalRecord>,
}

pub struct ApprovalRecord {
    pub timestamp: String,
    pub tool_name: String,
    pub approved: bool,
    pub reason: Option<String>,
}
```

---

## PHASE 6: PROVIDER CLEANUP [PRIORITY: MEDIUM]

### 6.1 Provider Capability Flags

**Add to provider trait**:
```rust
// hypr-claw-runtime/src/interfaces.rs
pub trait LLMProvider {
    fn supports_function_calling(&self) -> bool;
    fn supports_streaming(&self) -> bool;
    fn max_tokens(&self) -> usize;
}
```

**Validation on startup**:
```rust
if !llm_client.supports_function_calling() {
    return Err("Agent mode requires function calling support");
}
```

### 6.2 Provider Status

| Provider | Function Calling | Status | Action |
|----------|------------------|--------|--------|
| NVIDIA | ✅ | Stable | Keep |
| Google Gemini | ✅ | Stable | Keep |
| Local (Ollama) | ✅ | Stable | Keep |
| Codex Web | ⚠️ Unreliable | Unstable | Restrict to chat-only |
| Antigravity | ✅ | Stable | Integrate |
| Gemini CLI | ✅ | Stable | Integrate |

---

## PHASE 7: SOUL SYSTEM INTEGRATION [PRIORITY: HIGH]

### 7.1 Current State
**Souls**: Defined in `./souls/*.yaml`
**Loading**: Implemented in `hypr-claw-policy/src/soul.rs`
**Integration**: Partial

### 7.2 Full Integration

**Load soul on startup**:
```rust
// hypr-claw-app/src/main.rs
let soul = Soul::load(&format!("./souls/{}.yaml", agent_name))?;
```

**Filter tools by soul**:
```rust
// Only register tools allowed by soul
for tool_name in &soul.config.allowed_tools {
    if let Some(tool) = all_tools.get(tool_name) {
        registry.register(tool.clone());
    }
}
```

**Apply soul config to agent loop**:
```rust
let agent_loop = AgentLoop::new(
    // ... existing params
    soul.config.max_iterations,
);
```

**Store active soul in context**:
```rust
pub struct ContextData {
    // ... existing fields
    pub active_soul_id: String,
}
```

### 7.3 Soul Switching Command
```
hypr> soul switch system_admin
✅ Switched to soul: system_admin
   Max iterations: 20
   Autonomy: auto
   Risk tolerance: medium
```

---

## PHASE 8: MEMORY HARDENING [PRIORITY: MEDIUM]

### 8.1 Integrate Plan into Context

**Current**: Plan exists in `hypr-claw-core/src/planning.rs` but not persisted

**Add to `ContextData`**:
```rust
pub struct ContextData {
    // ... existing fields
    pub current_plan: Option<Plan>,
}
```

**Update plan during execution**:
```rust
// After each tool execution
context.current_plan.as_mut().unwrap().complete_step(step_id);
context_manager.save(&context).await?;
```

**Resume plan on restart**:
```rust
if let Some(plan) = context.current_plan {
    if !plan.is_complete() {
        println!("📋 Resuming incomplete plan: {}", plan.goal);
    }
}
```

---

## PHASE 9: CONVERSATIONAL UX [PRIORITY: LOW]

### 9.1 Enhanced REPL Commands

**Current**: `exit`, `help`, `status`, `tasks`, `clear`

**Add**:
- `soul switch <id>` - Switch active soul
- `soul list` - List available souls
- `approve <task_id>` - Approve pending task
- `history` - Show recent history
- `facts` - Show learned facts
- `plan` - Show current plan
- `context` - Show context stats

### 9.2 Rich Output Formatting
- Use colored output (via `colored` crate)
- Progress bars for long tasks (via `indicatif` crate)
- Table formatting for lists (via `prettytable` crate)

---

## PHASE 10: PRODUCTION HARDENING [PRIORITY: MEDIUM]

### 10.1 Structured Logging
**Current**: Mix of `println!` and `tracing`

**Standardize**:
```rust
use tracing::{info, warn, error, debug};

info!("Agent initialized: {}", agent_name);
warn!("Tool execution slow: {}ms", duration);
error!("LLM call failed: {}", error);
debug!("Context compacted: {} → {} entries", before, after);
```

### 10.2 Metrics Collection
**Add to `Metrics`**:
```rust
pub struct Metrics {
    // ... existing fields
    pub tools_by_type: HashMap<String, u64>,
    pub approval_rate: f64,
    pub avg_task_duration: Duration,
}
```

### 10.3 Graceful Shutdown
**Current**: Basic Ctrl+C handler exists

**Enhance**:
```rust
// On SIGINT/SIGTERM
1. Stop accepting new tasks
2. Wait for active tasks (max 30s)
3. Save context
4. Flush audit log
5. Exit
```

### 10.4 Panic Recovery
```rust
// Wrap agent loop in panic handler
let result = std::panic::catch_unwind(|| {
    agent_loop.run(...)
});

if result.is_err() {
    error!("Agent panicked! Saving context...");
    context_manager.save(&context).await?;
}
```

---

## IMPLEMENTATION TIMELINE

### Week 1: Core Fixes
- [x] Day 1: Fix runtime nesting ✅
- [x] Day 1: Fix tool message serialization ✅
- [ ] Day 2: Implement tool enforcement
- [ ] Day 3: Add missing OS tools (proc.kill, window management)
- [ ] Day 4: Implement approval flow
- [ ] Day 5: Test and debug

### Week 2: Integration
- [ ] Day 1: Full soul system integration
- [ ] Day 2: Background task persistence
- [ ] Day 3: Plan persistence
- [ ] Day 4: Provider capability validation
- [ ] Day 5: Test and debug

### Week 3: Polish
- [ ] Day 1: Enhanced REPL commands
- [ ] Day 2: Structured logging
- [ ] Day 3: Metrics and monitoring
- [ ] Day 4: Graceful shutdown and panic recovery
- [ ] Day 5: Documentation and examples

### Week 4: Validation
- [ ] Day 1-2: End-to-end testing
- [ ] Day 3: Security audit
- [ ] Day 4: Performance testing
- [ ] Day 5: Release preparation

---

## SUCCESS CRITERIA

### Functional Requirements
- ✅ Agent runs as persistent REPL
- ✅ Context survives restarts
- ⚠️ Tool-only execution enforced
- ⚠️ Approval flow for critical operations
- ⚠️ Background tasks persist and resume
- ⚠️ Soul system fully integrated
- ⚠️ Hyprland workspace/window control

### Non-Functional Requirements
- ⚠️ No runtime nesting errors
- ⚠️ No silent fallback to explanation
- ⚠️ Graceful shutdown on Ctrl+C
- ⚠️ Structured logging throughout
- ⚠️ Comprehensive error handling

### Example Workflow (End-to-End Test)
```
User: "Create a project folder, switch to workspace 3, open VSCode, and build in background"

Expected Behavior:
1. Agent generates plan with 5 steps
2. Calls fs_create_dir("~/projects/new-project")
3. Calls hypr_workspace_switch(3)
4. Calls hypr_exec("code ~/projects/new-project")
5. Spawns background task for build
6. Responds: "✅ Project created. Workspace switched. VSCode opened. Build running (Task ID: abc123)"

User: "status"

Expected Behavior:
Agent shows:
- Active session
- Current workspace
- Background tasks (1 running)
- Recent tool calls
```

---

## NOTES

### Design Principles
1. **Tool-First**: Never explain when action is possible
2. **Persistent**: Memory survives restarts
3. **Safe**: Multi-layer security with approval
4. **Transparent**: Audit everything
5. **Deterministic**: Same input → same behavior

### Anti-Patterns to Avoid
- ❌ Nested runtimes
- ❌ Silent fallback to explanation
- ❌ Generic shell execution in default soul
- ❌ Unapproved critical operations
- ❌ Lost context on crash
- ❌ Mixing sync and async incorrectly

### Future Enhancements (Post-MVP)
- Widget UI (GTK/Qt)
- Telegram bot interface
- Distributed multi-device coordination
- Voice control integration
- Custom tool development SDK
- Soul marketplace

---

**Status**: Planning Complete  
**Next Action**: Begin Week 1 implementation  
**Owner**: Development Team  
**Last Updated**: 2026-02-23
