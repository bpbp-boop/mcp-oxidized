# Troubleshooting Guide

This document covers common errors and their solutions.

## Understanding Error Messages

mcp-oxidized uses a structured error format optimized for AI assistants:

```
[Error] Brief description of what went wrong.
[Context] Additional information about the operation.
[Suggestions] Possible fixes or alternatives.
[Next Step] Specific action to take.
```

---

## Common Errors

### NodeNotFound

**Error Message:**
```
[Error] Node 'router-core-1' not found.
[Context] Search performed in Oxidized inventory.
[Suggestions] Similar nodes: router-core-01, router-core-02, router-core-03.
[Next Step] Use 'oxidized://nodes' to list all available nodes.
```

**Causes:**
- Typo in node name
- Node was removed from Oxidized inventory
- Node name is case-sensitive

**Solutions:**
1. Check the exact node name using `oxidized://nodes`
2. Use the suggested similar nodes
3. If the node was just added, run `reload_sources` tool first

---

### ApiUnreachable

**Error Message:**
```
[Error] Cannot connect to Oxidized API.
[Context] Attempted to reach http://oxidized.example.com:8888.
[Suggestions] Check network connectivity, verify OXIDIZED_URL is correct.
[Next Step] Verify Oxidized server is running with: curl http://oxidized.example.com:8888/nodes.json
```

**Causes:**
- Oxidized server is not running
- Wrong URL configured
- Network/firewall blocking connection
- oxidized-web gem not installed

**Solutions:**
1. Verify Oxidized is running: `systemctl status oxidized`
2. Check oxidized-web is installed: `gem list oxidized-web`
3. Test connectivity: `curl http://your-server:8888/nodes.json`
4. Check firewall rules for port 8888 (or your configured port)

---

### AuthFailed

**Error Message:**
```
[Error] Authentication failed.
[Context] HTTP 401 Unauthorized from Oxidized API.
[Suggestions] Verify OXIDIZED_USER and OXIDIZED_PASSWORD are correct.
[Next Step] Check credentials and try again.
```

**Causes:**
- Wrong username or password
- Credentials not configured
- Oxidized authentication method changed

**Solutions:**
1. Verify credentials work with curl:
   ```bash
   curl -u admin:password http://oxidized.example.com:8888/nodes.json
   ```
2. Check environment variables are set correctly
3. If using `OXIDIZED_PASSWORD_FILE`, verify the file exists and is readable

---

### InvalidRegex

**Error Message:**
```
[Error] Invalid regex pattern: unclosed group.
[Context] Pattern 'snmp-server (community' failed to compile.
[Suggestions] Check regex syntax, escape special characters.
[Next Step] Fix the pattern and try again.
```

**Causes:**
- Unbalanced parentheses, brackets, or braces
- Unescaped special characters
- Invalid regex syntax

**Solutions:**
1. Escape special characters with backslash: `\.` `\(` `\[`
2. Use online regex tester to validate pattern
3. Common fixes:
   - `10.0.0.1` → `10\.0\.0\.1`
   - `interface(Gi)` → `interface\(Gi\)` or `interface (Gi)`

---

### VersionNotFound

**Error Message:**
```
[Error] Version 'abc123' not found for node 'router-core-01'.
[Context] Available versions: def456, ghi789, jkl012.
[Suggestions] Use a valid OID from the versions list.
[Next Step] Read 'oxidized://node/router-core-01/versions' to see available versions.
```

**Causes:**
- Invalid or truncated OID
- Old version was garbage collected

**Solutions:**
1. Get valid OIDs from `oxidized://node/{name}/versions`
2. Use the full OID string, not truncated

---

### ConfigTooLarge

**Error Message:**
```
[Error] Configuration too large for full retrieval.
[Context] Config size: 2.5MB, ~85000 tokens (exceeds 30000 token limit).
[Suggestions] Use truncate=100 to get first 100 lines, or summary=true for overview.
[Next Step] Read 'oxidized://node/router-core-01/config?summary=true'.
```

**Causes:**
- Device has very large configuration (firewalls, large routers)
- Configuration exceeds LLM context window

**Solutions:**
1. Use `truncate` parameter: `oxidized://node/{name}/config?truncate=200`
2. Use `summary` parameter: `oxidized://node/{name}/config?summary=true`
3. Search for specific patterns using `search_configs` tool

---

### RateLimited

**Error Message:**
```
[Error] Rate limited by Oxidized API.
[Context] Too many requests in short period.
[Suggestions] Wait a moment before retrying.
[Next Step] Retry after a few seconds.
```

**Causes:**
- Too many concurrent requests
- Oxidized server under heavy load

**Solutions:**
1. Wait a few seconds and retry
2. Reduce concurrent operations
3. mcp-oxidized has built-in retry with exponential backoff

---

## Enabling Debug Logs

For detailed troubleshooting, enable debug logging:

```bash
# Set log level before starting
export RUST_LOG=mcp_oxidized=debug

# Or for maximum verbosity
export RUST_LOG=mcp_oxidized=trace
```

**Log output location:** stderr (stdout is reserved for MCP JSON-RPC)

### What debug logs show:
- HTTP request/response details
- Cache hits/misses
- Retry attempts
- Parameter parsing

---

## Testing Connectivity

### Quick connectivity test

```bash
# Test basic API access
curl http://your-oxidized-server:8888/nodes.json

# Test with authentication
curl -u admin:password http://your-oxidized-server:8888/nodes.json

# Test specific node
curl http://your-oxidized-server:8888/node/show/router-core-01.json
```

### Running integration tests

```bash
# Set environment
export OXIDIZED_URL="http://your-oxidized-server:8888"
export OXIDIZED_USER="admin"      # if needed
export OXIDIZED_PASSWORD="pass"   # if needed

# Run real API tests
cargo test -- --ignored
```

---

## Known Issues

### Oxidized API Quirks

mcp-oxidized handles these oxidized-web quirks automatically:

| Issue | Workaround |
|-------|------------|
| `/stats` returns 404 | Stats computed from `/nodes.json` |
| NodeNotFound returns HTTP 500 | Parsed from Ruby error message |
| `/node/show` missing fields | Fallback to nested data |

### Platform-Specific

| Platform | Issue | Notes |
|----------|-------|-------|
| Linux x86_64 | cargo-tarpaulin coverage | Only platform that supports tarpaulin |
| Windows | Path separators | Use forward slashes in config paths |

---

## Getting Help

If you can't resolve an issue:

1. Check [existing issues](https://github.com/fxthiry/mcp-oxidized/issues)
2. Open a new issue with:
   - mcp-oxidized version
   - Oxidized/oxidized-web versions
   - Full error message
   - Debug logs (`RUST_LOG=debug`)
   - Steps to reproduce
