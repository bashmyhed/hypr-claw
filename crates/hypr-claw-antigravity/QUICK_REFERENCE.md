# Antigravity API Quick Reference

## OAuth Constants

```rust
CLIENT_ID = "<REDACTED>"
CLIENT_SECRET = "<REDACTED>"
REDIRECT_URI = "http://localhost:51121/oauth-callback"

SCOPES = [
    "https://www.googleapis.com/auth/cloud-platform",
    "https://www.googleapis.com/auth/userinfo.email",
    "https://www.googleapis.com/auth/userinfo.profile",
    "https://www.googleapis.com/auth/cclog",
    "https://www.googleapis.com/auth/experimentsandconfigs",
]
```

## API Endpoints

### Antigravity API (Primary)
```
Base: https://daily-cloudcode-pa.sandbox.googleapis.com
Chat: /v1/chat:generateContent
LoadCodeAssist: /v1internal:loadCodeAssist

Fallbacks:
1. https://daily-cloudcode-pa.sandbox.googleapis.com (daily)
2. https://autopush-cloudcode-pa.sandbox.googleapis.com (autopush)
3. https://cloudcode-pa.googleapis.com (prod)
```

### Gemini CLI API (Separate Quota)
```
Base: https://cloudcode-pa.googleapis.com
Chat: /v1/chat:generateContent
```

### OAuth Endpoints
```
Authorization: https://accounts.google.com/o/oauth2/v2/auth
Token Exchange: https://oauth2.googleapis.com/token
User Info: https://www.googleapis.com/oauth2/v1/userinfo?alt=json
```

## Request Headers

### Antigravity Headers
```http
Authorization: Bearer <access_token>
Content-Type: application/json
User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Antigravity/1.18.3 Chrome/138.0.7204.235 Electron/37.3.1 Safari/537.36
X-Goog-Api-Client: google-cloud-sdk vscode_cloudshelleditor/0.1
Client-Metadata: {"ideType":"ANTIGRAVITY","platform":"WINDOWS","pluginType":"GEMINI"}
```

### Gemini CLI Headers
```http
Authorization: Bearer <access_token>
Content-Type: application/json
User-Agent: google-api-nodejs-client/9.15.1
X-Goog-Api-Client: gl-node/22.17.0
Client-Metadata: ideType=IDE_UNSPECIFIED,platform=PLATFORM_UNSPECIFIED,pluginType=GEMINI
```

## Request Body Examples

### Claude via Antigravity
```json
POST https://daily-cloudcode-pa.sandbox.googleapis.com/v1/chat:generateContent

{
  "model": "claude-opus-4-6-thinking",
  "messages": [
    {
      "role": "user",
      "content": "Explain quantum computing"
    }
  ],
  "thinkingConfig": {
    "thinkingBudget": 16384
  },
  "maxTokens": 2048,
  "temperature": 0.7
}
```

### Gemini 3 via Antigravity
```json
POST https://daily-cloudcode-pa.sandbox.googleapis.com/v1/chat:generateContent

{
  "model": "gemini-3-pro-low",
  "messages": [
    {
      "role": "user",
      "content": "What is Rust?"
    }
  ],
  "thinkingConfig": {
    "thinkingLevel": "low"
  },
  "maxTokens": 1024
}
```

### Gemini 3 via CLI (Separate Quota)
```json
POST https://cloudcode-pa.googleapis.com/v1/chat:generateContent

{
  "model": "gemini-3-flash-preview",
  "messages": [
    {
      "role": "user",
      "content": "Summarize this text"
    }
  ],
  "thinkingConfig": {
    "thinkingLevel": "high"
  }
}
```

## Model Name Transformations

### Input → API Model (Antigravity)
```
antigravity-gemini-3-flash → gemini-3-flash
antigravity-gemini-3-pro → gemini-3-pro-low (default tier added)
antigravity-gemini-3-pro-high → gemini-3-pro-high
antigravity-claude-opus-4-6-thinking → claude-opus-4-6-thinking
antigravity-claude-opus-4-6-thinking-medium → claude-opus-4-6-thinking
```

### Input → API Model (Gemini CLI)
```
gemini-3-flash-preview → gemini-3-flash
gemini-3-pro-preview → gemini-3-pro
gemini-3-flash-preview-high → gemini-3-flash
```

### Thinking Tier Mappings

**Claude / Gemini 2.5 Pro:**
```
-low    → thinkingBudget: 8192
-medium → thinkingBudget: 16384
-high   → thinkingBudget: 32768
(none)  → thinkingBudget: 32768 (default for Claude thinking)
```

**Gemini 2.5 Flash:**
```
-low    → thinkingBudget: 6144
-medium → thinkingBudget: 12288
-high   → thinkingBudget: 24576
```

**Gemini 3 (all variants):**
```
-minimal → thinkingLevel: "minimal"
-low     → thinkingLevel: "low"
-medium  → thinkingLevel: "medium"
-high    → thinkingLevel: "high"
(none)   → thinkingLevel: "low" (default)
```

## Rate Limit Response Handling

### 429 Response Structure
```json
{
  "error": {
    "code": 429,
    "message": "Quota exceeded for quota metric...",
    "status": "RESOURCE_EXHAUSTED"
  }
}
```

### Backoff Calculation
```rust
match error_reason {
    "QUOTA_EXHAUSTED" => {
        // Exponential: 1min, 5min, 30min, 2hr
        [60_000, 300_000, 1_800_000, 7_200_000][consecutive_failures]
    }
    "RATE_LIMIT_EXCEEDED" => 30_000,  // 30s
    "MODEL_CAPACITY_EXHAUSTED" => 45_000 + rand(0..30_000),  // 45s + jitter
    "SERVER_ERROR" => 20_000,  // 20s
    _ => 60_000,  // 1min
}
```

### Quota Keys
```
claude                    # All Claude models
gemini-antigravity        # Gemini via Antigravity quota
gemini-cli                # Gemini via CLI quota
gemini-antigravity:model  # Per-model tracking (optional)
gemini-cli:model          # Per-model tracking (optional)
```

## Token Refresh

### Request
```http
POST https://oauth2.googleapis.com/token
Content-Type: application/x-www-form-urlencoded

client_id=<CLIENT_ID>
&client_secret=<CLIENT_SECRET>
&refresh_token=<REFRESH_TOKEN>
&grant_type=refresh_token
```

### Response
```json
{
  "access_token": "ya29.a0...",
  "expires_in": 3599,
  "scope": "https://www.googleapis.com/auth/cloud-platform ...",
  "token_type": "Bearer"
}
```

### Token Storage Format
```
refresh_token|project_id|managed_project_id

Example:
1//0gHZ...abc|rising-fact-p41fc|
```

## Project ID Discovery

### Request
```http
POST https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist
Authorization: Bearer <access_token>
Content-Type: application/json

{
  "metadata": {
    "ideType": "ANTIGRAVITY",
    "platform": "WINDOWS",
    "pluginType": "GEMINI"
  }
}
```

### Response
```json
{
  "cloudaicompanionProject": "rising-fact-p41fc"
}
```

Or:
```json
{
  "cloudaicompanionProject": {
    "id": "rising-fact-p41fc"
  }
}
```

### Fallback Project ID
```
rising-fact-p41fc
```

## Fingerprint Structure

```json
{
  "deviceId": "uuid-v4-string",
  "sessionToken": "32-char-hex-string",
  "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Antigravity/1.18.3 Chrome/138.0.7204.235 Electron/37.3.1 Safari/537.36",
  "apiClient": "google-cloud-sdk vscode_cloudshelleditor/0.1",
  "clientMetadata": {
    "ideType": "ANTIGRAVITY",
    "platform": "WINDOWS",
    "pluginType": "GEMINI"
  },
  "createdAt": 1708700000000
}
```

### Randomization Pools
```rust
PLATFORMS = ["WINDOWS", "MACOS"]

SDK_CLIENTS = [
    "google-cloud-sdk vscode_cloudshelleditor/0.1",
    "google-cloud-sdk vscode/1.86.0",
    "google-cloud-sdk vscode/1.87.0",
    "google-cloud-sdk vscode/1.96.0",
]

ANTIGRAVITY_VERSION = "1.18.3"
```

## Schema Cleaning Rules

### Keywords to Remove
```
$schema, $defs, definitions
$ref, const, default, examples
additionalProperties, propertyNames
title, $id, $comment
minLength, maxLength
exclusiveMinimum, exclusiveMaximum
pattern, format
minItems, maxItems
```

### Example Transformation
```json
// Before
{
  "type": "object",
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$defs": {"Foo": {"type": "string"}},
  "properties": {
    "name": {
      "type": "string",
      "minLength": 1,
      "maxLength": 100,
      "default": "unnamed"
    }
  },
  "additionalProperties": false
}

// After
{
  "type": "object",
  "properties": {
    "name": {
      "type": "string"
    }
  }
}
```

## Error Codes

```
200 OK                    # Success
400 Bad Request           # Invalid request format
401 Unauthorized          # Invalid/expired token
403 Forbidden             # Insufficient permissions
404 Not Found             # Invalid endpoint
429 Too Many Requests     # Rate limit exceeded
500 Internal Server Error # Server error
503 Service Unavailable   # Model capacity exhausted
529 Site Overloaded       # Transient capacity issue
```

## Minimal cURL Examples

### OAuth Authorization
```bash
# Step 1: Generate authorization URL (manual)
https://accounts.google.com/o/oauth2/v2/auth?client_id=<REDACTED>&response_type=code&redirect_uri=http://localhost:51121/oauth-callback&scope=https://www.googleapis.com/auth/cloud-platform%20https://www.googleapis.com/auth/userinfo.email&code_challenge=<CHALLENGE>&code_challenge_method=S256&state=<STATE>&access_type=offline&prompt=consent

# Step 2: Exchange code for tokens
curl -X POST https://oauth2.googleapis.com/token \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "client_id=<REDACTED>" \
  -d "client_secret=<REDACTED>" \
  -d "code=<CODE>" \
  -d "grant_type=authorization_code" \
  -d "redirect_uri=http://localhost:51121/oauth-callback" \
  -d "code_verifier=<VERIFIER>"
```

### Chat Request (Antigravity)
```bash
curl -X POST https://daily-cloudcode-pa.sandbox.googleapis.com/v1/chat:generateContent \
  -H "Authorization: Bearer <ACCESS_TOKEN>" \
  -H "Content-Type: application/json" \
  -H "User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Antigravity/1.18.3 Chrome/138.0.7204.235 Electron/37.3.1 Safari/537.36" \
  -H "X-Goog-Api-Client: google-cloud-sdk vscode_cloudshelleditor/0.1" \
  -H "Client-Metadata: {\"ideType\":\"ANTIGRAVITY\",\"platform\":\"WINDOWS\",\"pluginType\":\"GEMINI\"}" \
  -d '{
    "model": "claude-opus-4-6-thinking",
    "messages": [{"role": "user", "content": "Hello"}],
    "thinkingConfig": {"thinkingBudget": 16384}
  }'
```

### Token Refresh
```bash
curl -X POST https://oauth2.googleapis.com/token \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "client_id=<REDACTED>" \
  -d "client_secret=<REDACTED>" \
  -d "refresh_token=<REFRESH_TOKEN>" \
  -d "grant_type=refresh_token"
```
