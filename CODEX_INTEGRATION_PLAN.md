# Codex Integration Plan for Hypr-Claw (REVISED)

**Goal:** Integrate OpenAI Codex OAuth provider into Hypr-Claw's agent runtime workflow

---

## Type System Analysis

### Runtime Types (`hypr-claw-runtime`)
```rust
// Message format
pub struct Message {
    pub schema_version: u32,
    pub role: Role,              // Enum: User, Assistant, Tool, System
    pub content: serde_json::Value,  // JSON value, not string
    pub metadata: Option<serde_json::Value>,
}

pub enum Role {
    User, Assistant, Tool, System
}

// Response format
pub enum LLMResponse {
    Final { schema_version: u32, content: String },
    ToolCall { schema_version: u32, tool_name: String, input: serde_json::Value },
}

// LLMClient interface
impl LLMClient {
    pub async fn call(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: &[String],
    ) -> Result<LLMResponse, RuntimeError>
}
```

### Provider Types (`hypr-claw-providers`)
```rust
// Message format
pub struct Message {
    pub role: String,        // String, not enum
    pub content: String,     // String, not JSON
}

// Response format
pub struct GenerateResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: String,
}

pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

// LLMProvider trait
#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn generate(
        &self,
        messages: &[Message],
        tools: Option<&[serde_json::Value]>,
    ) -> Result<GenerateResponse, ProviderError>;
}
```

### Key Differences
1. **Message.role**: `Role` enum vs `String`
2. **Message.content**: `serde_json::Value` vs `String`
3. **Response format**: `LLMResponse` enum vs `GenerateResponse` struct
4. **Tool format**: `Vec<String>` vs `Option<&[serde_json::Value]>`
5. **Error types**: `RuntimeError` vs `ProviderError`

---

## REVISED Integration Strategy

### Module 1: Codex Adapter (CRITICAL)
**Location:** `hypr-claw-runtime/src/codex_adapter.rs`

**Purpose:** Bridge type systems between runtime and providers crate

```rust
use crate::types::{LLMResponse, Message as RuntimeMessage, Role, SCHEMA_VERSION};
use crate::interfaces::RuntimeError;
use hypr_claw_providers::{CodexProvider, LLMProvider};
use hypr_claw_providers::traits::{Message as ProviderMessage, GenerateResponse};
use hypr_claw_memory::types::OAuthTokens;
use std::sync::Arc;

pub struct CodexAdapter {
    provider: Arc<CodexProvider>,
}

impl CodexAdapter {
    /// Create adapter with existing tokens
    pub async fn new(model: String, tokens: Option<OAuthTokens>) -> Result<Self, RuntimeError> {
        let provider = CodexProvider::new(model);
        
        if let Some(tokens) = tokens {
            let codex_tokens = hypr_claw_providers::codex::types::OAuthTokens {
                access_token: tokens.access_token,
                refresh_token: tokens.refresh_token,
                expires_at: tokens.expires_at,
                account_id: tokens.account_id,
            };
            provider.restore_tokens(codex_tokens).await;
        }
        
        Ok(Self {
            provider: Arc::new(provider),
        })
    }
    
    /// Run OAuth flow and return tokens
    pub async fn authenticate(model: String) -> Result<OAuthTokens, RuntimeError> {
        let provider = CodexProvider::new(model);
        let tokens = provider.authenticate().await
            .map_err(|e| RuntimeError::LLMError(e.to_string()))?;
        
        Ok(OAuthTokens {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_at: tokens.expires_at,
            account_id: tokens.account_id,
        })
    }
    
    /// Get current tokens (for persistence)
    pub async fn get_tokens(&self) -> Option<OAuthTokens> {
        self.provider.get_tokens().await.map(|t| OAuthTokens {
            access_token: t.access_token,
            refresh_token: t.refresh_token,
            expires_at: t.expires_at,
            account_id: t.account_id,
        })
    }
    
    /// Call LLM with runtime types
    pub async fn call(
        &self,
        system_prompt: &str,
        messages: &[RuntimeMessage],
        _tools: &[String],  // Codex doesn't use tool names, uses JSON schemas
    ) -> Result<LLMResponse, RuntimeError> {
        // Convert runtime messages to provider messages
        let provider_messages = self.convert_messages(system_prompt, messages)?;
        
        // Call provider
        let response = self.provider
            .generate(&provider_messages, None)
            .await
            .map_err(|e| RuntimeError::LLMError(e.to_string()))?;
        
        // Convert response back to runtime format
        self.convert_response(response)
    }
    
    fn convert_messages(
        &self,
        system_prompt: &str,
        messages: &[RuntimeMessage],
    ) -> Result<Vec<ProviderMessage>, RuntimeError> {
        let mut provider_messages = Vec::new();
        
        // Add system prompt as first message if present
        if !system_prompt.is_empty() {
            provider_messages.push(ProviderMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            });
        }
        
        // Convert runtime messages
        for msg in messages {
            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "tool",
                Role::System => "system",
            }.to_string();
            
            // Convert JSON content to string
            let content = match &msg.content {
                serde_json::Value::String(s) => s.clone(),
                other => serde_json::to_string(other)
                    .map_err(|e| RuntimeError::LLMError(format!("Failed to serialize content: {}", e)))?,
            };
            
            provider_messages.push(ProviderMessage { role, content });
        }
        
        Ok(provider_messages)
    }
    
    fn convert_response(&self, response: GenerateResponse) -> Result<LLMResponse, RuntimeError> {
        // Check if there are tool calls
        if !response.tool_calls.is_empty() {
            let tool_call = &response.tool_calls[0];
            Ok(LLMResponse::ToolCall {
                schema_version: SCHEMA_VERSION,
                tool_name: tool_call.name.clone(),
                input: tool_call.arguments.clone(),
            })
        } else {
            // Final response
            Ok(LLMResponse::Final {
                schema_version: SCHEMA_VERSION,
                content: response.content.unwrap_or_default(),
            })
        }
    }
}
```

**Key Changes from Original Plan:**
- Proper type conversions for `Message` (role enum → string, JSON → string)
- Proper type conversions for `LLMResponse` (enum variants)
- System prompt handling (prepend as system message)
- Error type conversions (`ProviderError` → `RuntimeError`)

---

### Module 2: Runtime Integration
**Location:** `hypr-claw-runtime/src/lib.rs`

```rust
pub mod codex_adapter;
pub use codex_adapter::CodexAdapter;
```

**Dependencies to add:**
```toml
# hypr-claw-runtime/Cargo.toml
[dependencies]
hypr-claw-providers = { path = "../providers" }
hypr-claw-memory = { path = "../memory" }
```

---

### Module 3: Config (Already Correct ✅)
**Location:** `hypr-claw-app/src/config.rs`

```rust
pub enum LLMProvider {
    Codex,  // Add this
}
```

---

### Module 4: Bootstrap (Already Correct ✅)
**Location:** `hypr-claw-app/src/bootstrap.rs`

```rust
fn bootstrap_codex() -> Result<Config> {
    // Get model
    print!("\nEnter model [gpt-5.1-codex]: ");
    let model = read_user_input()?;
    let model = if model.is_empty() { "gpt-5.1-codex" } else { model };
    
    Config {
        provider: LLMProvider::Codex,
        model: model.to_string(),
    }.save()?;
    
    Ok(config)
}
```

---

### Module 5: Main App Integration (REVISED)
**Location:** `hypr-claw-app/src/main.rs`

```rust
use hypr_claw_runtime::CodexAdapter;
use hypr_claw_memory::ContextManager;

// In main() after config loading:
let llm_client = match &config.provider {
    LLMProvider::Codex => {
        // Load context to check for tokens
        let context_manager = ContextManager::new("./data/context");
        let session_id = format!("{}_{}", user_id, agent_name);
        let mut context = match context_manager.load(&session_id) {
            Ok(ctx) => ctx,
            Err(_) => ContextData::default(),
        };
        
        // Check for existing tokens
        let adapter = if let Some(tokens) = &context.oauth_tokens {
            println!("[Codex] Restoring tokens from memory...");
            println!("[Codex] Account ID: {}", tokens.account_id);
            CodexAdapter::new(config.model.clone(), Some(tokens.clone())).await?
        } else {
            println!("[Codex] Starting OAuth authentication...");
            let tokens = CodexAdapter::authenticate(config.model.clone()).await?;
            println!("[Codex] Authentication successful!");
            println!("[Codex] Account ID: {}", tokens.account_id);
            
            // Save tokens
            context.oauth_tokens = Some(tokens.clone());
            context_manager.save(&session_id, &context)?;
            
            CodexAdapter::new(config.model.clone(), Some(tokens)).await?
        };
        
        // Wrap adapter to match LLMClient interface
        CodexLLMClientWrapper::new(adapter)
    }
    // ... other providers
};
```

**Problem:** `AgentLoop` expects `LLMClient`, not `CodexAdapter`

**Solution:** Create a wrapper that implements the same interface:

```rust
// In hypr-claw-runtime/src/codex_adapter.rs

pub struct CodexLLMClientWrapper {
    adapter: CodexAdapter,
}

impl CodexLLMClientWrapper {
    pub fn new(adapter: CodexAdapter) -> Self {
        Self { adapter }
    }
    
    /// Same signature as LLMClient::call
    pub async fn call(
        &self,
        system_prompt: &str,
        messages: &[RuntimeMessage],
        tools: &[String],
    ) -> Result<LLMResponse, RuntimeError> {
        self.adapter.call(system_prompt, messages, tools).await
    }
}
```

**Better Solution:** Make `AgentLoop` generic over client type or use enum:

```rust
// In hypr-claw-runtime/src/agent_loop.rs

pub enum LLMClientType {
    Standard(LLMClient),
    Codex(CodexAdapter),
}

impl LLMClientType {
    pub async fn call(
        &self,
        system_prompt: &str,
        messages: &[Message],
        tools: &[String],
    ) -> Result<LLMResponse, RuntimeError> {
        match self {
            Self::Standard(client) => client.call(system_prompt, messages, tools).await,
            Self::Codex(adapter) => adapter.call(system_prompt, messages, tools).await,
        }
    }
}

// Update AgentLoop to use LLMClientType instead of LLMClient
pub struct AgentLoop<S, L, D, R, Sum> {
    // ...
    llm_client: LLMClientType,  // Changed from LLMClient
    // ...
}
```

---

## REVISED Implementation Order

### Phase 1: Adapter (3-4 hours)
1. Create `codex_adapter.rs` with type conversions
2. Add dependencies to runtime
3. Test adapter standalone
4. Verify type conversions work

### Phase 2: Runtime Integration (2-3 hours)
5. Add `LLMClientType` enum to agent_loop
6. Update `AgentLoop` to use enum
7. Test with mock responses

### Phase 3: App Integration (2-3 hours)
8. Update config enum
9. Add bootstrap function
10. Update main.rs initialization
11. Test OAuth flow

### Phase 4: Testing (2-3 hours)
12. Test full workflow
13. Test token persistence
14. Test token refresh
15. Test error scenarios

**Total: 9-13 hours**

---

## Critical Type Conversion Points

### 1. Message Conversion (Runtime → Provider)
```rust
RuntimeMessage {
    role: Role::User,
    content: json!("Hello"),
} 
→ 
ProviderMessage {
    role: "user",
    content: "Hello",
}
```

### 2. Response Conversion (Provider → Runtime)
```rust
GenerateResponse {
    content: Some("Hi"),
    tool_calls: vec![],
}
→
LLMResponse::Final {
    content: "Hi",
}

GenerateResponse {
    tool_calls: vec![ToolCall { name: "echo", arguments: json!({...}) }],
}
→
LLMResponse::ToolCall {
    tool_name: "echo",
    input: json!({...}),
}
```

### 3. Token Conversion (Bidirectional)
```rust
memory::OAuthTokens ↔ providers::codex::types::OAuthTokens
```

---

## Testing Checklist

- [ ] Type conversions work correctly
- [ ] OAuth flow completes
- [ ] Tokens persist to context
- [ ] Tokens restore on restart
- [ ] Agent completes simple task
- [ ] Agent completes multi-step task
- [ ] Tool calls work
- [ ] Error handling works
- [ ] Token refresh works
- [ ] No regressions in other providers

---

**Plan Status:** ✅ REVISED AND CONFIRMED

**Key Insight:** The adapter is the critical piece - it must handle all type conversions between the runtime's structured types and the provider's simpler types.

**Ready to implement?**


---

## Architecture Analysis

### Current Hypr-Claw Flow
```
User Input → Bootstrap/Config → LLMClient (HTTP) → Agent Loop → Tools → Memory
```

### Current LLM Integration Points
1. **Config Layer** (`hypr-claw-app/src/config.rs`)
   - `LLMProvider` enum defines available providers
   - Stores provider type and model in `config.yaml`

2. **Bootstrap Layer** (`hypr-claw-app/src/bootstrap.rs`)
   - Provider-specific setup (API keys, OAuth)
   - Credential storage via `CredentialStore`

3. **Runtime Layer** (`hypr-claw-runtime/src/llm_client.rs`)
   - `LLMClient` makes HTTP requests to LLM endpoints
   - Uses OpenAI-compatible format
   - Circuit breaker for reliability

4. **Main App** (`hypr-claw-app/src/main.rs`)
   - Initializes `LLMClient` based on config
   - Passes to `AgentLoop` via `RuntimeController`

### Codex Provider Architecture
```
CodexProvider (providers crate)
├── OAuth Flow (PKCE, tokens)
├── Token Management (refresh, persistence)
├── Request Transformation (Codex API format)
└── LLMProvider trait implementation
```

---

## Integration Challenge

**Problem:** Hypr-Claw runtime uses `LLMClient` (HTTP-based), but Codex is implemented as `CodexProvider` (trait-based in providers crate).

**Solution:** Create an adapter layer that bridges `CodexProvider` → `LLMClient` interface.

---

## Modular Implementation Plan

### **Module 1: Runtime Adapter** (2-3 hours)
**Location:** `hypr-claw-runtime/src/codex_adapter.rs`

**Purpose:** Bridge between `CodexProvider` and runtime's `LLMClient` interface

**Components:**
```rust
pub struct CodexLLMClient {
    provider: Arc<CodexProvider>,
    model: String,
}

impl CodexLLMClient {
    pub async fn new(model: String, tokens: Option<OAuthTokens>) -> Result<Self>
    pub async fn authenticate() -> Result<OAuthTokens>
}

// Implement same interface as LLMClient
impl CodexLLMClient {
    pub async fn generate(&self, system_prompt: String, messages: Vec<Message>, tools: Vec<String>) -> Result<LLMResponse>
}
```

**Key Features:**
- Wraps `CodexProvider` from providers crate
- Converts runtime's `Message` format to providers' format
- Handles OAuth token lifecycle
- Maintains same error handling as `LLMClient`

---

### **Module 2: Config Integration** (30 min)
**Location:** `hypr-claw-app/src/config.rs`

**Changes:**
```rust
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LLMProvider {
    Nvidia,
    Google,
    Local { base_url: String },
    Antigravity,
    GeminiCli,
    Codex,  // ⭐ NEW
}

impl LLMProvider {
    pub fn requires_oauth(&self) -> bool {
        matches!(self, LLMProvider::Antigravity | LLMProvider::GeminiCli | LLMProvider::Codex)
    }
}
```

---

### **Module 3: Bootstrap Integration** (1-2 hours)
**Location:** `hypr-claw-app/src/bootstrap.rs`

**New Function:**
```rust
fn bootstrap_codex() -> Result<Config> {
    println!("\n🔐 OpenAI Codex OAuth Setup");
    println!("This will authenticate with your ChatGPT Plus/Pro account.");
    
    // Get model choice
    print!("\nEnter model [gpt-5.1-codex]: ");
    let model = read_user_input()?;
    let model = if model.is_empty() { "gpt-5.1-codex" } else { model };
    
    let config = Config {
        provider: LLMProvider::Codex,
        model: model.to_string(),
    };
    
    config.save()?;
    println!("✅ Codex provider configured");
    println!("💡 OAuth flow will run on first use");
    
    Ok(config)
}
```

**Integration:**
- Add to provider selection menu (option 6)
- OAuth happens on first agent run (lazy initialization)

---

### **Module 4: Memory Integration** (Already Done ✅)
**Location:** `crates/memory/src/types.rs`

**Status:** OAuth tokens field already added to `ContextData`

```rust
pub struct ContextData {
    // ... existing fields
    pub oauth_tokens: Option<OAuthTokens>,  // ✅ Already added
}
```

---

### **Module 5: Main App Integration** (2-3 hours)
**Location:** `hypr-claw-app/src/main.rs`

**New Initialization Path:**
```rust
LLMProvider::Codex => {
    // Load context to check for existing tokens
    let context_manager = ContextManager::new("./data/context");
    let session_id = format!("{}_{}", user_id, agent_name);
    let mut context = context_manager.load(&session_id)?;
    
    // Check if we have tokens
    if let Some(tokens) = &context.oauth_tokens {
        println!("[Codex] Restoring tokens from memory...");
        let client = CodexLLMClient::new(config.model.clone(), Some(tokens.clone())).await?;
        println!("[Codex] Account ID: {}", tokens.account_id);
        client
    } else {
        println!("[Codex] No stored tokens. Starting OAuth flow...");
        let tokens = CodexLLMClient::authenticate().await?;
        
        // Store tokens in context
        context.oauth_tokens = Some(tokens.clone());
        context_manager.save(&session_id, &context)?;
        
        let client = CodexLLMClient::new(config.model.clone(), Some(tokens)).await?;
        println!("[Codex] Authentication successful!");
        client
    }
}
```

---

### **Module 6: Runtime Controller Integration** (1 hour)
**Location:** `hypr-claw-runtime/src/runtime_controller.rs`

**Changes:**
- Accept either `LLMClient` or `CodexLLMClient`
- Use enum or trait object for flexibility

**Option A: Enum Wrapper**
```rust
pub enum LLMClientType {
    Standard(LLMClient),
    Codex(CodexLLMClient),
}

impl LLMClientType {
    pub async fn generate(&self, ...) -> Result<LLMResponse> {
        match self {
            Self::Standard(client) => client.generate(...).await,
            Self::Codex(client) => client.generate(...).await,
        }
    }
}
```

**Option B: Trait Object** (cleaner)
```rust
pub trait LLMClientTrait {
    async fn generate(&self, ...) -> Result<LLMResponse>;
}

impl LLMClientTrait for LLMClient { ... }
impl LLMClientTrait for CodexLLMClient { ... }

// Use Box<dyn LLMClientTrait> in RuntimeController
```

---

### **Module 7: Error Handling & Token Refresh** (1-2 hours)
**Location:** Throughout integration

**Key Scenarios:**
1. **Token Expired During Run**
   - Detect 401 errors
   - Attempt automatic refresh
   - If refresh fails, prompt user to re-authenticate
   - Save new tokens to context

2. **OAuth Flow Interrupted**
   - Timeout after 5 minutes
   - Clear partial state
   - Allow retry

3. **Network Errors**
   - Use existing circuit breaker
   - Retry with exponential backoff

**Implementation:**
```rust
impl CodexLLMClient {
    async fn generate_with_retry(&self, ...) -> Result<LLMResponse> {
        match self.provider.generate(...).await {
            Ok(response) => Ok(response),
            Err(ProviderError::Api(msg)) if msg.contains("401") => {
                // Token expired, try refresh
                self.refresh_token().await?;
                self.provider.generate(...).await
            }
            Err(e) => Err(e.into()),
        }
    }
}
```

---

### **Module 8: Soul Configuration** (30 min)
**Location:** `souls/*.yaml`

**Add Codex-specific soul:**
```yaml
# souls/codex_assistant.yaml
id: codex_assistant
system_prompt: |
  You are an advanced coding assistant powered by OpenAI Codex.
  You have access to GPT-5.1 Codex for sophisticated code generation.
  Always explain your reasoning and provide clean, well-documented code.
config:
  allowed_tools:
    - echo
    - file_read
    - file_write
    - file_list
  autonomy_mode: auto
  max_iterations: 20
  risk_tolerance: medium
  verbosity: normal
  llm_provider: codex  # Optional: force Codex for this soul
  codex_model: gpt-5.1-codex-high
```

---

## Implementation Order

### Phase 1: Foundation (3-4 hours)
1. ✅ Create `CodexAdapter` in runtime
2. ✅ Add config enum variant
3. ✅ Add bootstrap function
4. ✅ Test OAuth flow standalone

### Phase 2: Integration (3-4 hours)
5. ✅ Integrate into main.rs initialization
6. ✅ Add to RuntimeController
7. ✅ Test with simple agent task
8. ✅ Verify token persistence

### Phase 3: Polish (2-3 hours)
9. ✅ Add error handling
10. ✅ Add token refresh logic
11. ✅ Create Codex soul
12. ✅ Update documentation

**Total Estimated Time: 8-11 hours**

---

## File Structure

```
hypr-claw/
├── crates/
│   ├── providers/
│   │   └── src/
│   │       └── codex/          # ✅ Already implemented
│   │           ├── mod.rs
│   │           ├── oauth.rs
│   │           ├── server.rs
│   │           ├── transform.rs
│   │           ├── types.rs
│   │           └── constants.rs
│   │
│   ├── memory/
│   │   └── src/
│   │       └── types.rs        # ✅ Already updated
│   │
│   └── hypr-claw-runtime/
│       └── src/
│           ├── codex_adapter.rs  # ⭐ NEW
│           ├── llm_client.rs     # Existing
│           └── lib.rs            # Export adapter
│
└── hypr-claw-app/
    └── src/
        ├── config.rs             # ⭐ UPDATE
        ├── bootstrap.rs          # ⭐ UPDATE
        └── main.rs               # ⭐ UPDATE
```

---

## Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_codex_adapter_message_conversion() { ... }
    
    #[tokio::test]
    async fn test_codex_adapter_token_refresh() { ... }
}
```

### Integration Tests
```bash
# Test OAuth flow
cargo test -p hypr-claw-runtime codex_adapter::tests

# Test full agent workflow
cargo run --release
# Select Codex provider
# Run simple task: "Write a hello world in Rust"
```

### Manual Testing Checklist
- [ ] OAuth flow completes successfully
- [ ] Tokens saved to context
- [ ] Tokens restored on next run
- [ ] Token refresh works
- [ ] Agent can complete multi-step tasks
- [ ] Error handling works (network errors, token expiry)
- [ ] Works with different souls

---

## Dependencies to Add

### `hypr-claw-runtime/Cargo.toml`
```toml
[dependencies]
# Existing dependencies...
hypr-claw-providers = { path = "../providers" }
hypr-claw-memory = { path = "../memory" }
```

### `hypr-claw-app/Cargo.toml`
```toml
# Already has all needed dependencies
```

---

## Key Design Decisions

### 1. **Adapter Pattern**
**Why:** Keeps existing `LLMClient` interface intact, minimizes changes to runtime

### 2. **Lazy OAuth**
**Why:** Don't block bootstrap, authenticate on first use

### 3. **Token in Context**
**Why:** Leverage existing persistence mechanism, no new storage needed

### 4. **Trait Object for Client**
**Why:** Clean abstraction, easy to add more providers later

### 5. **Minimal Runtime Changes**
**Why:** Reduce risk, maintain stability of existing providers

---

## Rollback Plan

If integration fails:
1. Revert config.rs changes
2. Remove bootstrap function
3. Remove main.rs Codex branch
4. Codex still works standalone via example

**Risk:** Low - changes are additive, existing providers unaffected

---

## Success Metrics

✅ OAuth flow completes in < 30 seconds  
✅ Token persistence works across restarts  
✅ Agent completes 10-step coding task  
✅ No regressions in existing providers  
✅ Error messages are clear and actionable  

---

## Next Steps

1. **Review this plan** - Confirm approach
2. **Start Phase 1** - Build adapter
3. **Test incrementally** - Each module independently
4. **Integrate** - Wire everything together
5. **Document** - Update README and docs

---

**Ready to proceed?** Confirm the plan and I'll start implementation.
