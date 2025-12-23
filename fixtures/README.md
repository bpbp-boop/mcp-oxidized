# Test Fixtures

This directory contains sample JSON data for unit testing the Oxidized client implementation.

## Files

| File | Description | Used In |
|------|-------------|---------|
| `nodes.json` | Sample list of 5 network nodes | Unit tests for `get_nodes()` deserialization |
| `node.json` | Single node details | Unit tests for `get_node()` deserialization |
| `stats.json` | Global server statistics | Unit tests for `get_stats()` deserialization |
| `versions.json` | Configuration version history | Unit tests for `get_node_versions()` deserialization |

## Data Format

All fixtures use the same JSON format as the Oxidized REST API (v0.28+).
Field names use `snake_case` matching both Rust structs and API responses.

### Node Fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Short device name |
| `full_name` | string | Fully qualified name |
| `ip` | string | Device IP address |
| `group` | string | Device category |
| `model` | string | Device platform |
| `status` | string | Current backup status |
| `last_status` | string | Previous backup status |
| `time` | string? | Last backup timestamp |
| `mtime` | string? | Last config modification |

### Version Fields

| Field | Type | Description |
|-------|------|-------------|
| `oid` | string | Git commit hash |
| `date` | string | Commit timestamp |
| `author` | string | Commit author |
| `message` | string | Commit message |

### Stats Fields

| Field | Type | Description |
|-------|------|-------------|
| `total_nodes` | number? | Total managed nodes |
| `success_count` | number? | Successful backups |
| `failure_count` | number? | Failed backups |
| `last_run` | string? | Last backup run time |

## Usage in Tests

```rust
use std::fs;

#[test]
fn test_node_deserialization() {
    let json = fs::read_to_string("fixtures/node.json").unwrap();
    let node: Node = serde_json::from_str(&json).unwrap();
    assert_eq!(node.name, "SW-Core-01");
}
```

## Contributing

When updating fixtures:

1. Ensure JSON is valid (`jq . fixture.json`)
2. Use realistic but fake data
3. Include edge cases (optional fields missing)
4. Update this README if adding new files
