# mcp-oxidized

MCP server for Oxidized network device configuration backup system.

## Development

### Running Tests

```bash
# Run all unit tests
cargo test

# Run integration tests (requires real Oxidized server)
export OXIDIZED_URL="http://your-oxidized-server:8888"
export OXIDIZED_USER="admin"      # optional
export OXIDIZED_PASSWORD="secret"  # optional
cargo test -- --ignored
```

### Code Quality

```bash
cargo fmt --check  # Check formatting
cargo clippy       # Run linter
cargo build        # Build project
```
