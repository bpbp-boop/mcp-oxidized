# Tools Reference

This document describes all MCP tools provided by mcp-oxidized.

## Overview

| Tool | Description |
|------|-------------|
| [`fetch_node_config`](#fetch_node_config) | Trigger immediate backup of a node |
| [`prioritize_node`](#prioritize_node) | Move node to front of backup queue |
| [`reload_sources`](#reload_sources) | Reload Oxidized source inventory |
| [`diff_configs`](#diff_configs) | Compare two configuration versions |
| [`search_configs`](#search_configs) | Search patterns across configurations |

---

## fetch_node_config

Triggers an immediate backup of a node's configuration. This is useful when you need fresh data after making changes to a device.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `node` | string | Yes | The node name to backup |

### Example

**Request:**
```json
{
  "name": "fetch_node_config",
  "arguments": {
    "node": "router-core-01"
  }
}
```

**Response:**
```json
{
  "success": true,
  "message": "Backup triggered for node 'router-core-01'",
  "node": "router-core-01"
}
```

### Error Cases

- **NodeNotFound**: If the node doesn't exist, suggestions for similar nodes are provided
- **ApiUnreachable**: If Oxidized server is not accessible

### Cache Behavior

This operation invalidates the cache for the specified node. Subsequent resource reads will fetch fresh data.

---

## prioritize_node

Moves a node to the front of the Oxidized backup queue. The node will be processed before other pending nodes.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `node` | string | Yes | The node name to prioritize |

### Example

**Request:**
```json
{
  "name": "prioritize_node",
  "arguments": {
    "node": "switch-access-02"
  }
}
```

**Response:**
```json
{
  "success": true,
  "message": "Node 'switch-access-02' moved to front of backup queue",
  "node": "switch-access-02"
}
```

### Cache Behavior

This operation invalidates the cache for the specified node.

---

## reload_sources

Reloads the Oxidized source inventory. New devices added to your inventory will become available immediately after this operation.

### Parameters

None.

### Example

**Request:**
```json
{
  "name": "reload_sources",
  "arguments": {}
}
```

**Response:**
```json
{
  "success": true,
  "message": "Oxidized source inventory reloaded"
}
```

### Cache Behavior

This operation invalidates the **entire** node cache. All subsequent resource reads will fetch fresh data.

---

## diff_configs

Compares two configuration versions of a node using the Myers/LCS diff algorithm (same as git). Returns a structured diff with additions, deletions, and context.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `node` | string | Yes | The node name |
| `version1` | string | Yes | First version OID (older) |
| `version2` | string | Yes | Second version OID (newer) |

### Example

**Request:**
```json
{
  "name": "diff_configs",
  "arguments": {
    "node": "router-core-01",
    "version1": "abc123def456",
    "version2": "789ghi012jkl"
  }
}
```

**Response:**
```json
{
  "node": "router-core-01",
  "version1": "abc123def456",
  "version2": "789ghi012jkl",
  "summary": {
    "lines_added": 5,
    "lines_deleted": 2,
    "modification_blocks": 3
  },
  "unified_diff": "@@ -10,5 +10,8 @@\n context line\n-removed line\n+added line\n context line",
  "additions": [
    "ip route 10.0.0.0/8 192.168.1.1",
    "snmp-server community newcommunity RO"
  ],
  "deletions": [
    "ip route 172.16.0.0/12 192.168.1.1"
  ]
}
```

### Getting Version OIDs

Use the `oxidized://node/{name}/versions` resource to get available version OIDs:

```
Read resource: oxidized://node/router-core-01/versions
```

### LLM-Friendly Output

The response includes both structured data and a unified diff format that LLMs can easily interpret.

---

## search_configs

Searches for regex patterns across all (or specified) device configurations. Useful for security audits, compliance checks, and finding configuration patterns.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `pattern` | string | Yes | - | Regex pattern to search for |
| `nodes` | array | No | all | List of node names to search |
| `case_sensitive` | boolean | No | true | Case-sensitive search |
| `limit` | integer | No | 100 | Max matches to return (1-1000) |

### Examples

**Search all devices for SNMP community:**
```json
{
  "name": "search_configs",
  "arguments": {
    "pattern": "snmp-server community",
    "case_sensitive": false
  }
}
```

**Search specific devices for IP addresses:**
```json
{
  "name": "search_configs",
  "arguments": {
    "pattern": "ip address 10\\.0\\.",
    "nodes": ["router-core-01", "router-core-02"]
  }
}
```

**Response:**
```json
{
  "pattern": "snmp-server community",
  "nodes_searched": 15,
  "nodes_matched": 8,
  "total_matches": 24,
  "shown_matches": 24,
  "matches": [
    {
      "node": "router-core-01",
      "matches": [
        {
          "line_number": 42,
          "content": "snmp-server community public RO",
          "context_before": ["!", "interface GigabitEthernet0/0"],
          "context_after": ["snmp-server community private RW"]
        }
      ]
    }
  ]
}
```

### Pattern Syntax

Uses Rust regex syntax. Common patterns:

| Pattern | Matches |
|---------|---------|
| `snmp-server` | Literal text |
| `10\.0\.0\.\d+` | IP addresses in 10.0.0.x range |
| `(?i)password` | Case-insensitive "password" |
| `interface (Gi\|Te)` | Gigabit or TenGig interfaces |

### Performance

- Uses Oxidized's `conf_search` API for server-side pre-filtering when available
- Parallel configuration fetching with rate limiting
- Stop searching when limit is reached

### Security Considerations

- Pattern complexity is limited to prevent ReDoS attacks
- Large result sets are truncated at the specified limit
