# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0] - 2025-12-23

### Initial Release

mcp-oxidized is an MCP server that exposes Oxidized network configuration backup capabilities to AI assistants (Claude Desktop, Cursor, Zed, Windsurf).

**Key Features:**

- 5 MCP Tools - Trigger backups, compare configs, search patterns across your network
- 6 MCP Resources - Discover nodes, view configurations, browse version history
- Actionable Errors - LLM-optimized error messages with suggestions and next steps
- Smart Caching - moka-based cache with automatic invalidation on write operations
- Large Config Handling - Truncation, summary mode, and token estimation for oversized configs
- Resilient Operations - Retry logic with exponential backoff for transient failures

**Compatibility:**

- Oxidized 0.35.0+ / Oxidized-web 0.18.0+
- MCP Stdio transport (Claude Desktop, Cursor, Zed, Windsurf)

### Added

#### Tools

- `fetch_node_config` - Trigger immediate backup of a node's configuration
- `prioritize_node` - Move a node to the front of the backup queue
- `reload_sources` - Reload Oxidized source inventory (new devices available immediately)
- `diff_configs` - Compare two configuration versions using Myers/LCS algorithm
- `search_configs` - Regex search across configurations with server-side pre-filter

#### Resources

- `oxidized://nodes` - List all nodes with pagination and group filtering
- `oxidized://node/{name}` - Node details (model, status, last backup time)
- `oxidized://node/{name}/config` - Current configuration with truncate/summary options
- `oxidized://node/{name}/versions` - Configuration version history
- `oxidized://node/{name}/versions/{oid}` - Specific historical version content
- `oxidized://stats` - Global backup statistics

#### Infrastructure

- Actionable error framework with `[Error]`, `[Context]`, `[Suggestions]`, `[Next Step]` format
- Cache with TTL: nodes (5min), config (2min), stats (30s)
- E2E test suite with wiremock mock server (runs in CI without real Oxidized)
- Integration tests for real Oxidized validation (`cargo test -- --ignored`)
- Code coverage with cargo-tarpaulin in CI
- cargo-dist for multi-platform binary releases

### Documentation

- README with quick start guide (< 5 minutes)
- CONTRIBUTING.md with development setup and PR guidelines
- docs/tools.md - Complete tool reference with examples
- docs/resources.md - Resource URI patterns and response formats
- docs/configuration.md - All environment variables and MCP client configs
- docs/troubleshooting.md - Common errors and solutions

[Unreleased]: https://github.com/fxthiry/mcp-oxidized/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/fxthiry/mcp-oxidized/releases/tag/v1.0.0
