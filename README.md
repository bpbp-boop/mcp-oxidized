# mcp-oxidized

MCP server for Oxidized network device configuration backup system.

**Key Differentiator:** Actionable error messages optimized for LLM consumption.

## Compatibility

| Component | Version |
|-----------|---------|
| Oxidized (backend) | 0.35.0+ |
| Oxidized-web (REST API) | 0.18.0+ |

> **Note**: The REST API is provided by [oxidized-web](https://github.com/ytti/oxidized-web), a separate Ruby gem from Oxidized itself.

## Quick Start

### Installation

Download the latest binary from [Releases](https://github.com/fxthiry/mcp-oxidized/releases) or build from source:

```bash
cargo build --release
```

### Configuration (Claude Desktop)

Add to your Claude Desktop config (`~/.config/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "oxidized": {
      "command": "/path/to/mcp-oxidized",
      "env": {
        "OXIDIZED_URL": "http://your-oxidized-server:8888",
        "OXIDIZED_USER": "admin",
        "OXIDIZED_PASSWORD": "secret"
      }
    }
  }
}
```

**Zero-config mode:** If no env vars are set, defaults to `http://localhost:8888` with no authentication.

## Features

### Resources (6)

| Resource URI | Description |
|--------------|-------------|
| `oxidized://nodes` | List all network devices (paginated) |
| `oxidized://node/{name}` | Get specific node details |
| `oxidized://node/{name}/config` | Get current configuration with size metadata |
| `oxidized://node/{name}/versions` | Get configuration version history |
| `oxidized://node/{name}/versions/{oid}` | Get specific historical version |
| `oxidized://stats` | Global backup statistics |

### Tools (5)

| Tool | Description |
|------|-------------|
| `fetch_node_config` | Trigger immediate backup of a node's configuration |
| `prioritize_node` | Move a node to the front of the backup queue |
| `reload_sources` | Reload Oxidized source inventory |
| `diff_configs` | Compare two configuration versions (Myers/LCS algorithm) |
| `search_configs` | Search regex patterns across all device configurations |

## Actionable Errors

When something goes wrong, mcp-oxidized provides structured error messages optimized for AI assistants:

```
[Error] Node 'SW-Unknown' not found.
[Context] Search performed in Oxidized inventory.
[Suggestions] Similar nodes: SW-Core-01, SW-Access-02.
[Next Step] Use 'oxidized://nodes' to list all available nodes.
```

## Development

### Running Tests

The project uses a two-tier testing strategy:

| Test Type | Server | Runs in CI | Command |
|-----------|--------|------------|---------|
| Unit + E2E | Mock (wiremock) | ✅ | `cargo test` |
| Real API | Real Oxidized | ❌ | `cargo test -- --ignored` |

```bash
# Run all tests (unit + E2E with mock server) - No external dependencies!
cargo test

# Run real API tests (requires real Oxidized server)
export OXIDIZED_URL="http://your-oxidized-server:8888"
export OXIDIZED_USER="admin"      # optional
export OXIDIZED_PASSWORD="secret"  # optional
cargo test -- --ignored
```

> **Note for contributors:** You can run `cargo test` without any Oxidized server - the E2E tests use a mock server (wiremock).

### Code Quality

```bash
cargo fmt --check  # Check formatting
cargo clippy       # Run linter
cargo build        # Build project
```

## License

MIT
