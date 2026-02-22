# Contributing to Hypr-Claw

Thank you for your interest in contributing to Hypr-Claw!

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/yourusername/hypr-claw.git`
3. Create a branch: `git checkout -b feature/your-feature`
4. Make your changes
5. Run tests: `cargo test --workspace`
6. Submit a pull request

## Development Setup

### Prerequisites

- Rust 1.75+ (2021 edition)
- Linux (Ubuntu/Arch recommended)
- Git

### Build

```bash
cargo build
```

### Test

```bash
cargo test --workspace
```

### Code Quality

```bash
# Format code
cargo fmt

# Run clippy
cargo clippy --all-targets -- -D warnings
```

## Contribution Guidelines

### Code Standards

1. **All tests must pass**
   ```bash
   cargo test --workspace
   ```

2. **No clippy warnings**
   ```bash
   cargo clippy --all-targets -- -D warnings
   ```

3. **Code must be formatted**
   ```bash
   cargo fmt
   ```

4. **New features require tests**
   - Unit tests for individual functions
   - Integration tests for cross-crate functionality

5. **Documentation must be updated**
   - Update README.md if user-facing changes
   - Update docs/ if architecture changes
   - Add inline documentation for public APIs

### Commit Messages

Use clear, descriptive commit messages:

```
Add rate limiting to tool execution

- Implement RateLimiter with time-window based limits
- Add per-tool and per-session rate limiting
- Include tests for rate limiter functionality
```

### Pull Request Process

1. **Update documentation** - Ensure README and docs/ are current
2. **Add tests** - All new code must have tests
3. **Pass CI checks** - All tests and lints must pass
4. **Describe changes** - Clearly explain what and why
5. **Link issues** - Reference related issues if applicable

## Adding New Features

### Adding a Tool

1. Create tool in `crates/tools/src/`:

```rust
use async_trait::async_trait;
use crate::traits::{Tool, ToolResult, ToolError};

pub struct MyTool;

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str { "my_tool" }
    fn description(&self) -> &str { "Tool description" }
    fn schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "param": {"type": "string"}
            },
            "required": ["param"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolResult, ToolError> {
        // Implementation
        Ok(ToolResult {
            success: true,
            output: json!({"result": "value"}),
            error: None,
        })
    }
}
```

2. Add tests in `crates/tools/tests/`
3. Register in tool registry
4. Update documentation

### Adding a Soul Profile

1. Create YAML file in `souls/`:

```yaml
id: my_soul
system_prompt: |
  You are a specialized assistant.
  
config:
  allowed_tools:
    - echo
    - file_read
  autonomy_mode: confirm
  max_iterations: 10
  risk_tolerance: low
  verbosity: normal
```

2. Test the soul profile
3. Document in README.md

### Adding Documentation

1. Technical docs go in `docs/`
2. User-facing docs go in README.md
3. Use clear, concise language
4. Include code examples
5. Add diagrams where helpful

## Testing

### Unit Tests

Test individual functions and modules:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature() {
        // Test implementation
        assert_eq!(result, expected);
    }
}
```

### Integration Tests

Test cross-crate functionality in `crates/*/tests/`:

```rust
#[tokio::test]
async fn test_integration() {
    // Test implementation
}
```

### Running Tests

```bash
# All tests
cargo test --workspace

# Specific crate
cargo test --package hypr-claw-core

# Specific test
cargo test test_name
```

## Code Review

All contributions will be reviewed for:

1. **Correctness** - Does it work as intended?
2. **Tests** - Are there adequate tests?
3. **Documentation** - Is it documented?
4. **Code quality** - Is it clean and maintainable?
5. **Security** - Are there security implications?

## Security

If you discover a security vulnerability:

1. **Do not** open a public issue
2. Email security details to maintainers
3. Allow time for a fix before disclosure

## Questions?

- Open an issue for questions
- Check existing documentation in `docs/`
- Review closed issues for similar questions

## License

By contributing, you agree that your contributions will be licensed under the same license as the project.
