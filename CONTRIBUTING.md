# Contributing to mcp-oxidized

Thank you for your interest in contributing to mcp-oxidized! This document provides guidelines and instructions for contributing.

## Development Setup

### Prerequisites

- **Rust 1.85+** (edition 2024)
- **cargo-tarpaulin** (Linux x86_64 only) for code coverage

```bash
# Install Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install development tools
rustup component add rustfmt clippy

# Install coverage tool (Linux x86_64 only)
cargo install cargo-tarpaulin
```

### Clone and Build

```bash
git clone https://github.com/fxthiry/mcp-oxidized.git
cd mcp-oxidized
cargo build
```

## Testing

The project uses a **three-tier testing strategy**:

| Tier | Type | Server | Runs in CI | Command |
|------|------|--------|------------|---------|
| 1 | Unit tests | None | Yes | `cargo test` |
| 2 | E2E tests | Mock (wiremock) | Yes | `cargo test` |
| 3 | Real API tests | Real Oxidized | No | `cargo test -- --ignored` |

### Running Tests

```bash
# Run all tests (unit + E2E with mock server)
# No external dependencies required!
cargo test

# Run real API tests (requires Oxidized server)
export OXIDIZED_URL="http://your-oxidized-server:8888"
export OXIDIZED_USER="admin"        # optional
export OXIDIZED_PASSWORD="secret"   # optional
cargo test -- --ignored

# Run a specific test
cargo test test_node_discovery

# Run tests with output
cargo test -- --nocapture
```

### Code Coverage

Coverage is measured using [cargo-tarpaulin](https://github.com/xd009642/tarpaulin).

> **Note**: tarpaulin only works on Linux x86_64 (uses ptrace).

```bash
# Generate coverage report
cargo tarpaulin

# Generate HTML report
cargo tarpaulin --out Html --output-dir coverage
open coverage/tarpaulin-report.html
```

**Coverage target:** >= 80% on core modules (`error.rs`, `oxidized.rs`, `resources.rs`, `tools/`).

## Code Style

### Formatting and Linting

```bash
# Check formatting
cargo fmt --check

# Apply formatting
cargo fmt

# Run linter (must pass with 0 warnings)
cargo clippy -- -D warnings
```

### Naming Conventions (RFC 430)

| Item | Convention | Example |
|------|------------|---------|
| Types, Traits, Enums | `UpperCamelCase` | `OxidizedClient`, `Actionable` |
| Functions, Methods, Variables | `snake_case` | `get_node_config`, `is_transient` |
| Constants | `SCREAMING_SNAKE_CASE` | `DEFAULT_TTL_SECS`, `MAX_RETRIES` |
| Modules | `snake_case` | `oxidized`, `tools`, `error` |

### Error Messages

All errors must implement the `Actionable` trait with structured format:

```rust
pub const ERROR_PREFIX: &str = "[Error]";
pub const CONTEXT_PREFIX: &str = "[Context]";
pub const SUGGESTIONS_PREFIX: &str = "[Suggestions]";
pub const NEXT_STEP_PREFIX: &str = "[Next Step]";
```

## Adding New Features

### Adding a New Tool

1. Create a new file in `src/tools/`:

```rust
// src/tools/my_new_tool.rs
use rmcp::model::Tool;

pub struct MyNewTool;

impl MyNewTool {
    pub fn definition() -> Tool {
        Tool::new("my_new_tool", "Description...")
            .with_string_param("param1", "Parameter description", true)
    }

    #[instrument(skip(backend), fields(request_id))]
    pub async fn execute(
        backend: &impl OxidizedBackend,
        params: &Value,
    ) -> Result<ToolResult, OxidizedError> {
        // Implementation
    }
}
```

2. Register in `src/tools/mod.rs`:

```rust
pub mod my_new_tool;
pub use my_new_tool::MyNewTool;
```

3. Add to `ServerHandler::call_tool()` in `src/main.rs`:

```rust
"my_new_tool" => MyNewTool::execute(&self.backend, &params).await,
```

4. Write tests in `tests/e2e_tests.rs` (mock) and `tests/integration_real_api.rs` (real).

### Adding a New Resource

1. Add handler in `src/resources.rs`:

```rust
pub async fn handle_my_resource(
    backend: &impl OxidizedBackend,
    params: &ResourceParams,
) -> Result<String, OxidizedError> {
    // Implementation
}
```

2. Register in `ServerHandler::list_resources()` and `read_resource()` in `src/main.rs`.

3. Write tests following the same pattern.

## Pull Request Process

### Before Submitting

1. **All tests pass**: `cargo test`
2. **No clippy warnings**: `cargo clippy -- -D warnings`
3. **Formatted code**: `cargo fmt --check`
4. **Coverage maintained**: Run `cargo tarpaulin` (Linux only)

### PR Guidelines

1. **One feature per PR** - Keep PRs focused
2. **Descriptive title** - Use conventional commit format: `feat(tools): add diff_configs`
3. **Update documentation** if adding new features
4. **Add tests** for new functionality

### Conventional Commits

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(scope): add new feature
fix(scope): fix bug
docs: update documentation
test: add tests
refactor: code refactoring
chore: maintenance tasks
```

### Review Process

1. CI must pass (tests, fmt, clippy)
2. At least one maintainer approval required
3. Squash merge preferred for clean history

## Architecture Overview

**Key architectural decisions:**

- **Trait-based backend**: `OxidizedBackend` trait allows testing with mock implementations
- **Actionable errors**: All errors implement `Actionable` for LLM-friendly messages
- **Cache invalidation**: Write operations (tools) invalidate related cache entries
- **Three-tier testing**: Unit + E2E mock + Real API integration

## Platform Notes

### cargo-tarpaulin

tarpaulin uses `ptrace` and only works on **Linux x86_64**. Coverage CI job runs on Ubuntu.

### Cross-compilation

Binaries are built for 5 platforms via cargo-dist:
- Linux x86_64, aarch64
- macOS x86_64, aarch64
- Windows x86_64

## Getting Help

- Open an [issue](https://github.com/fxthiry/mcp-oxidized/issues) for bugs or feature requests
- See [docs/troubleshooting.md](docs/troubleshooting.md) for common issues

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.
