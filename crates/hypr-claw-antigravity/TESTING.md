# Testing Antigravity & Gemini CLI

## Quick Start

### Interactive Chat (Recommended for Testing)

```bash
cargo run --example chat -p hypr-claw-antigravity
```

This will:
1. Guide you through OAuth authentication (if needed)
2. Let you select a model (Claude or Gemini)
3. Start an interactive chat session

**Available Models:**
- Claude Opus 4.6 Thinking (medium/high) - Antigravity quota
- Gemini 3 Flash (high) - Antigravity quota
- Gemini 3 Flash Preview (high) - Gemini CLI quota (separate)
- Gemini 3 Pro (low) - Antigravity quota

## Authentication Flow

### First Time Setup

1. Run: `cargo run --example chat -p hypr-claw-antigravity`
2. Open the OAuth URL in your browser
3. Authorize with Google account
4. Copy the redirect URL: `http://localhost:51121/oauth-callback?code=...&state=...`
5. Extract and paste `code` and `state` values
6. Done! Account saved to `./data/antigravity-accounts.json`

### Subsequent Uses

Just run the chat example - it will use your saved account.

## Model Selection Guide

**Antigravity Quota:**
- Claude Opus 4.6 Thinking (medium) - 16K thinking tokens
- Claude Opus 4.6 Thinking (high) - 32K thinking tokens
- Gemini 3 Flash (high) - Fast responses
- Gemini 3 Pro (low) - Balanced

**Gemini CLI Quota (Separate):**
- Gemini 3 Flash Preview (high) - 2x capacity!

**Tip:** System automatically rotates accounts on rate limits.
