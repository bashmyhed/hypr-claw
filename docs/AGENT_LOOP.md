# Agent Loop

## Overview

The agent loop is the core execution mechanism of Hypr-Claw. It implements a multi-step planning system where the agent iteratively executes tasks until completion or max iterations.

## Execution Flow

```
┌─────────────────────────────────────────────────────────────┐
│                     Start Task                              │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│              Load Persistent Context                        │
│  - Recent history                                           │
│  - Long-term summary                                        │
│  - Facts                                                    │
│  - Active tasks                                             │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│            Capture Environment Snapshot                     │
│  - Workspace path                                           │
│  - Running processes                                        │
│  - Memory/disk usage                                        │
│  - Battery level                                            │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│                 Create Plan                                 │
│  - Goal: task description                                   │
│  - Steps: empty (will be populated)                         │
│  - Status: Pending                                          │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
        ┌─────────────────────────────┐
        │   Iteration Loop            │
        │   (0..max_iterations)       │
        └─────────────┬───────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│              Generate LLM Response                          │
│  Input:                                                     │
│  - System prompt (from soul)                                │
│  - Environment snapshot                                     │
│  - Recent history                                           │
│  - Available tools                                          │
│                                                             │
│  Output:                                                    │
│  - Content (optional)                                       │
│  - Tool calls (optional)                                    │
│  - Completed flag                                           │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
              ┌───────────────┐
              │ Tool calls?   │
              └───┬───────┬───┘
                  │       │
                Yes      No
                  │       │
                  ▼       └──────────────┐
┌─────────────────────────────────────┐  │
│     For each tool call:             │  │
│                                     │  │
│  1. Add step to plan                │  │
│  2. Check permissions               │  │
│  3. Check rate limits               │  │
│  4. Execute tool                    │  │
│  5. Update plan step                │  │
│  6. Add result to history           │  │
└─────────────────┬───────────────────┘  │
                  │                      │
                  └──────────┬───────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│           Add Assistant Response to History                 │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
              ┌───────────────┐
              │  Completed?   │
              └───┬───────┬───┘
                  │       │
                Yes      No
                  │       │
                  ▼       └──────────────┐
┌─────────────────────────────────────┐  │
│  Compact Context                    │  │
│  Save Context                       │  │
│  Return Result                      │  │
└─────────────────────────────────────┘  │
                                         │
                                         ▼
                              ┌──────────────────┐
                              │ Max iterations?  │
                              └───┬──────────┬───┘
                                  │          │
                                Yes         No
                                  │          │
                                  ▼          │
                          Return Error       │
                                             │
                                             └─► Continue Loop
```

## Pseudocode

```rust
async fn execute_task(context: &mut AgentContext, task: &str) -> Result<String> {
    // Initialize
    let mut plan = Plan::new(task.to_string());
    context.history.push(user_message(task));
    
    // Iteration loop
    for iteration in 0..context.soul_config.max_iterations {
        tracing::debug!("Iteration {}/{}", iteration + 1, max_iterations);
        
        // Generate LLM response
        let response = provider.generate(
            context,
            &context.history
        ).await?;
        
        // Handle tool calls
        if !response.tool_calls.is_empty() {
            for tool_call in response.tool_calls {
                // Add step to plan
                plan.add_step(format!("Execute tool: {}", tool_call.name));
                
                // Check permissions
                let permission = policy.check(&tool_call);
                if permission.requires_approval() {
                    if !interface.request_approval(&tool_call).await {
                        plan.fail_step("User denied approval");
                        continue;
                    }
                }
                
                // Check rate limits
                if !rate_limiter.check(&tool_call.name) {
                    plan.fail_step("Rate limit exceeded");
                    metrics.inc_permission_denials();
                    continue;
                }
                
                // Execute tool
                metrics.inc_tool_executions();
                let result = executor.execute(&tool_call, context).await?;
                
                // Update plan
                if result.success {
                    plan.complete_step(result.output);
                } else {
                    plan.fail_step(result.error);
                    metrics.inc_tool_failures();
                }
                
                // Add to history
                context.history.push(tool_result(result));
            }
        }
        
        // Add assistant response
        if let Some(content) = response.content {
            context.history.push(assistant_message(content));
        }
        
        // Check completion
        if response.completed {
            tracing::info!("Task completed. Progress: {:.1}%", plan.progress() * 100.0);
            
            // Compact and save
            if ContextCompactor::compact(&mut context) {
                metrics.inc_compactions();
            }
            context_manager.save(&context).await?;
            
            return Ok(response.content.unwrap_or_default());
        }
    }
    
    // Max iterations reached
    Err(EngineError::MaxIterations)
}
```

## Key Components

### 1. Context Loading
- Load persistent context from `./data/context/<session_id>.json`
- Includes history, facts, tasks, tool stats
- Survives restarts

### 2. Environment Snapshot
- Captured before every iteration
- Includes workspace, processes, memory, disk, battery
- Injected into system prompt

### 3. Plan Management
- Tracks execution steps
- Monitors progress (0.0-1.0)
- Records success/failure per step

### 4. LLM Generation
- Sends context + tools to LLM
- Receives content and/or tool calls
- Handles completion flag

### 5. Tool Execution
- Permission checking (blocked patterns, tier)
- Rate limiting (per tool, per session)
- Sandboxed execution
- Result capture

### 6. Context Compaction
- Automatic when history > 50 entries
- Token-based when > 100k tokens
- Preserves key facts
- Summarizes older history

### 7. Metrics Tracking
- LLM requests/failures
- Tool executions/failures
- Compactions
- Permission denials

## Iteration Limits

Configured per soul:
- `safe_assistant`: 10 iterations
- `system_admin`: 20 iterations
- `automation_agent`: 50 iterations
- `research_agent`: 15 iterations

## Error Handling

### Max Iterations Reached
```rust
Err(EngineError::MaxIterations)
```
- Task too complex
- Increase max_iterations in soul config
- Or break into smaller tasks

### Tool Execution Failed
```rust
plan.fail_step(error_message)
metrics.inc_tool_failures()
```
- Logged to history
- Agent can retry or adjust approach

### Permission Denied
```rust
PermissionResult::Denied(reason)
metrics.inc_permission_denials()
```
- Blocked pattern detected
- Tool not in allowed list
- Rate limit exceeded

### LLM Failure
```rust
Err(EngineError::Provider(error))
metrics.inc_llm_failures()
```
- Network error
- API error
- Timeout

## State Persistence

### After Each Iteration
- History updated with tool results
- Plan steps updated
- Metrics incremented

### On Completion
- Context compacted if needed
- Full context saved to disk
- Metrics snapshot available

### On Failure
- Partial context saved
- Error logged
- Metrics updated

## Concurrency

- Multiple sessions run in parallel
- Per-session locking prevents conflicts
- Rate limiting per session
- Metrics are thread-safe (atomic)

## Performance

- Async/await throughout
- Non-blocking I/O
- Lock-free metrics
- Efficient compaction

## Security

- Permission checks before execution
- Rate limiting prevents abuse
- Sandboxed file operations
- Blocked dangerous patterns
- Approval flow for critical ops
