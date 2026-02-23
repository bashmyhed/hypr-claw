# OpenAI Codex OAuth Integration

This module implements OAuth 2.0 authentication with PKCE for OpenAI's Codex backend, enabling access to GPT-5.x and Codex models via ChatGPT Plus/Pro subscription.

## Features

- ✅ OAuth 2.0 with PKCE (Proof Key for Code Exchange)
- ✅ Local callback server on port 1455
- ✅ Automatic token refresh
- ✅ Token persistence in agent context
- ✅ Model normalization (strips reasoning suffixes)
- ✅ Request transformation for Codex API
- ✅ Fallback to manual URL paste if port is busy

## Architecture

```
codex/
├── mod.rs          # CodexProvider implementation (LLMProvider trait)
├── oauth.rs        # OAuth flow (PKCE, token exchange, refresh)
├── server.rs       # Local HTTP server for OAuth callback
├── transform.rs    # Request/response transformation
├── types.rs        # Codex-specific types
└── constants.rs    # OAuth constants and API URLs
```

## Usage

### Example: Standalone Usage

```rust
use hypr_claw_providers::{CodexProvider, LLMProvider};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create provider
    let provider = CodexProvider::new("gpt-5.1-codex".to_string());
    
    // Authenticate (opens browser)
    let tokens = provider.authenticate().await?;
    
    // Make a request
    let messages = vec![
        Message {
            role: "user".to_string(),
            content: "Write a Rust function".to_string(),
        },
    ];
    
    let response = provider.generate(&messages, None).await?;
    println!("{}", response.content.unwrap());
    
    Ok(())
}
```

### Example: With Token Persistence

```bash
cargo run --example codex_test -p hypr-claw-providers
```

This example demonstrates:
- OAuth authentication flow
- Token storage in context JSON
- Token restoration on subsequent runs
- Automatic token refresh

## OAuth Flow

1. **Generate PKCE**: Create challenge/verifier pair using SHA-256
2. **Generate State**: Random 32-byte string for CSRF protection
3. **Build Auth URL**: Construct authorization URL with parameters
4. **Start Server**: Listen on `http://127.0.0.1:1455/auth/callback`
5. **Open Browser**: User authenticates with ChatGPT
6. **Receive Callback**: Validate state, extract authorization code
7. **Exchange Code**: POST to token endpoint with code + verifier
8. **Decode JWT**: Extract ChatGPT account ID from access token
9. **Store Tokens**: Save to agent context for persistence
10. **Refresh**: Automatically refresh 5 minutes before expiration

## Model Normalization

The provider automatically normalizes model names:

```
Input Model              → Normalized Model + Reasoning Effort
─────────────────────────────────────────────────────────────
gpt-5.1-codex-high       → gpt-5.1-codex (effort: high)
gpt-5.1-codex-medium     → gpt-5.1-codex (effort: medium)
gpt-5.1-codex-low        → gpt-5.1-codex (effort: low)
gpt-5.1-codex            → gpt-5.1-codex (effort: medium)
gpt-5-codex              → gpt-5.1-codex (legacy mapping)
```

## Request Format

Requests are transformed to Codex API format:

```json
{
  "model": "gpt-5.1-codex",
  "store": false,
  "include": ["reasoning.encrypted_content"],
  "input": [
    {
      "type": "message",
      "role": "user",
      "content": "..."
    }
  ],
  "reasoning": {
    "effort": "medium",
    "summary": "auto"
  },
  "text": {
    "verbosity": "medium"
  },
  "stream": false
}
```

## Required Headers

```
Authorization: Bearer <access_token>
chatgpt-account-id: <account_id>
OpenAI-Beta: responses=experimental
originator: codex_cli_rs
Content-Type: application/json
```

## Token Management

Tokens are stored in `ContextData.oauth_tokens`:

```rust
pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: u64,
    pub account_id: String,
}
```

The provider automatically:
- Checks token expiration before each request
- Refreshes tokens 5 minutes before expiration
- Handles refresh failures gracefully

## Security

- **PKCE**: Prevents authorization code interception
- **State Validation**: Protects against CSRF attacks
- **Localhost Only**: Server listens on `127.0.0.1` (not `0.0.0.0`)
- **Token Encryption**: Tokens stored in encrypted context (when using credential store)
- **HTTPS**: All API requests over HTTPS

## Error Handling

The provider handles:
- Port 1455 busy → Falls back to manual URL paste
- Token expired → Automatic refresh
- Refresh failed → Returns error (user must re-authenticate)
- Network errors → Propagated as `ProviderError::Http`
- API errors → Propagated as `ProviderError::Api`

## Configuration

### Bootstrap

When running `hypr-claw` for the first time:

```
Select provider:
1. NVIDIA Kimi
2. Google Gemini
3. Local model
4. Antigravity (Claude + Gemini via Google OAuth)
5. Gemini CLI (Gemini via Google OAuth)
6. OpenAI Codex (ChatGPT Plus/Pro via OAuth)

Choice [1-6]: 6

Enter model [gpt-5.1-codex]: gpt-5.1-codex-high

✅ Codex provider configured
💡 OAuth flow will run on first use
```

### Soul Configuration

Add to your soul YAML files:

```yaml
id: codex_assistant
system_prompt: |
  You are a coding assistant powered by OpenAI Codex.
config:
  llm_provider: codex
  codex_model: gpt-5.1-codex-high
  allowed_tools: [...]
```

## Troubleshooting

### Port 1455 is busy

The provider automatically falls back to manual URL paste:

```
[Codex] Port 1455 is busy. Please paste the redirect URL:
```

Paste the full URL from your browser after authentication.

### Token refresh fails

If refresh fails, the provider returns an error. User must re-authenticate:

```bash
hypr-claw config reset
hypr-claw
```

### Authentication timeout

OAuth flow times out after 5 minutes. Restart if needed.

## Compliance

**IMPORTANT**: This implementation is for **personal development use** with your own ChatGPT Plus/Pro subscription.

**NOT intended for**:
- Commercial resale
- Multi-user services
- High-volume automated extraction
- Any use violating OpenAI Terms of Service

Users are responsible for compliance with:
- [OpenAI Terms of Use](https://openai.com/policies/terms-of-use/)
- [OpenAI Usage Policies](https://openai.com/policies/usage-policies/)

For production applications, use the [OpenAI Platform API](https://platform.openai.com/).

## References

- **OAuth 2.0 RFC**: https://datatracker.ietf.org/doc/html/rfc6749
- **PKCE RFC**: https://datatracker.ietf.org/doc/html/rfc7636
- **OpenAI Codex CLI**: https://github.com/openai/codex
- **Implementation Guide**: `/home/bigfoot/opencode-openai-codex-auth/OAUTH_IMPLEMENTATION_GUIDE.md`

## Dependencies

```toml
base64 = "0.22"
sha2 = "0.10"
rand = "0.8"
urlencoding = "2.1"
open = "5.0"
reqwest = { version = "0.11", features = ["json"] }
tokio = { version = "1.0", features = ["net", "io-util", "sync"] }
```

## Status

✅ **Phase 1-5 Complete**: Core OAuth infrastructure, server, transformation, provider, memory integration

⏳ **Phase 6-8 Pending**: Runtime integration, configuration, comprehensive testing

The provider is fully functional for standalone use. Integration with the agent runtime requires an adapter layer (see main.rs for current status).
