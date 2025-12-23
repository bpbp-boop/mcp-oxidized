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

### Tools (3)

| Tool | Description |
|------|-------------|
| `fetch_node_config` | Trigger immediate backup of a node's configuration |
| `prioritize_node` | Move a node to the front of the backup queue |
| `reload_sources` | Reload Oxidized source inventory |

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

## License

MIT
