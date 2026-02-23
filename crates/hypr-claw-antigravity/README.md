# Hypr-Claw Antigravity Integration

Rust implementation of Google Antigravity and Gemini CLI API access, extracted from [opencode-antigravity-auth](https://github.com/NoeFabris/opencode-antigravity-auth).

## Overview

This crate provides:
- **Google OAuth with PKCE** - Secure authentication flow
- **Dual Quota System** - Access both Antigravity and Gemini CLI quotas
- **Multi-Account Management** - Automatic rotation on rate limits
- **Request Transformation** - Schema cleaning, thinking config injection
- **Fingerprint Generation** - Rate limit mitigation

## Key Constants (Extracted from TypeScript)

```rust
// OAuth Configuration
const ANTIGRAVITY_CLIENT_ID: &str = "<REDACTED>";
const ANTIGRAVITY_CLIENT_SECRET: &str = "<REDACTED>";
const ANTIGRAVITY_REDIRECT_URI: &str = "http://localhost:51121/oauth-callback";

// API Endpoints
const ANTIGRAVITY_ENDPOINT: &str = "https://daily-cloudcode-pa.sandbox.googleapis.com";
const GEMINI_CLI_ENDPOINT: &str = "https://cloudcode-pa.googleapis.com";

// Scopes
const ANTIGRAVITY_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/cloud-platform",
    "https://www.googleapis.com/auth/userinfo.email",
    "https://www.googleapis.com/auth/userinfo.profile",
    "https://www.googleapis.com/auth/cclog",
    "https://www.googleapis.com/auth/experimentsandconfigs",
];
```

## API Request Format

### Antigravity API

**Endpoint:** `https://daily-cloudcode-pa.sandbox.googleapis.com/v1/chat:generateContent`

**Headers:**
```
Authorization: Bearer <access_token>
User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Antigravity/1.18.3 Chrome/138.0.7204.235 Electron/37.3.1 Safari/537.36
X-Goog-Api-Client: google-cloud-sdk vscode_cloudshelleditor/0.1
Client-Metadata: {"ideType":"ANTIGRAVITY","platform":"WINDOWS","pluginType":"GEMINI"}
```

**Body:**
```json
{
  "model": "claude-opus-4-6-thinking",
  "messages": [...],
  "thinkingConfig": {
    "thinkingBudget": 16384
  }
}
```

### Gemini CLI API

**Endpoint:** `https://cloudcode-pa.googleapis.com/v1/chat:generateContent`

**Headers:**
```
Authorization: Bearer <access_token>
User-Agent: google-api-nodejs-client/9.15.1
X-Goog-Api-Client: gl-node/22.17.0
Client-Metadata: ideType=IDE_UNSPECIFIED,platform=PLATFORM_UNSPECIFIED,pluginType=GEMINI
```

**Body:**
```json
{
  "model": "gemini-3-flash-preview",
  "messages": [...],
  "thinkingConfig": {
    "thinkingLevel": "high"
  }
}
```

## Model Name Mappings

### Antigravity Models (with prefix)
- `antigravity-gemini-3-flash` → `gemini-3-flash` (Antigravity quota)
- `antigravity-gemini-3-pro-low` → `gemini-3-pro-low` (Antigravity quota)
- `antigravity-claude-opus-4-6-thinking` → `claude-opus-4-6-thinking` (Antigravity quota)

### Gemini CLI Models (no prefix)
- `gemini-3-flash-preview` → `gemini-3-flash` (Gemini CLI quota)
- `gemini-3-pro-preview` → `gemini-3-pro` (Gemini CLI quota)

### Thinking Tiers
- `-low` → 8192 tokens (Claude/Gemini 2.5 Pro), 6144 (Gemini 2.5 Flash)
- `-medium` → 16384 tokens (Claude/Gemini 2.5 Pro), 12288 (Gemini 2.5 Flash)
- `-high` → 32768 tokens (Claude/Gemini 2.5 Pro), 24576 (Gemini 2.5 Flash)

For Gemini 3 models, tiers map to `thinkingLevel` strings instead of numeric budgets.

## Usage

### 1. OAuth Authentication

```rust
use hypr_claw_antigravity::oauth;

// Generate authorization URL
let auth = oauth::authorize_antigravity(None).await?;
println!("Open: {}", auth.url);

// Exchange code for tokens (after user authorizes)
let result = oauth::exchange_antigravity(&code, &state).await?;
println!("Refresh token: {}", result.refresh);
println!("Access token: {}", result.access);
println!("Project ID: {}", result.project_id);
```

### 2. Initialize Client

```rust
use hypr_claw_antigravity::AntigravityClient;
use std::path::PathBuf;

let storage_path = PathBuf::from("./data/antigravity-accounts.json");
let mut client = AntigravityClient::new(storage_path).await?;

// Add account
client.add_account(
    Some("user@gmail.com".to_string()),
    result.refresh,
    result.project_id,
).await?;
```

### 3. Make Requests

```rust
use hypr_claw_antigravity::api_client::{ChatRequest, Message};

// Use Claude via Antigravity
let request = ChatRequest {
    model: "antigravity-claude-opus-4-6-thinking-medium".to_string(),
    messages: vec![
        Message {
            role: "user".to_string(),
            content: "Explain quantum entanglement".to_string(),
        }
    ],
    tools: None,
    max_tokens: Some(2048),
    temperature: Some(0.7),
};

let response = client.chat(request).await?;
println!("{}", response.choices[0].message.content);

// Use Gemini via CLI quota
let request = ChatRequest {
    model: "gemini-3-flash-preview-high".to_string(),
    messages: vec![
        Message {
            role: "user".to_string(),
            content: "What is Rust?".to_string(),
        }
    ],
    tools: None,
    max_tokens: Some(1024),
    temperature: Some(0.7),
};

let response = client.chat(request).await?;
```

## Dual Quota System

The system automatically routes requests between two quota pools:

1. **Antigravity Quota** (default)
   - Used for models with `antigravity-` prefix
   - Used for Claude models
   - Used for image generation models
   - Endpoint: `https://daily-cloudcode-pa.sandbox.googleapis.com`

2. **Gemini CLI Quota** (fallback)
   - Used for Gemini models without prefix
   - Separate quota pool from Antigravity
   - Endpoint: `https://cloudcode-pa.googleapis.com`

When one quota is exhausted (429 error), the system automatically:
1. Marks the current account as rate-limited for that quota
2. Rotates to the next available account
3. Retries the request

## Account Storage Format

```json
{
  "accounts": [
    {
      "email": "user@gmail.com",
      "refresh_token": "1//...",
      "project_id": "rising-fact-p41fc",
      "access_token": "ya29...",
      "expires": 1708704000000,
      "enabled": true,
      "added_at": 1708700000000,
      "last_used": 1708703000000,
      "fingerprint": {
        "deviceId": "uuid-v4",
        "sessionToken": "hex-string",
        "userAgent": "Mozilla/5.0...",
        "apiClient": "google-cloud-sdk...",
        "clientMetadata": {
          "ideType": "ANTIGRAVITY",
          "platform": "WINDOWS",
          "pluginType": "GEMINI"
        },
        "createdAt": 1708700000000
      },
      "rate_limit_reset_times": {
        "claude": 1708704000000,
        "gemini-antigravity": 0,
        "gemini-cli": 0
      }
    }
  ]
}
```

## Request Transformation

### 1. Schema Cleaning

Removes unsupported JSON schema keywords:
- `$schema`, `$defs`, `definitions`
- `$ref`, `const`, `default`, `examples`
- `additionalProperties`, `minLength`, `maxLength`
- `pattern`, `format`, `minItems`, `maxItems`

```rust
use hypr_claw_antigravity::request_transform::clean_json_schema;

let mut schema = serde_json::json!({
    "type": "object",
    "$schema": "...",
    "properties": {...}
});

clean_json_schema(&mut schema);
// $schema removed, properties preserved
```

### 2. Thinking Config Injection

Adds thinking configuration based on model tier:

```rust
use hypr_claw_antigravity::request_transform::add_thinking_config;

let mut body = serde_json::json!({
    "model": "claude-opus-4-6-thinking",
    "messages": [...]
});

// For Claude/Gemini 2.5: numeric budget
add_thinking_config(&mut body, Some(16384), None);
// Result: { ..., "thinkingConfig": { "thinkingBudget": 16384 } }

// For Gemini 3: level string
add_thinking_config(&mut body, None, Some("high"));
// Result: { ..., "thinkingConfig": { "thinkingLevel": "high" } }
```

### 3. Thinking Block Stripping

Removes internal thinking blocks from Claude responses:

```rust
use hypr_claw_antigravity::request_transform::strip_thinking_blocks;

let mut content = serde_json::json!([
    {"type": "thinking", "thinking": "internal thoughts"},
    {"type": "text", "text": "visible response"}
]);

strip_thinking_blocks(&mut content);
// Only text block remains
```

## Rate Limit Handling

### Backoff Strategy

```rust
// Extracted from accounts.ts parseRateLimitReason()
match error_reason {
    "QUOTA_EXHAUSTED" => {
        // Exponential backoff: 1min, 5min, 30min, 2hr
        let backoffs = [60_000, 300_000, 1_800_000, 7_200_000];
        backoffs[consecutive_failures.min(3)]
    }
    "RATE_LIMIT_EXCEEDED" => 30_000,  // 30 seconds
    "MODEL_CAPACITY_EXHAUSTED" => 45_000 + jitter,  // 45s + jitter
    "SERVER_ERROR" => 20_000,  // 20 seconds
    _ => 60_000,  // 1 minute default
}
```

### Account Rotation

```rust
// When rate limited:
client.mark_rate_limited("claude", backoff_ms).await?;
// 1. Marks current account as rate-limited
// 2. Rotates to next available account
// 3. Next request uses new account
```

## Fingerprint Generation

Each account gets a unique device fingerprint to distribute API usage:

```rust
use hypr_claw_antigravity::fingerprint::generate_fingerprint;

let fingerprint = generate_fingerprint();
// Generates:
// - Random device ID (UUID v4)
// - Random session token (16 bytes hex)
// - Randomized User-Agent
// - Randomized API client version
// - Randomized platform (Windows/macOS)
```

## Module Structure

```
hypr-claw-antigravity/
├── src/
│   ├── lib.rs                  # Public API
│   ├── oauth.rs                # OAuth flow with PKCE
│   ├── api_client.rs           # HTTP client for both APIs
│   ├── accounts.rs             # Multi-account manager
│   ├── models.rs               # Model resolution and routing
│   ├── request_transform.rs   # Request/response transformation
│   └── fingerprint.rs          # Device fingerprint generation
├── examples/
│   └── basic_usage.rs          # Complete working example
└── Cargo.toml
```

## Key Differences from TypeScript Implementation

**Simplified:**
- No OpenCode plugin system integration
- No session recovery hooks
- No TUI/toast notifications
- No debug logging infrastructure
- No auto-update checker

**Preserved:**
- OAuth flow with PKCE
- Dual quota routing logic
- Account rotation on rate limits
- Request transformation (schema cleaning, thinking config)
- Fingerprint generation
- Model name resolution

## Testing

```bash
# Run example
cargo run --example basic_usage

# Run tests
cargo test

# Build
cargo build --release
```

## Integration with Hypr-Claw

Add to your agent's provider configuration:

```rust
use hypr_claw_antigravity::AntigravityClient;

// In your LLM provider enum:
pub enum LlmProvider {
    Nvidia(NvidiaClient),
    Google(GoogleClient),
    Antigravity(AntigravityClient),  // Add this
}

// In your request handler:
match provider {
    LlmProvider::Antigravity(client) => {
        client.chat(request).await?
    }
    // ... other providers
}
```

## References

- [opencode-antigravity-auth](https://github.com/NoeFabris/opencode-antigravity-auth) - Original TypeScript implementation
- [Google OAuth 2.0](https://developers.google.com/identity/protocols/oauth2)
- [PKCE RFC 7636](https://tools.ietf.org/html/rfc7636)

## License

Same as Hypr-Claw project.
