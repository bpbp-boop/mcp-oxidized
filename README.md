# mcp-oxidized

[![CI](https://github.com/fxthiry/mcp-oxidized/actions/workflows/ci.yml/badge.svg)](https://github.com/fxthiry/mcp-oxidized/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/mcp-oxidized.svg)](https://crates.io/crates/mcp-oxidized)
[![codecov](https://codecov.io/gh/fxthiry/mcp-oxidized/branch/main/graph/badge.svg)](https://codecov.io/gh/fxthiry/mcp-oxidized)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)

> MCP server exposing Oxidized network configuration backups to AI assistants with **actionable error messages**.

mcp-oxidized is a [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) server that connects AI assistants (Claude Desktop, Cursor, Zed, Windsurf) to your [Oxidized](https://github.com/ytti/oxidized) network device configuration backup system.

**Key Differentiator:** When something goes wrong, you get structured error messages that LLMs can understand and act upon - not cryptic stack traces.

## Quick Start

Get up and running in less than 5 minutes:

### 1. Prerequisites

- **Oxidized 0.35.0+** with **oxidized-web 0.18.0+** running and accessible
- An MCP-compatible client (Claude Desktop, Cursor, Zed, Windsurf)

> **Note**: The REST API is provided by [oxidized-web](https://github.com/ytti/oxidized-web), a separate Ruby gem from Oxidized itself.

### 2. Installation

**Option A: Download binary** (recommended)

Download the latest binary for your platform from [Releases](https://github.com/fxthiry/mcp-oxidized/releases).

**Option B: Build from source**

```bash
cargo install mcp-oxidized
# or
git clone https://github.com/fxthiry/mcp-oxidized.git
cd mcp-oxidized
cargo build --release
```

### 3. Configuration

Add to your MCP client config. Example for Claude Desktop (`~/.config/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "oxidized": {
      "command": "/path/to/mcp-oxidized",
      "env": {
        "OXIDIZED_URL": "https://your-oxidized-server:8888",
        "OXIDIZED_USER": "admin",
        "OXIDIZED_PASSWORD": "your-password"
      }
    }
  }
}
```

**Zero-config mode:** If no env vars are set, defaults to `http://localhost:8888` with no authentication.

📖 **[Full Configuration Guide](docs/configuration.md)** - All environment variables, MCP client configs (Cursor, Zed, Windsurf), SSL options, custom headers, and security best practices.

## Tools

| Tool | Parameters | Description |
|------|------------|-------------|
| `fetch_node_config` | `node` | Trigger immediate backup of a node's configuration |
| `prioritize_node` | `node` | Move a node to the front of the backup queue |
| `reload_sources` | _(none)_ | Reload Oxidized source inventory (new devices become available) |
| `diff_configs` | `node`, `version1`, `version2` | Compare two configuration versions using Myers/LCS algorithm |
| `search_configs` | `pattern`, `nodes?`, `case_sensitive?`, `limit?` | Search regex patterns across all device configurations |

## Resources

| Resource URI | Description |
|--------------|-------------|
| `oxidized://nodes` | List all nodes with pagination (`offset`, `limit`, `group`) |
| `oxidized://node/{name}` | Node details (model, status, last backup time) |
| `oxidized://node/{name}/config` | Current configuration (with `truncate`, `summary` options for large configs) |
| `oxidized://node/{name}/versions` | Configuration version history |
| `oxidized://node/{name}/versions/{oid}` | Specific historical version content |
| `oxidized://stats` | Global backup statistics |

## Actionable Errors

When something goes wrong, mcp-oxidized provides structured error messages optimized for AI assistants:

```
[Error] Node 'SW-Unknown' not found.
[Context] Search performed in Oxidized inventory.
[Suggestions] Similar nodes: SW-Core-01, SW-Access-02.
[Next Step] Use 'oxidized://nodes' to list all available nodes.
```

This format helps LLMs understand what went wrong and suggest corrections automatically.

## Example Usage

Ask your AI assistant:

- "List all network devices in Oxidized"
- "Show me the configuration of router-core-01"
- "Compare the last two versions of switch-access-02"
- "Find all devices with SNMP community 'public' configured"
- "Trigger a backup of firewall-edge-01 now"

---

## Security Considerations

> ⚠️ **Important:** Network device configurations often contain sensitive information (passwords, SNMP communities, VPN keys, ACLs, etc.). By using mcp-oxidized, you are giving your AI assistant access to this data.

**Recommendations:**

- **Review what you share** - Be mindful when asking your AI to analyze configurations; the content is sent to the LLM provider
- **Use with trusted LLM providers** - Ensure your organization's policies allow sending network configuration data to the AI service you're using
- **Consider data residency** - Some LLM providers may process data in different jurisdictions
- **Limit scope when possible** - Use node filtering in searches rather than querying all configurations

mcp-oxidized itself does not log or transmit configuration content beyond what the MCP protocol requires for your AI assistant to function.

---

## Contributing

Contributions are welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, testing, and PR guidelines.

## Documentation

- [Tools Reference](docs/tools.md) - Detailed tool documentation with examples
- [Resources Reference](docs/resources.md) - Resource URI patterns and response formats
- [Configuration Guide](docs/configuration.md) - All environment variables and options
- [Troubleshooting](docs/troubleshooting.md) - Common errors and solutions

## License

Apache License 2.0 - see [LICENSE](LICENSE) for details.
