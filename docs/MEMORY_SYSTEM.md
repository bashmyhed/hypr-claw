# Memory System

## Overview

The memory system provides persistent, compacted context storage that survives restarts and prevents token explosion.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    ContextManager                           │
│  - Load/save context                                        │
│  - Atomic writes                                            │
│  - Session management                                       │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│                  ContextCompactor                           │
│  - History compaction                                       │
│  - Token management                                         │
│  - Fact deduplication                                       │
│  - Task pruning                                             │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│              ./data/context/<session_id>.json               │
│  - Persistent storage                                       │
│  - Human-readable JSON                                      │
│  - Atomic updates                                           │
└─────────────────────────────────────────────────────────────┘
```

## Context Structure

```json
{
  "session_id": "user:agent:timestamp",
  "system_state": {
    "last_workspace": "/home/user/project",
    "preferences": {}
  },
  "facts": [
    "User prefers verbose output",
    "Project uses Rust 1.75"
  ],
  "recent_history": [
    {
      "timestamp": 1708654321,
      "role": "user",
      "content": "Create a new file",
      "token_count": 5
    },
    {
      "timestamp": 1708654322,
      "role": "assistant",
      "content": "I'll create the file for you",
      "token_count": 8
    },
    {
      "timestamp": 1708654323,
      "role": "tool",
      "content": "{\"success\": true}",
      "token_count": 12
    }
  ],
  "long_term_summary": "User requested file creation. Successfully created test.txt.",
  "active_tasks": [
    {
      "id": "task_001",
      "description": "Monitor system resources",
      "status": "Running",
      "progress": 0.5,
      "created_at": 1708654000,
      "updated_at": 1708654321
    }
  ],
  "tool_stats": {
    "total_calls": 42,
    "by_tool": {
      "file_write": 15,
      "file_read": 20,
      "echo": 7
    },
    "failures": 2
  },
  "last_known_environment": {
    "workspace": "/home/user/project",
    "last_update": 1708654321,
    "system_snapshot": {
      "memory_usage_mb": 8192,
      "disk_usage_percent": 45.2
    }
  },
  "token_usage": {
    "total_input": 15420,
    "total_output": 8930,
    "by_session": 24350
  }
}
```

## Compaction Strategy

### Triggers

1. **History Length**: When `recent_history.len() > 50`
2. **Token Count**: When total tokens > 100,000
3. **Manual**: Explicit call to `compact()`

### Process

```
┌─────────────────────────────────────────────────────────────┐
│                  History Compaction                         │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
              ┌───────────────┐
              │ History > 50? │
              └───┬───────┬───┘
                  │       │
                Yes      No
                  │       │
                  ▼       └──────────────┐
┌─────────────────────────────────────┐  │
│  Take oldest (len - 30) entries     │  │
│  Summarize them                     │  │
│  Append to long_term_summary        │  │
│  Remove from recent_history         │  │
└─────────────────┬───────────────────┘  │
                  │                      │
                  └──────────┬───────────┘
                             │
                             ▼
              ┌───────────────────┐
              │ Tokens > 100k?    │
              └───┬───────────┬───┘
                  │           │
                Yes          No
                  │           │
                  ▼           └──────────┐
┌─────────────────────────────────────┐  │
│  Take oldest 50% of entries         │  │
│  Summarize them                     │  │
│  Append to long_term_summary        │  │
│  Remove from recent_history         │  │
└─────────────────┬───────────────────┘  │
                  │                      │
                  └──────────┬───────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────┐
│                  Fact Deduplication                         │
│  - Sort facts                                               │
│  - Remove duplicates                                        │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│                    Task Pruning                             │
│  - Keep running tasks                                       │
│  - Keep completed tasks < 24h old                           │
│  - Remove older completed tasks                             │
└─────────────────────────────────────────────────────────────┘
```

## API

### ContextManager

```rust
pub struct ContextManager {
    base_path: PathBuf,
}

impl ContextManager {
    // Initialize storage directory
    pub async fn initialize(&self) -> Result<(), MemoryError>
    
    // Load context (creates new if not exists)
    pub async fn load(&self, session_id: &str) -> Result<ContextData, MemoryError>
    
    // Save context atomically
    pub async fn save(&self, context: &ContextData) -> Result<(), MemoryError>
    
    // Delete context
    pub async fn delete(&self, session_id: &str) -> Result<(), MemoryError>
    
    // List all sessions
    pub async fn list_sessions(&self) -> Result<Vec<String>, MemoryError>
}
```

### ContextCompactor

```rust
pub struct ContextCompactor;

impl ContextCompactor {
    // Compact context, returns true if compaction occurred
    pub fn compact(context: &mut ContextData) -> bool
}
```

## Usage

### Load Context

```rust
let manager = ContextManager::new("./data/context");
manager.initialize().await?;

let context = manager.load("user:agent:123").await?;
```

### Update Context

```rust
// Add to history
context.recent_history.push(HistoryEntry {
    timestamp: chrono::Utc::now().timestamp(),
    role: "user".to_string(),
    content: "Hello".to_string(),
    token_count: Some(1),
});

// Add fact
context.facts.push("User speaks English".to_string());

// Update tool stats
context.tool_stats.total_calls += 1;
```

### Compact Context

```rust
let compacted = ContextCompactor::compact(&mut context);
if compacted {
    tracing::info!("Context compacted");
    metrics.inc_compactions();
}
```

### Save Context

```rust
manager.save(&context).await?;
```

## Compaction Example

### Before Compaction

```json
{
  "recent_history": [
    {"role": "user", "content": "Message 1", "token_count": 5},
    {"role": "assistant", "content": "Response 1", "token_count": 10},
    // ... 50 more entries ...
    {"role": "user", "content": "Message 51", "token_count": 5}
  ],
  "long_term_summary": ""
}
```

### After Compaction

```json
{
  "recent_history": [
    {"role": "user", "content": "Message 31", "token_count": 5},
    // ... 20 more recent entries ...
    {"role": "user", "content": "Message 51", "token_count": 5}
  ],
  "long_term_summary": "Previous conversation summary:\n- user: Message 1\n- assistant: Response 1\n..."
}
```

## Token Tracking

### Per Message

```rust
HistoryEntry {
    timestamp: 1708654321,
    role: "user",
    content: "Create a file",
    token_count: Some(4),  // Tracked per message
}
```

### Total Tracking

```rust
let total_tokens: usize = context.recent_history
    .iter()
    .filter_map(|e| e.token_count)
    .sum();

if total_tokens > 100_000 {
    // Trigger compaction
}
```

## Storage

### File Location

```
./data/context/
├── user1:agent1:123.json
├── user1:agent2:456.json
└── user2:agent1:789.json
```

### Atomic Writes

```rust
// Write to temp file
let temp_path = path.with_extension("tmp");
fs::write(&temp_path, content).await?;

// Atomic rename
fs::rename(&temp_path, &path).await?;
```

Ensures no corruption on crash.

## Performance

### Load Time
- O(1) file read
- JSON parsing: ~1ms for typical context

### Save Time
- O(1) file write
- Atomic rename: ~1ms

### Compaction Time
- O(n) where n = history length
- Typically < 10ms

### Memory Usage
- In-memory context: ~10-100 KB
- Disk storage: ~10-100 KB per session

## Best Practices

### 1. Compact Regularly

```rust
// After each task
if ContextCompactor::compact(&mut context) {
    metrics.inc_compactions();
}
```

### 2. Track Tokens

```rust
// Estimate tokens (rough: 1 token ≈ 4 chars)
let token_count = Some(content.len() / 4);
```

### 3. Prune Facts

```rust
// Keep facts relevant
context.facts.retain(|fact| is_relevant(fact));
```

### 4. Clean Old Tasks

```rust
// Remove completed tasks > 24h old
let cutoff = chrono::Utc::now().timestamp() - 86400;
context.active_tasks.retain(|task| {
    task.status != "Completed" || task.updated_at > cutoff
});
```

## Troubleshooting

### Context Too Large

**Symptom**: Slow LLM responses, high token usage

**Solution**:
```rust
// Force compaction
ContextCompactor::compact(&mut context);

// Or reduce max history
const MAX_RECENT_HISTORY: usize = 30; // Default: 50
```

### Lost Context

**Symptom**: Agent doesn't remember previous interactions

**Solution**:
```rust
// Check long_term_summary
println!("{}", context.long_term_summary);

// Increase history retention
const MAX_RECENT_HISTORY: usize = 100;
```

### Corrupted Context

**Symptom**: Failed to load context

**Solution**:
```rust
// Delete and recreate
manager.delete(session_id).await?;
let context = manager.load(session_id).await?;
```

## Future Enhancements

- [ ] Semantic summarization (LLM-based)
- [ ] Vector embeddings for facts
- [ ] Distributed context store
- [ ] Context versioning
- [ ] Automatic fact extraction
