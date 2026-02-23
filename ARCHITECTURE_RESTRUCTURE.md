# Hypr-Claw Architecture Restructure - Implementation Plan

**Date**: 2026-02-23  
**Status**: 🚧 IN PROGRESS  
**Goal**: Transform Hypr-Claw into a production-grade local OS agent

---

## Executive Summary

This document outlines the complete architectural restructure of Hypr-Claw from a single-shot CLI tool into a persistent, REPL-based OS agent with:

- Continuous session memory
- Structured OS capabilities
- Hyprland integration
- Background task management
- Permission approval flow
- Provider capability enforcement

---

## Current State Analysis

### Problems Identified

1. **Single-Shot Execution** - Process restarts between tasks, losing context
2. **Nested Tokio Runtimes** - `block_on()` calls within async context
3. **Generic Shell Execution** - Unsafe, unstructured command execution
4. **No REPL Loop** - Not conversational, not persistent
5. **Provider Inconsistency** - Codex doesn't support function calling
6. **No Background Tasks** - Cannot run long-running operations
7. **No Approval Flow** - Critical operations execute without confirmation
8. **Fragmented Tool System** - Mix of structured and unstructured tools

### What Works (Keep)

✅ Context persistence system  
✅ Permission engine  
✅ Audit logging  
✅ Lock manager  
✅ Tool registry  
✅ Memory compaction  
✅ Environment awareness  

---

## Phase 1: Core Runtime Rewrite ✅ STARTED

### Objective
Remove nested runtimes, implement REPL loop

### Implementation

**Files Created**:
- `hypr-claw-app/src/repl.rs` - REPL agent runtime

**Key Changes**:
1. Single `#[tokio::main]` at binary entry
2. All async operations use `.await`
3. Blocking operations use `tokio::task::spawn_blocking()`
4. REPL loop maintains session state

**REPL Loop Structure**:
```rust
loop {
    print!("hypr> ");
    let input = read_line();
    
    match input {
        "exit" => break,
        "status" => show_status(),
        "tasks" => show_tasks(),
        _ => {
            let response = agent.run(session_id, input).await;
            println!(response);
        }
    }
}
```

**Status**: ✅ Module created, needs integration

---

## Phase 2: Structured OS Capability Layer ✅ STARTED

### Objective
Replace generic shell execution with type-safe OS operations

### Implementation

**Files Created**:
- `hypr-claw-tools/src/os_capabilities/mod.rs` - Module structure
- `hypr-claw-tools/src/os_capabilities/filesystem.rs` - File operations
- `hypr-claw-tools/src/os_capabilities/hyprland.rs` - Hyprland control
- `hypr-claw-tools/src/os_capabilities/process.rs` - Process management
- `hypr-claw-tools/src/os_capabilities/system.rs` - System operations

**Capabilities Implemented**:

**Filesystem**:
- `fs.create_dir(path)` - Create directory
- `fs.delete(path)` - Delete file/directory
- `fs.move(from, to)` - Move/rename
- `fs.copy(from, to)` - Copy file
- `fs.read(path)` - Read file
- `fs.write(path, content)` - Write file
- `fs.list(path)` - List directory

**Hyprland**:
- `hypr.workspace.switch(id)` - Switch workspace
- `hypr.workspace.move_window(window_id, workspace_id)` - Move window
- `hypr.window.focus(window_id)` - Focus window
- `hypr.window.close(window_id)` - Close window
- `hypr.exec(command)` - Execute program
- `hypr.get_active_workspace()` - Get current workspace

**Process**:
- `proc.spawn(command, args)` - Spawn process
- `proc.kill(pid)` - Kill process
- `proc.list()` - List processes

**System**:
- `wallpaper.set(image_path)` - Set wallpaper
- `system.shutdown()` - Shutdown
- `system.reboot()` - Reboot
- `system.battery()` - Get battery level
- `system.memory()` - Get memory info

**Security**:
- All operations validate inputs
- No arbitrary command injection
- Numeric workspace IDs only
- Path validation for filesystem ops

**Status**: ✅ Modules created, needs tool wrappers

---

## Phase 3: Hyprland Integration ✅ COMPLETE

### Objective
Make agent Hyprland-aware with workspace control

### Implementation

**Hyprland Control via hyprctl**:
```rust
// Switch workspace
hyprctl dispatch workspace 3

// Move window
hyprctl dispatch movetoworkspace 3,window_id

// Execute program
hyprctl dispatch exec code
```

**Safety**:
- Workspace IDs validated as numeric
- No command injection possible
- Error handling for failed operations

**Status**: ✅ Complete in os_capabilities/hyprland.rs

---

## Phase 4: Background Task Manager ⏳ NEXT

### Objective
Enable async task execution with progress tracking

### Design

**TaskManager Structure**:
```rust
pub struct TaskManager {
    tasks: Arc<Mutex<HashMap<TaskId, TaskHandle>>>,
}

impl TaskManager {
    pub async fn spawn_task(&self, task: Task) -> TaskId;
    pub async fn get_status(&self, id: TaskId) -> TaskStatus;
    pub async fn cancel_task(&self, id: TaskId) -> Result<()>;
    pub async fn list_tasks(&self) -> Vec<TaskInfo>;
}
```

**Task Persistence**:
- Store in `ContextData.active_tasks`
- Update progress periodically
- Resume on restart (for resumable tasks)

**Example Flow**:
```
User: "Download repo and build in background"
Agent: → spawn_task(download_and_build)
       → Returns TaskId: task_001
       → "Build running in background (Task ID: task_001)"

User: "status"
Agent: → list_tasks()
       → "Task task_001: Building... 45% complete"
```

**Status**: ⏳ Design complete, implementation pending

---

## Phase 5: Approval Flow ⏳ NEXT

### Objective
Require user confirmation for critical operations

### Design

**Approval Trigger**:
```rust
if tool.permission_tier() == PermissionTier::SystemCritical {
    let approved = request_approval(&tool_description).await?;
    if !approved {
        return Err("Operation denied by user");
    }
}
```

**Approval UX**:
```
⚠️  APPROVAL REQUIRED
Action: Delete directory /home/user/important
Risk: High - Data loss possible
Approve? [y/N]: _
```

**Timeout**: 30 seconds, defaults to deny

**Audit**: All approval decisions logged

**Status**: ⏳ Design complete, implementation pending

---

## Phase 6: Provider Cleanup ⏳ NEXT

### Objective
Enforce provider capability requirements

### Design

**Provider Capability Flag**:
```rust
pub trait LLMProvider {
    fn supports_function_calling(&self) -> bool;
}
```

**Startup Validation**:
```rust
if !provider.supports_function_calling() {
    return Err("Agent mode requires function calling support");
}
```

**Provider Status**:
- ✅ Google Gemini - Supports function calling
- ✅ NVIDIA - Supports function calling (OpenAI-compatible)
- ✅ Local models - Supports function calling (OpenAI-compatible)
- ❌ Codex - Does NOT support function calling
- ✅ OpenAI API - Supports function calling

**Codex Handling**:
- Remove from agent mode
- Keep for chat-only mode
- Add clear error message

**Status**: ⏳ Design complete, implementation pending

---

## Phase 7: Soul System Integration ⏳ NEXT

### Objective
Integrate Soul configuration into AgentLoop

### Design

**Soul Loading**:
```rust
let soul = Soul::load(&soul_id)?;
let allowed_tools = soul.config.allowed_tools;
let max_iterations = soul.config.max_iterations;
```

**Tool Filtering**:
```rust
let tool_schemas = registry.get_tool_schemas(agent_id);
let filtered = tool_schemas.into_iter()
    .filter(|schema| allowed_tools.contains(&schema.name))
    .collect();
```

**Context Storage**:
```rust
context.active_soul_id = Some(soul_id);
```

**Status**: ⏳ Design complete, implementation pending

---

## Phase 8: Memory Hardening ⏳ NEXT

### Objective
Integrate Plan into ContextData for persistent planning

### Design

**ContextData Update**:
```rust
pub struct ContextData {
    // ... existing fields ...
    pub current_plan: Option<Plan>,
}
```

**Agent Execution Flow**:
```
1. Generate plan
2. Store in context
3. Execute steps
4. Update step status
5. Persist context
6. Revise plan if failure
```

**Status**: ⏳ Design complete, implementation pending

---

## Phase 9: Conversational UX ⏳ NEXT

### Objective
Enhance REPL with OS-level commands

### Commands

**Implemented**:
- `exit`, `quit` - Exit agent
- `help` - Show help
- `status` - Show agent status
- `clear` - Clear screen

**To Implement**:
- `tasks` - List background tasks
- `soul switch <id>` - Switch soul
- `approve <task_id>` - Approve pending task
- `cancel <task_id>` - Cancel background task
- `history` - Show conversation history
- `context` - Show context stats

**Status**: ⏳ Partial implementation

---

## Phase 10: Production Hardening ⏳ NEXT

### Objective
Add production-grade reliability features

### Features

**Structured Logging**:
```rust
tracing::info!("Agent started");
tracing::error!("Tool execution failed: {}", error);
```

**Metrics**:
```rust
metrics::counter!("tool_executions").increment(1);
metrics::histogram!("llm_latency").record(duration);
```

**Graceful Shutdown**:
```rust
tokio::signal::ctrl_c().await?;
context.save().await?;
println!("Context saved. Goodbye!");
```

**Panic Recovery**:
```rust
let result = std::panic::catch_unwind(|| {
    // Agent execution
});
```

**Status**: ⏳ Design complete, implementation pending

---

## Migration Strategy

### Step 1: Create New Modules (✅ DONE)
- REPL runtime
- OS capabilities
- Keep existing code working

### Step 2: Integrate REPL (⏳ NEXT)
- Update main.rs to use REPL
- Test with existing tools
- Verify context persistence

### Step 3: Migrate Tools (⏳ NEXT)
- Create tool wrappers for OS capabilities
- Update tool registry
- Remove generic shell tool

### Step 4: Add Background Tasks (⏳ NEXT)
- Implement TaskManager
- Integrate with context
- Add task commands

### Step 5: Add Approval Flow (⏳ NEXT)
- Implement approval interface
- Integrate with permission engine
- Add audit logging

### Step 6: Provider Cleanup (⏳ NEXT)
- Add capability checks
- Remove/restrict Codex
- Update documentation

### Step 7: Soul Integration (⏳ NEXT)
- Load souls in AgentLoop
- Filter tools by soul
- Store active soul in context

### Step 8: Memory Hardening (⏳ NEXT)
- Add Plan to ContextData
- Implement plan generation
- Persist plans

### Step 9: UX Enhancement (⏳ NEXT)
- Add remaining commands
- Improve error messages
- Add help system

### Step 10: Production Hardening (⏳ NEXT)
- Add logging
- Add metrics
- Add graceful shutdown
- Add panic recovery

---

## Testing Strategy

### Unit Tests
- Each OS capability function
- REPL command parsing
- Task manager operations
- Approval flow logic

### Integration Tests
- Full REPL session
- Multi-step task execution
- Background task lifecycle
- Context persistence across restarts

### Manual Testing
- Real Hyprland environment
- Actual file operations
- Process management
- Workspace switching

---

## Success Criteria

✅ **REPL Loop** - Persistent session, no restarts  
⏳ **Structured Tools** - No generic shell execution  
⏳ **Hyprland Control** - Workspace and window management  
⏳ **Background Tasks** - Async task execution  
⏳ **Approval Flow** - User confirmation for critical ops  
⏳ **Provider Enforcement** - Only function-calling providers  
⏳ **Soul Integration** - Soul-based tool filtering  
⏳ **Plan Persistence** - Plans stored in context  
⏳ **Production Ready** - Logging, metrics, graceful shutdown  

---

## Timeline Estimate

- **Phase 1-3**: ✅ 2 hours (COMPLETE)
- **Phase 4-6**: ⏳ 4 hours (IN PROGRESS)
- **Phase 7-8**: ⏳ 3 hours
- **Phase 9-10**: ⏳ 3 hours

**Total**: ~12 hours of focused implementation

---

## Next Steps

1. ✅ Create OS capability modules
2. ⏳ Create tool wrappers for OS capabilities
3. ⏳ Integrate REPL into main.rs
4. ⏳ Implement TaskManager
5. ⏳ Implement approval flow
6. ⏳ Add provider capability checks
7. ⏳ Integrate Soul system
8. ⏳ Add Plan to ContextData
9. ⏳ Enhance REPL commands
10. ⏳ Add production hardening

---

**Status**: 🚧 Architecture defined, core modules created, integration in progress

This is a structural rewrite, not a feature patch. Each phase builds on the previous, maintaining correctness over speed.
