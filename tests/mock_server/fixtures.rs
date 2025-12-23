//! Mock fixtures for E2E testing.
//!
//! This module provides test data structures that **exactly match** Oxidized-web 0.18.0+ API responses.
//! Validated against real API: https://oxidized.te-mgmt.io on 2025-12-23.
//!
//! # Data Structures
//!
//! - [`MockNode`] - Node with all real API fields including `last`, `vars`, `mtime`, top-level `status`/`time`
//! - [`MockNodeLast`] - Nested status object `{status, start, end, time}`
//! - [`MockNodeVars`] - Variables object (usually `{enable: null}`)
//! - [`MockVersion`] - Version with nested `author` object (API quirk)
//! - [`MockVersionAuthor`] - Nested author `{name, email, time}`
//!
//! # Default Fixtures
//!
//! - [`default_nodes()`] - 3 nodes with varied statuses (success, failure, never)
//! - [`default_versions()`] - 3 versions for testing diff and history
//! - [`default_configs()`] - Realistic configs keyed by node name

use serde::Serialize;
use std::collections::HashMap;

/// Mock node matching Oxidized-web API structure **exactly**.
///
/// Validated against real API response from /nodes.json:
/// ```json
/// {
///   "name": "Parc-Core",
///   "full_name": "mikrotik/Parc-Core",
///   "ip": "10.255.42.21",
///   "group": "mikrotik",
///   "model": "RouterOS",
///   "last": {"start": "...", "end": "...", "status": "success", "time": 0.724985114},
///   "vars": {"enable": null},
///   "mtime": "unknown",
///   "status": "success",
///   "time": "2025-12-23 14:05:51 UTC"
/// }
/// ```
#[derive(Clone, Debug, Serialize)]
pub struct MockNode {
    pub name: String,
    pub full_name: String,
    pub ip: String,
    pub group: String,
    pub model: String,
    /// Nested object with detailed backup status
    pub last: MockNodeLast,
    /// Variables (usually just `{enable: null}`)
    pub vars: MockNodeVars,
    /// Modification time (often "unknown")
    pub mtime: String,
    /// Top-level status (duplicates last.status)
    pub status: String,
    /// Top-level time (duplicates last.end)
    pub time: String,
}

/// Nested status object in Node.
///
/// Contains detailed timing info for the last backup attempt.
#[derive(Clone, Debug, Serialize)]
pub struct MockNodeLast {
    pub start: String,
    pub end: String,
    pub status: String,
    /// Duration in seconds (float)
    pub time: f64,
}

/// Variables object in Node (usually minimal).
#[derive(Clone, Debug, Serialize)]
pub struct MockNodeVars {
    /// Enable password (usually null)
    pub enable: Option<String>,
}

/// Mock version matching Oxidized-web API structure.
///
/// Key quirk: `author` is a nested object, not a string.
#[derive(Clone, Debug, Serialize)]
pub struct MockVersion {
    pub oid: String,
    pub date: String,
    /// Nested author object - API quirk
    pub author: MockVersionAuthor,
    pub message: String,
}

/// Nested author object in Version - API quirk.
///
/// Oxidized-web returns `{name, email, time}` not a string.
#[derive(Clone, Debug, Serialize)]
pub struct MockVersionAuthor {
    pub name: String,
    pub email: String,
    pub time: String,
}

/// Returns 3 nodes with varied statuses for testing.
///
/// Structure matches real Oxidized-web API exactly (validated 2025-12-23).
///
/// - `router-1`: success status (has config)
/// - `switch-1`: failure status (backup failed)
/// - `fw-1`: never status (never backed up)
pub fn default_nodes() -> Vec<MockNode> {
    vec![
        MockNode {
            name: "router-1".to_string(),
            full_name: "network/router-1".to_string(),
            ip: "192.168.1.1".to_string(),
            group: "network".to_string(),
            model: "cisco".to_string(),
            last: MockNodeLast {
                start: "2025-12-23 10:00:00 UTC".to_string(),
                end: "2025-12-23 10:00:05 UTC".to_string(),
                status: "success".to_string(),
                time: 5.123456,
            },
            vars: MockNodeVars { enable: None },
            mtime: "unknown".to_string(),
            status: "success".to_string(),
            time: "2025-12-23 10:00:05 UTC".to_string(),
        },
        MockNode {
            name: "switch-1".to_string(),
            full_name: "network/switch-1".to_string(),
            ip: "192.168.1.2".to_string(),
            group: "network".to_string(),
            model: "cisco".to_string(),
            last: MockNodeLast {
                start: "2025-12-23 09:00:00 UTC".to_string(),
                end: "2025-12-23 09:00:30 UTC".to_string(),
                status: "failure".to_string(),
                time: 30.0,
            },
            vars: MockNodeVars { enable: None },
            mtime: "unknown".to_string(),
            status: "failure".to_string(),
            time: "2025-12-23 09:00:30 UTC".to_string(),
        },
        MockNode {
            name: "fw-1".to_string(),
            full_name: "security/fw-1".to_string(),
            ip: "192.168.1.3".to_string(),
            group: "security".to_string(),
            model: "fortinet".to_string(),
            last: MockNodeLast {
                start: "".to_string(),
                end: "".to_string(),
                status: "never".to_string(),
                time: 0.0,
            },
            vars: MockNodeVars { enable: None },
            mtime: "unknown".to_string(),
            status: "never".to_string(),
            time: "".to_string(),
        },
    ]
}

/// Returns 3 versions for testing diff and history.
///
/// All versions have nested `author` object to reproduce API quirk.
pub fn default_versions() -> Vec<MockVersion> {
    vec![
        MockVersion {
            oid: "abc123def456".to_string(),
            date: "2025-12-23T10:00:00Z".to_string(),
            author: MockVersionAuthor {
                name: "oxidized".to_string(),
                email: "oxidized@localhost".to_string(),
                time: "2025-12-23T10:00:00Z".to_string(),
            },
            message: "Automatic backup".to_string(),
        },
        MockVersion {
            oid: "789ghi012jkl".to_string(),
            date: "2025-12-22T10:00:00Z".to_string(),
            author: MockVersionAuthor {
                name: "oxidized".to_string(),
                email: "oxidized@localhost".to_string(),
                time: "2025-12-22T10:00:00Z".to_string(),
            },
            message: "Automatic backup".to_string(),
        },
        MockVersion {
            oid: "mno345pqr678".to_string(),
            date: "2025-12-21T10:00:00Z".to_string(),
            author: MockVersionAuthor {
                name: "oxidized".to_string(),
                email: "oxidized@localhost".to_string(),
                time: "2025-12-21T10:00:00Z".to_string(),
            },
            message: "Automatic backup".to_string(),
        },
    ]
}

/// Returns sample configs keyed by node name.
///
/// Uses the existing `fixtures/large_config.txt` for router-1,
/// and a smaller inline config for switch-1.
pub fn default_configs() -> HashMap<String, String> {
    let mut configs = HashMap::new();

    // Use existing fixture file for realistic large config
    let large_config = include_str!("../../fixtures/large_config.txt");
    configs.insert("router-1".to_string(), large_config.to_string());

    // Different config for switch (smaller, inline is OK)
    configs.insert(
        "switch-1".to_string(),
        r#"!
version 15.0
hostname switch-1
!
vlan 10
 name USERS
vlan 20
 name SERVERS
!
interface Vlan10
 ip address 192.168.10.1 255.255.255.0
!
snmp-server community public RO
!
line vty 0 15
 transport input ssh telnet
!
end
"#
        .to_string(),
    );

    configs
}

/// Returns alternative config for version comparison (diff testing).
///
/// This is a modified version of router-1 config with some changes.
pub fn modified_config() -> String {
    r#"! Cisco IOS Configuration - Modified Version
! hostname changed, interface added
!
version 15.2
hostname SW-Core-01-Modified
!
aaa new-model
aaa authentication login default local
aaa authorization exec default local
!
username admin privilege 15 secret 5 $1$xxxx$xxxxxxxxxxxxxxxxxxxxxx
username readonly privilege 1 secret 5 $1$yyyy$yyyyyyyyyyyyyyyyyyyy
username newuser privilege 5 secret 5 $1$zzzz$zzzzzzzzzzzzzzzzzzzz
!
ip domain-name example.com
ip name-server 8.8.8.8
ip name-server 8.8.4.4
!
interface GigabitEthernet0/0
 description Management Interface - Updated
 ip address 10.0.0.1 255.255.255.0
 no shutdown
!
interface GigabitEthernet0/99
 description New Interface Added
 ip address 10.99.0.1 255.255.255.0
 no shutdown
!
end
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_nodes_returns_three_nodes() {
        let nodes = default_nodes();
        assert_eq!(nodes.len(), 3);
    }

    #[test]
    fn test_default_nodes_have_varied_statuses() {
        let nodes = default_nodes();
        let statuses: Vec<&str> = nodes.iter().map(|n| n.last.status.as_str()).collect();
        assert!(statuses.contains(&"success"));
        assert!(statuses.contains(&"failure"));
        assert!(statuses.contains(&"never"));
    }

    #[test]
    fn test_default_nodes_have_all_real_api_fields() {
        let nodes = default_nodes();
        for node in &nodes {
            // Nested last object with all fields
            assert!(!node.last.status.is_empty());
            // Top-level status matches last.status
            assert_eq!(node.status, node.last.status);
            // vars object exists
            assert!(node.vars.enable.is_none()); // Usually null
            // mtime exists
            assert!(!node.mtime.is_empty());
            // last.time is a valid duration
            assert!(node.last.time >= 0.0);
        }
    }

    #[test]
    fn test_default_nodes_match_real_api_format() {
        let nodes = default_nodes();
        let router = &nodes[0];

        // Verify format matches real API: "2025-12-23 10:00:00 UTC"
        assert!(router.last.start.contains("UTC"));
        assert!(router.last.end.contains("UTC"));
        assert!(router.time.contains("UTC"));
    }

    #[test]
    fn test_default_versions_returns_three_versions() {
        let versions = default_versions();
        assert_eq!(versions.len(), 3);
    }

    #[test]
    fn test_default_versions_have_nested_author() {
        let versions = default_versions();
        for version in &versions {
            assert!(!version.author.name.is_empty());
            assert!(!version.author.email.is_empty());
            assert!(!version.author.time.is_empty());
        }
    }

    #[test]
    fn test_default_configs_has_router_and_switch() {
        let configs = default_configs();
        assert!(configs.contains_key("router-1"));
        assert!(configs.contains_key("switch-1"));
    }

    #[test]
    fn test_default_configs_router_is_large() {
        let configs = default_configs();
        let router_config = configs.get("router-1").unwrap();
        // Large config should be > 100 lines
        assert!(router_config.lines().count() > 100);
    }
}
