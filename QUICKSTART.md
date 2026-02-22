# Hypr-Claw Quick Start Guide

## Build & Run

```bash
# Build release binary
cargo build --release

# Run the application
./target/release/hypr-claw
```

## Interactive Prompts

When you run the application, you'll be prompted for:

1. **LLM base URL**: The endpoint for your LLM service
   - Example: `http://localhost:8080`
   - Example: `https://api.openai.com/v1`

2. **Agent name**: Which agent configuration to use
   - Default: `default` (auto-generated)
   - Custom: Create `./data/agents/<name>.yaml`

3. **Your task**: The task for the agent to execute
   - Example: `echo hello world`
   - Example: `List files in sandbox`

## Directory Structure

After first run, these directories are created:

```
./data/
├── sessions/          # Session history
├── credentials/       # Encrypted credentials (unused in minimal version)
├── agents/           # Agent configurations
│   ├── default.yaml  # Auto-generated default agent
│   └── default_soul.md
└── audit.log         # Audit trail

./sandbox/            # Sandboxed file operations
```

## Agent Configuration

Default agent config (`./data/agents/default.yaml`):

```yaml
id: default
soul: default_soul.md
tools:
  - echo
  - file_read
  - file_write
  - file_list
  - shell_exec
```

## Available Tools

1. **echo** - Echo back input
2. **file_read** - Read files from sandbox
3. **file_write** - Write files to sandbox
4. **file_list** - List files in sandbox
5. **shell_exec** - Execute whitelisted shell commands

## Creating Custom Agents

1. Create agent config: `./data/agents/myagent.yaml`
2. Create soul file: `./data/agents/myagent_soul.md`
3. Run with: Enter `myagent` when prompted for agent name

Example custom agent:

```yaml
id: myagent
soul: myagent_soul.md
tools:
  - echo
  - file_read
```

## Verification

Run the verification script:

```bash
./verify.sh
```

This checks:
- ✅ cargo check passes
- ✅ cargo test passes
- ✅ cargo clippy passes
- ✅ Binary exists
- ✅ Directory structure is correct

## Development

```bash
# Check code
cargo check

# Run tests
cargo test

# Run clippy
cargo clippy --all-targets -- -D warnings

# Build debug version
cargo build

# Build release version
cargo build --release
```

## Troubleshooting

**Error: "Config file not found"**
- Ensure you're running from the project root
- Check that `./data/agents/default.yaml` exists
- Run the application once to auto-generate defaults

**Error: "LLM error"**
- Verify LLM endpoint is accessible
- Check LLM service is running
- Ensure correct URL format (include http:// or https://)

**Error: "Lock timeout"**
- Another session may be active
- Wait 30 seconds for lock to expire
- Check `./data/sessions/` for stale locks

## Architecture

```
┌─────────────────────────────────────┐
│     hypr-claw-app (Binary)          │
│     • System wiring                 │
│     • CLI interface                 │
└─────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────┐
│     hypr-claw-runtime               │
│     • Agent loop                    │
│     • LLM client                    │
│     • Message compaction            │
└─────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────┐
│     hypr_claw_tools                 │
│     • Tool dispatcher               │
│     • Tool registry                 │
│     • Sandboxed execution           │
└─────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────┐
│     hypr_claw (Infrastructure)      │
│     • Session store                 │
│     • Lock manager                  │
│     • Permission engine             │
│     • Audit logger                  │
└─────────────────────────────────────┘
```

## Next Steps

1. Set up an LLM endpoint (local or remote)
2. Run the application
3. Test with simple tasks
4. Create custom agents for specific use cases
5. Integrate with your workflow

## Support

For issues or questions:
- Check IMPLEMENTATION.md for detailed documentation
- Review CHANGES.md for what was modified
- Run verify.sh to ensure system integrity
