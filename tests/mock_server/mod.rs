//! Mock server module for E2E testing without a real Oxidized server.
//!
//! This module provides `MockOxidizedServer` which reproduces the quirks
//! of the Oxidized-web 0.18.0 REST API using wiremock.
//!
//! # Oxidized-web API Quirks Reproduced
//!
//! | Quirk | Real Behavior | Mock Implementation |
//! |-------|---------------|---------------------|
//! | NodeNotFound | HTTP 500 + Ruby stack trace | 500 + "unable to find 'NAME'" |
//! | VersionAuthor | Nested `{name, email, time}` | `MockVersionAuthor` struct |
//! | Stats endpoint | 404 (buggy) | Return 404, use `Stats::from_nodes()` |
//! | conf_search | POST returns HTML | HTML with `<td>node_name</td>` |
//! | Node.last | Nested `{status, start, end}` | Include in mock node JSON |
//!
//! # Usage
//!
//! ```no_run
//! use mock_server::{MockOxidizedServer, default_nodes};
//!
//! #[tokio::test]
//! async fn test_example() {
//!     let mock = MockOxidizedServer::start()
//!         .await
//!         .with_nodes(default_nodes());
//!     mock.mount_all().await;
//!
//!     // Create client pointing to mock.uri() and test...
//! }
//! ```

mod fixtures;
mod server;

pub use fixtures::*;
pub use server::MockOxidizedServer;
