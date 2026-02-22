# Hypr-Claw Directory Structure

## Current Structure (Post Phase 1-3)

```
hypr-claw/
│
├── crates/                          # NEW: Restructured architecture
│   ├── core/                        # ✅ Agent engine (soul-agnostic)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── agent_engine.rs      # Core execution loop
│   │   │   ├── planning.rs          # Planning loop (Phase 4)
│   │   │   └── types.rs             # Core types
│   │   └── Cargo.toml
│   │
│   ├── memory/                      # ✅ Persistent context system
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── context_manager.rs   # JSON persistence
│   │   │   ├── compactor.rs         # Automatic compaction
│   │   │   └── types.rs             # Memory types
│   │   └── Cargo.toml
│   │
│   ├── policy/                      # ✅ Souls + permissions
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── soul.rs              # Soul configuration
│   │   │   └── permissions.rs       # Permission engine
│   │   └── Cargo.toml
│   │
│   ├── executor/                    # ✅ Environment + commands
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── environment.rs       # System snapshot
│   │   │   └── command_executor.rs  # Whitelisted commands
│   │   └── Cargo.toml
│   │
│   ├── tools/                       # ✅ Structured tool system
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── traits.rs            # Tool trait
│   │   │   ├── registry.rs          # Tool registry
│   │   │   ├── system_tools.rs      # Echo, etc
│   │   │   └── file_tools.rs        # Sandboxed file ops
│   │   └── Cargo.toml
│   │
│   ├── providers/                   # ✅ LLM provider abstraction
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── traits.rs            # Provider trait
│   │   │   └── openai_compatible.rs # NVIDIA/Google/local
│   │   └── Cargo.toml
│   │
│   └── interfaces/                  # ✅ Interface abstraction
│       ├── src/
│       │   ├── lib.rs
│       │   ├── traits.rs            # Interface trait
│       │   └── terminal.rs          # Terminal implementation
│       └── Cargo.toml
│
├── hypr-claw-app/                   # Binary entrypoint (to be updated)
│   ├── src/
│   │   ├── main.rs
│   │   ├── config.rs
│   │   └── bootstrap.rs
│   ├── tests/
│   └── Cargo.toml
│
├── hypr-claw-runtime/               # LEGACY: Will migrate to crates/core
│   ├── src/
│   │   ├── agent_loop.rs
│   │   ├── llm_client.rs
│   │   ├── compactor.rs
│   │   └── ...
│   └── Cargo.toml
│
├── hypr-claw-tools/                 # LEGACY: Will migrate to crates/tools
│   ├── src/
│   │   ├── dispatcher.rs
│   │   ├── registry.rs
│   │   └── tools/
│   └── Cargo.toml
│
├── hypr-claw-infra/                 # LEGACY: Will migrate to crates/memory
│   ├── src/
│   │   └── infra/
│   │       ├── session_store.rs
│   │       ├── lock_manager.rs
│   │       └── ...
│   └── Cargo.toml
│
├── data/                            # Runtime data
│   ├── context/                     # ✅ NEW: Persistent memory
│   │   └── <session_id>.json
│   ├── agents/                      # LEGACY: Will become souls/
│   │   ├── default.yaml
│   │   └── default_soul.md
│   ├── sessions/                    # LEGACY: Session logs
│   │   └── <agent>:<user>.jsonl
│   ├── credentials/                 # Encrypted credentials
│   │   └── *.enc
│   ├── config.yaml                  # System config
│   ├── audit.log                    # Audit trail
│   └── .master_key                  # Encryption key
│
├── souls/                           # FUTURE: Soul configurations
│   ├── safe_assistant.yaml
│   ├── system_admin.yaml
│   ├── automation_agent.yaml
│   └── research_agent.yaml
│
├── sandbox/                         # Sandboxed file operations
│
├── docs/                            # Documentation
│   ├── ARCHITECTURE.md              # ✅ System design
│   ├── PHASE_1_3_COMPLETE.md        # ✅ Implementation summary
│   ├── ROADMAP.md                   # ✅ Project roadmap
│   ├── AGENT_LOOP.md                # TODO: Execution flow
│   ├── MEMORY_SYSTEM.md             # TODO: Context management
│   ├── SECURITY_MODEL.md            # TODO: Permission system
│   └── TOOL_DEVELOPMENT.md          # TODO: Creating tools
│
├── Cargo.toml                       # Workspace manifest
├── Cargo.lock
├── README.md
├── LICENSE
└── .gitignore
```

## Data Flow

```
User Input
    │
    ▼
./data/context/<session_id>.json     # Load persistent context
    │
    ▼
crates/executor/environment.rs       # Capture system state
    │
    ▼
crates/core/agent_engine.rs          # Execute task
    │
    ├─► crates/providers/             # Generate LLM response
    │       │
    │       ▼
    │   Tool calls?
    │       │
    │       ├─► crates/policy/        # Check permissions
    │       │       │
    │       │       ▼
    │       │   crates/tools/         # Execute tool
    │       │       │
    │       │       ▼
    │       │   crates/memory/        # Update context
    │       │
    │       └─► Complete?
    │               │
    │               └─► Return result
    │
    ▼
./data/context/<session_id>.json     # Save persistent context
    │
    ▼
crates/interfaces/terminal.rs        # Output to user
```

## Migration Path

### Phase 1-3 (Complete)
- ✅ Created new crate structure
- ✅ Implemented core components
- ✅ All new crates compile and test

### Phase 4-6 (Next)
- Update `hypr-claw-app` to use new crates
- Migrate remaining logic from legacy crates
- Create `./souls/` directory

### Phase 7-9 (Future)
- Deprecate legacy crates
- Complete migration
- Remove old code

### Phase 10+ (Production)
- Widget integration
- Telegram integration
- Distributed architecture

## Key Directories

### `./crates/` - New Architecture
All new code goes here. Clean, layered, testable.

### `./data/context/` - Persistent Memory
Session context stored as JSON. Survives restarts.

### `./souls/` - Soul Configurations
Agent personalities and permissions. YAML format.

### `./sandbox/` - Safe File Operations
All file tools operate within this directory.

### Legacy Crates
`hypr-claw-runtime`, `hypr-claw-tools`, `hypr-claw-infra` remain functional during migration.

## File Naming Conventions

- Crates: `hypr-claw-<name>`
- Modules: `snake_case`
- Types: `PascalCase`
- Functions: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`

## Testing Structure

```
crates/<name>/
├── src/
│   └── *.rs              # Implementation + inline tests
└── tests/
    └── *.rs              # Integration tests
```

Run tests:
```bash
cargo test --package hypr-claw-<name>
```

## Build Artifacts

```
target/
├── debug/
│   └── hypr-claw         # Debug binary
└── release/
    └── hypr-claw         # Release binary
```

Build:
```bash
cargo build --release
```

## Configuration Files

- `Cargo.toml` - Workspace manifest
- `./data/config.yaml` - System configuration
- `./souls/*.yaml` - Soul configurations
- `./.gitignore` - Git exclusions

## Environment Variables

- `RUST_LOG` - Logging level (error/warn/info/debug/trace)
- `HYPR_CLAW_DATA_DIR` - Override data directory
- `HYPR_CLAW_SANDBOX_DIR` - Override sandbox directory

## Security Boundaries

```
User Input
    │
    ▼
Interface Layer          # Input validation
    │
    ▼
Policy Layer            # Permission checks
    │
    ▼
Tool Layer              # Schema validation
    │
    ▼
Executor Layer          # Command whitelist
    │
    ▼
Sandbox                 # Path restrictions
```

## Future Additions

- `./widgets/` - GTK/Qt UI components
- `./telegram/` - Telegram bot implementation
- `./distributed/` - Multi-device coordination
- `./plugins/` - Third-party tool plugins
