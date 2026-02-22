# Quick Start Guide - LLM Provider Configuration

## First Time Setup

### Option 1: NVIDIA Kimi (Cloud)

1. Run the application:
   ```bash
   ./target/release/hypr-claw
   ```

2. When prompted, select provider:
   ```
   Select provider:
   1. NVIDIA Kimi (cloud)
   2. Local model
   
   Choice [1-2]: 1
   ```

3. Enter your NVIDIA API key (input is hidden):
   ```
   Enter NVIDIA API key:
   [type your key - not visible]
   ```

4. Configuration saved! Continue with your task.

### Option 2: Local Model

1. Run the application:
   ```bash
   ./target/release/hypr-claw
   ```

2. When prompted, select provider:
   ```
   Select provider:
   1. NVIDIA Kimi (cloud)
   2. Local model
   
   Choice [1-2]: 2
   ```

3. Enter your local LLM base URL:
   ```
   Enter local LLM base URL: http://localhost:8080
   ```

4. Configuration saved! Continue with your task.

## Subsequent Runs

After initial setup, simply run:
```bash
./target/release/hypr-claw
```

No provider configuration needed - it's remembered!

## Reconfiguration

To change providers or update API key:
```bash
./target/release/hypr-claw config reset
```

Then run normally to go through setup again.

## Troubleshooting

### "Invalid API key (401 Unauthorized)"
Your NVIDIA API key is incorrect or expired.
```bash
./target/release/hypr-claw config reset
```

### "Rate limit exceeded (429 Too Many Requests)"
Wait a moment and try again. NVIDIA has rate limits.

### "Failed to retrieve NVIDIA API key"
Your credential store may be corrupted.
```bash
./target/release/hypr-claw config reset
```

### "Failed to load config.yaml"
Your config file may be corrupted.
```bash
./target/release/hypr-claw config reset
```

## Configuration Files

### Location
- Config: `./data/config.yaml`
- Master Key: `./data/.master_key`
- Encrypted Credentials: `./data/credentials/`

### Manual Inspection (NVIDIA)
```bash
cat ./data/config.yaml
```
Output:
```yaml
provider: nvidia
model: moonshotai/kimi-k2.5
```

### Manual Inspection (Local)
```bash
cat ./data/config.yaml
```
Output:
```yaml
provider: !local
  base_url: http://localhost:8080
model: default
```

## Security Notes

- API keys are encrypted at rest using AES-256-GCM
- Master key is automatically generated on first run
- Keys are never logged or printed
- Password input is hidden (no echo)
- Credentials stored in `./data/credentials/` (encrypted)

## Development

### Build
```bash
cargo build --release
```

### Test
```bash
cargo test
```

### Lint
```bash
cargo clippy --all-targets -- -D warnings
```

### Run Tests for Bootstrap
```bash
cargo test --test test_bootstrap --test test_config
```
