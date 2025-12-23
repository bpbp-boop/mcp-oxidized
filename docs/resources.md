# Resources Reference

This document describes all MCP resources provided by mcp-oxidized.

## Overview

| Resource URI | Description |
|--------------|-------------|
| [`oxidized://nodes`](#oxidizednodes) | List all nodes with pagination |
| [`oxidized://node/{name}`](#oxidizednodename) | Node details |
| [`oxidized://node/{name}/config`](#oxidizednodename-config) | Current configuration |
| [`oxidized://node/{name}/versions`](#oxidizednodename-versions) | Version history |
| [`oxidized://node/{name}/versions/{oid}`](#oxidizednodenameversionoid) | Specific version |
| [`oxidized://stats`](#oxidizedstats) | Global statistics |

---

## oxidized://nodes

Lists all network devices in the Oxidized inventory with pagination support.

### URI Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `offset` | integer | 0 | Number of nodes to skip |
| `limit` | integer | 100 | Max nodes to return (max: 500) |
| `group` | string | - | Filter by device group |

### Examples

**List first 100 nodes:**
```
oxidized://nodes
```

**Paginate through nodes:**
```
oxidized://nodes?offset=0&limit=50
oxidized://nodes?offset=50&limit=50
```

**Filter by group:**
```
oxidized://nodes?group=datacenter-west
```

### Response

```json
{
  "total": 150,
  "offset": 0,
  "limit": 100,
  "count": 100,
  "nodes": [
    {
      "name": "router-core-01",
      "group": "datacenter-west",
      "model": "ios",
      "status": "success",
      "last_backup": "2025-12-23T10:30:00Z"
    }
  ],
  "cache": {
    "hit": true,
    "age_seconds": 45
  }
}
```

---

## oxidized://node/{name}

Gets detailed information about a specific node.

### URI Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | Yes | Node name (URL-encoded if contains special characters) |

### Example

```
oxidized://node/router-core-01
```

### Response

```json
{
  "name": "router-core-01",
  "full": "datacenter-west/router-core-01",
  "group": "datacenter-west",
  "model": "ios",
  "ip": "192.168.1.1",
  "status": "success",
  "last_backup": "2025-12-23T10:30:00Z",
  "vars": {
    "location": "Building A, Rack 12"
  },
  "cache": {
    "hit": false
  }
}
```

### Error Cases

If the node is not found, the error includes suggestions for similar nodes:

```
[Error] Node 'router-core-1' not found.
[Context] Search performed in Oxidized inventory.
[Suggestions] Similar nodes: router-core-01, router-core-02, router-core-03.
[Next Step] Use 'oxidized://nodes' to list all available nodes.
```

---

## oxidized://node/{name}/config

Gets the current configuration of a node. Includes handling for large configurations.

### URI Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `name` | string | Yes | Node name |
| `truncate` | integer | - | Truncate to N lines (useful for large configs) |
| `summary` | boolean | false | Return summary instead of full config |

### Examples

**Full configuration:**
```
oxidized://node/router-core-01/config
```

**Truncated (first 100 lines):**
```
oxidized://node/router-core-01/config?truncate=100
```

**Summary only:**
```
oxidized://node/router-core-01/config?summary=true
```

### Response (Full)

```json
{
  "node": "router-core-01",
  "config": "! Configuration for router-core-01\n...",
  "size": {
    "bytes": 45678,
    "lines": 1234,
    "estimated_tokens": 15000
  },
  "cache": {
    "hit": true,
    "age_seconds": 120
  }
}
```

### Response (Summary)

When `summary=true` or config exceeds token threshold:

```json
{
  "node": "router-core-01",
  "summary": {
    "sections": ["interfaces", "routing", "acl", "snmp"],
    "interfaces_count": 48,
    "acl_count": 15,
    "routing_protocols": ["ospf", "bgp"]
  },
  "size": {
    "bytes": 245678,
    "lines": 5678,
    "estimated_tokens": 85000
  },
  "truncated": true,
  "message": "Configuration too large. Use truncate parameter or summary=true."
}
```

### Large Configuration Handling

Configurations are automatically assessed for size. If a config exceeds ~100KB or ~30,000 tokens:
- Metadata is returned with size information
- Suggestions to use `truncate` or `summary` parameters
- Estimated token count helps LLMs understand context limits

---

## oxidized://node/{name}/versions

Gets the version history (commits) for a node's configuration.

### URI Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | Yes | Node name |

### Example

```
oxidized://node/router-core-01/versions
```

### Response

```json
{
  "node": "router-core-01",
  "versions": [
    {
      "oid": "abc123def456789",
      "date": "2025-12-23T10:30:00Z",
      "author": {
        "name": "oxidized",
        "email": "oxidized@localhost"
      },
      "message": "update router-core-01"
    },
    {
      "oid": "789ghi012jkl345",
      "date": "2025-12-22T14:15:00Z",
      "author": {
        "name": "oxidized",
        "email": "oxidized@localhost"
      },
      "message": "update router-core-01"
    }
  ],
  "count": 42,
  "cache": {
    "hit": false
  }
}
```

### Using Versions

The `oid` values can be used with:
- `oxidized://node/{name}/versions/{oid}` - to view specific version
- `diff_configs` tool - to compare two versions

---

## oxidized://node/{name}/versions/{oid}

Gets a specific historical version of a node's configuration.

### URI Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `name` | string | Yes | Node name |
| `oid` | string | Yes | Version OID from version history |

### Example

```
oxidized://node/router-core-01/versions/abc123def456789
```

### Response

```json
{
  "node": "router-core-01",
  "oid": "abc123def456789",
  "date": "2025-12-23T10:30:00Z",
  "config": "! Configuration for router-core-01 (version abc123def456789)\n...",
  "size": {
    "bytes": 45678,
    "lines": 1234
  }
}
```

---

## oxidized://stats

Gets global backup statistics for the Oxidized instance.

### Example

```
oxidized://stats
```

### Response

```json
{
  "total_nodes": 150,
  "status_counts": {
    "success": 142,
    "failure": 5,
    "never": 3
  },
  "success_rate": 94.67,
  "last_updated": "2025-12-23T10:35:00Z",
  "groups": {
    "datacenter-west": 75,
    "datacenter-east": 50,
    "branch-offices": 25
  },
  "cache": {
    "hit": true,
    "age_seconds": 15
  }
}
```

### Notes

- Statistics are computed from the nodes list (Oxidized's native stats endpoint has issues)
- Success rate is calculated as: `success_count / total_nodes * 100`
- Cache TTL for stats is 30 seconds (shorter than nodes cache)

---

## Cache Metadata

All resource responses include cache information:

```json
{
  "cache": {
    "hit": true,      // true if served from cache
    "age_seconds": 45 // seconds since cache entry was created
  }
}
```

### Cache TTLs

| Resource | TTL |
|----------|-----|
| Nodes list | 5 minutes |
| Node details | 5 minutes |
| Configuration | 2 minutes |
| Stats | 30 seconds |

### Cache Invalidation

Write operations (tools) automatically invalidate related cache entries:
- `fetch_node_config(node)` invalidates that node's cache
- `prioritize_node(node)` invalidates that node's cache
- `reload_sources()` invalidates **all** node caches
