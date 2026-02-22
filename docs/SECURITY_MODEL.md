# Security Model

## Overview

Multi-layer security system with permission tiers, rate limiting, sandboxing, and approval flows.

## Security Layers

```
┌─────────────────────────────────────────────────────────────┐
│  Layer 1: Permission Engine                                 │
│  - Blocked patterns                                         │
│  - Permission tiers                                         │
│  - Approval requirements                                    │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│  Layer 2: Rate Limiter                                      │
│  - Per-tool limits                                          │
│  - Per-session limits                                       │
│  - Time-window based                                        │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│  Layer 3: Tool Validation                                   │
│  - Schema validation                                        │
│  - Argument type checking                                   │
│  - Required field enforcement                               │
└─────────────────────┬───────────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────────┐
│  Layer 4: Sandbox                                           │
│  - Path restrictions                                        │
│  - Command whitelist                                        │
│  - Execution isolation                                      │
└─────────────────────────────────────────────────────────────┘
```

## Permission Tiers

### Read
- **Risk**: Low
- **Approval**: Not required
- **Examples**: file_read, file_list, process_list, system_info

### Write
- **Risk**: Medium
- **Approval**: Not required (within sandbox)
- **Examples**: file_write

### Execute
- **Risk**: Medium-High
- **Approval**: Not required (if whitelisted)
- **Examples**: Whitelisted commands only

### SystemCritical
- **Risk**: High
- **Approval**: **Required**
- **Examples**: System modifications, network access

## Blocked Patterns

Dangerous commands automatically denied:

```rust
vec![
    "rm -rf",           // Recursive delete
    "dd if=",           // Disk operations
    "mkfs",             // Format filesystem
    "format",           // Format disk
    "shutdown",         // System shutdown
    "reboot",           // System reboot
    "init 0",           // Shutdown
    "init 6",           // Reboot
    ":(){ :|:& };:",    // Fork bomb
]
```

## Rate Limiting

### Configuration

```rust
let mut limiter = RateLimiter::new();

// 10 calls per minute per tool
limiter.set_limit("file_write".to_string(), 10, Duration::from_secs(60));

// 100 calls per hour per session
limiter.set_limit("session:user123".to_string(), 100, Duration::from_secs(3600));
```

### Enforcement

```rust
if !rate_limiter.check(&tool_name) {
    return Err(ToolError::RateLimitExceeded);
}
```

## Sandboxing

### File Operations

All file tools restricted to `./sandbox/`:

```rust
let full_path = sandbox_path.join(user_path);

// Security check
if !full_path.starts_with(&sandbox_path) {
    return Err(ToolError::PermissionDenied("Path outside sandbox"));
}
```

### Path Traversal Prevention

```rust
// Blocked:
file_read("../../../etc/passwd")  // ❌
file_read("/etc/passwd")           // ❌

// Allowed:
file_read("data.txt")              // ✅
file_read("subdir/file.txt")       // ✅
```

### Command Whitelist

Only approved commands allowed:

```rust
const WHITELIST: &[&str] = &[
    "ls", "cat", "echo", "pwd", "date", "whoami"
];

if !WHITELIST.contains(&command) {
    return Err(ExecutorError::NotWhitelisted(command));
}
```

## Approval Flow

### SystemCritical Operations

```
Tool Call (SystemCritical)
    │
    ▼
┌─────────────────────────────────┐
│  Check Permission Tier          │
└─────────────┬───────────────────┘
              │
              ▼
      ┌───────────────┐
      │ SystemCritical?│
      └───┬───────┬───┘
          │       │
         Yes     No
          │       │
          ▼       └─► Execute
┌─────────────────────────────────┐
│  Request User Approval          │
│  "⚠️  Approval required: ..."   │
│  "Approve? (y/n): "             │
└─────────────┬───────────────────┘
              │
              ▼
      ┌───────────────┐
      │  Approved?    │
      └───┬───────┬───┘
          │       │
         Yes     No
          │       │
          ▼       ▼
      Execute   Deny
```

### Implementation

```rust
let permission = policy.check(&tool_call, PermissionTier::SystemCritical);

match permission {
    PermissionResult::Allowed => {
        // Execute directly
    }
    PermissionResult::RequiresApproval => {
        if interface.request_approval(&tool_call).await {
            // Execute with approval
        } else {
            // Deny
            metrics.inc_permission_denials();
        }
    }
    PermissionResult::Denied(reason) => {
        // Block
        metrics.inc_permission_denials();
    }
}
```

## Soul-Based Access Control

### Tool Restrictions

Each soul defines allowed tools:

```yaml
# safe_assistant.yaml
config:
  allowed_tools:
    - echo
    - file_read
    - file_write
    - file_list
```

### Autonomy Mode

```yaml
# automation_agent.yaml
config:
  autonomy_mode: auto  # No approval required

# safe_assistant.yaml
config:
  autonomy_mode: confirm  # Approval required for critical ops
```

### Risk Tolerance

```yaml
# system_admin.yaml
config:
  risk_tolerance: medium  # More permissive

# safe_assistant.yaml
config:
  risk_tolerance: low  # More restrictive
```

## Audit Logging

All security events logged:

```rust
tracing::warn!(
    "Permission denied: {} attempted {} (reason: {})",
    user_id,
    tool_name,
    reason
);
```

## Metrics

Security metrics tracked:

```rust
metrics.inc_permission_denials();  // Blocked operations
metrics.inc_tool_failures();       // Failed executions
```

## Best Practices

### 1. Principle of Least Privilege

```yaml
# Give minimum necessary permissions
config:
  allowed_tools:
    - file_read  # ✅ Only what's needed
    # - file_write  # ❌ Not needed, don't include
```

### 2. Use Confirm Mode for Sensitive Operations

```yaml
config:
  autonomy_mode: confirm  # Require approval
```

### 3. Set Appropriate Rate Limits

```rust
// Prevent abuse
limiter.set_limit("file_write".to_string(), 10, Duration::from_secs(60));
```

### 4. Monitor Metrics

```rust
let snapshot = metrics.snapshot();
if snapshot.permission_denials > 10 {
    tracing::warn!("High number of permission denials");
}
```

### 5. Regular Security Audits

```bash
# Check audit logs
tail -f ./data/audit.log

# Review permission denials
grep "Permission denied" ./data/audit.log
```

## Attack Vectors & Mitigations

### 1. Path Traversal

**Attack**: `file_read("../../../etc/passwd")`

**Mitigation**:
```rust
if !full_path.starts_with(&sandbox_path) {
    return Err(ToolError::PermissionDenied);
}
```

### 2. Command Injection

**Attack**: `shell_exec("ls; rm -rf /")`

**Mitigation**: No shell_exec tool. Whitelist only.

### 3. Symlink Escape

**Attack**: Create symlink outside sandbox

**Mitigation**:
```rust
let canonical = full_path.canonicalize()?;
if !canonical.starts_with(&sandbox_path) {
    return Err(ToolError::PermissionDenied);
}
```

### 4. Rate Limit Bypass

**Attack**: Multiple sessions to bypass limits

**Mitigation**: Global rate limits + per-session limits

### 5. Privilege Escalation

**Attack**: Modify soul config to gain permissions

**Mitigation**: Soul configs loaded from trusted directory only

## Security Checklist

- [ ] All file operations sandboxed
- [ ] Command whitelist enforced
- [ ] Dangerous patterns blocked
- [ ] Rate limits configured
- [ ] Approval flow for critical ops
- [ ] Audit logging enabled
- [ ] Metrics monitored
- [ ] Regular security reviews

## Incident Response

### Suspicious Activity Detected

1. **Check metrics**:
   ```rust
   let snapshot = metrics.snapshot();
   println!("Permission denials: {}", snapshot.permission_denials);
   ```

2. **Review audit logs**:
   ```bash
   grep "Permission denied" ./data/audit.log
   ```

3. **Identify pattern**:
   - Which tool?
   - Which user?
   - What was attempted?

4. **Take action**:
   - Block user if malicious
   - Add pattern to blocklist
   - Adjust rate limits

### Breach Response

1. **Isolate**: Stop affected sessions
2. **Investigate**: Review logs and context
3. **Remediate**: Fix vulnerability
4. **Document**: Update security model
5. **Test**: Verify fix

## Future Enhancements

- [ ] Network access controls
- [ ] Resource usage limits (CPU, memory)
- [ ] Encrypted context storage
- [ ] Multi-factor approval
- [ ] Anomaly detection
- [ ] Security policy versioning
