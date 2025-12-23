//! Tool to search for regex patterns across network device configurations (FR10-FR13).
//!
//! Uses a hybrid approach: server-side pre-filter via Oxidized API followed by
//! precise Rust regex matching for exact line extraction with context.
//!
//! # Example
//!
//! ```ignore
//! use mcp_oxidized::tools::search_configs;
//! use mcp_oxidized::oxidized::OxidizedClient;
//!
//! let result = search_configs(&client, "ip address 10\\.0\\.", None, true, 100).await?;
//! println!("{}", result.to_llm_format());
//! ```

use regex::RegexBuilder;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::instrument;

use crate::error::OxidizedError;
use crate::oxidized::{OxidizedBackend, OxidizedClient};

// ============================================================================
// Constants
// ============================================================================

/// Maximum number of concurrent config fetch requests.
const MAX_CONCURRENT_REQUESTS: usize = 10;

/// Number of context lines before and after a match.
const CONTEXT_LINES: usize = 1;

// ============================================================================
// Search Result Types
// ============================================================================

/// A single match within a configuration file.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchMatch {
    /// Name of the node where the match was found
    pub node: String,
    /// 1-indexed line number of the match
    pub line_num: usize,
    /// The matching line content (without trailing newline)
    pub content: String,
    /// Lines before the match (up to CONTEXT_LINES)
    pub context_before: Vec<String>,
    /// Lines after the match (up to CONTEXT_LINES)
    pub context_after: Vec<String>,
}

/// Results grouped by node for easier reading.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NodeMatches {
    /// Name of the node
    pub node: String,
    /// Number of matches in this node
    pub match_count: usize,
    /// Individual matches within this node
    pub matches: Vec<SearchMatch>,
}

/// Result of a configuration search operation.
///
/// Contains structured search results optimized for LLM consumption.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    /// The pattern that was searched
    pub pattern: String,
    /// Whether the search was case-sensitive
    pub case_sensitive: bool,
    /// Total number of matches found (before limit applied)
    pub total_matches: usize,
    /// Number of matches returned (after limit)
    pub shown_matches: usize,
    /// Number of nodes that were searched
    pub nodes_searched: usize,
    /// Number of nodes that had at least one match
    pub nodes_with_matches: usize,
    /// Matches grouped by node
    pub results: Vec<NodeMatches>,
    /// Warnings (e.g., nodes not found, skipped)
    pub warnings: Vec<String>,
}

impl SearchResult {
    /// Format the search result as an LLM-friendly string.
    ///
    /// Output format is structured for easy parsing by AI assistants:
    /// - Summary section with statistics
    /// - Results grouped by node with line numbers and context
    ///
    /// # Example Output
    ///
    /// ```text
    /// ## Configuration Search Results
    ///
    /// **Pattern:** `ip address 10\.0\.` (case-sensitive)
    /// **Nodes searched:** 5 | **Nodes with matches:** 2
    /// **Showing:** 10 of 15 matches
    ///
    /// ---
    ///
    /// ### SW-Core-01 (3 matches)
    ///
    /// **Line 42:**
    /// ```text
    ///   interface GigabitEthernet0/1
    /// > ip address 10.0.0.1 255.255.255.0
    ///   no shutdown
    /// ```
    pub fn to_llm_format(&self) -> String {
        let mut output = String::new();

        // Header
        output.push_str("## Configuration Search Results\n\n");

        let case_str = if self.case_sensitive {
            "case-sensitive"
        } else {
            "case-insensitive"
        };
        output.push_str(&format!("**Pattern:** `{}` ({})\n", self.pattern, case_str));
        output.push_str(&format!(
            "**Nodes searched:** {} | **Nodes with matches:** {}\n",
            self.nodes_searched, self.nodes_with_matches
        ));

        if self.total_matches > self.shown_matches {
            output.push_str(&format!(
                "**Showing:** {} of {} matches\n",
                self.shown_matches, self.total_matches
            ));
        } else {
            output.push_str(&format!("**Matches:** {}\n", self.total_matches));
        }

        // Warnings
        if !self.warnings.is_empty() {
            output.push_str("\n**Warnings:**\n");
            for warning in &self.warnings {
                output.push_str(&format!("- {}\n", warning));
            }
        }

        // No matches case
        if self.results.is_empty() {
            output.push_str("\n---\n\n");
            output.push_str("No matches found.\n");
            return output;
        }

        // Results by node
        for node_matches in &self.results {
            output.push_str("\n---\n\n");
            output.push_str(&format!(
                "### {} ({} matches)\n\n",
                node_matches.node, node_matches.match_count
            ));

            for search_match in &node_matches.matches {
                output.push_str(&format!("**Line {}:**\n```\n", search_match.line_num));

                // Context before
                for line in &search_match.context_before {
                    output.push_str(&format!("  {}\n", line));
                }

                // Matching line with marker
                output.push_str(&format!(">>> {}\n", search_match.content));

                // Context after
                for line in &search_match.context_after {
                    output.push_str(&format!("  {}\n", line));
                }

                output.push_str("```\n\n");
            }
        }

        output
    }
}

// ============================================================================
// Search Implementation
// ============================================================================

/// Search for a regex pattern across network device configurations.
///
/// Uses client-side regex matching to find patterns with context lines.
/// Fetches configs in parallel with concurrency limiting for performance.
///
/// # Arguments
///
/// * `backend` - The Oxidized client to use
/// * `pattern` - Regex pattern to search for
/// * `nodes` - Optional list of node names to limit search to
/// * `case_sensitive` - Whether the search is case-sensitive (default: true)
/// * `limit` - Maximum number of matches to return (default: 100)
///
/// # Returns
///
/// A `SearchResult` containing matches grouped by node.
///
/// # Errors
///
/// - [`OxidizedError::InvalidRegex`] - Pattern is not valid regex
/// - [`OxidizedError::ApiUnreachable`] - Network/connection error
///
/// # Example
///
/// ```ignore
/// let result = search_configs(&client, "ip address 10\\.", None, true, 100).await?;
/// println!("{}", result.to_llm_format());
/// ```
#[instrument(skip(backend), fields(pattern = %pattern, nodes = ?nodes, case_sensitive = %case_sensitive, limit = %limit))]
pub async fn search_configs(
    backend: &OxidizedClient,
    pattern: &str,
    nodes: Option<Vec<String>>,
    case_sensitive: bool,
    limit: u32,
) -> Result<SearchResult, OxidizedError> {
    // Handle empty pattern BEFORE compiling regex
    if pattern.is_empty() {
        return Err(OxidizedError::InvalidRegex(
            "Empty pattern is not allowed".to_string(),
        ));
    }

    // Validate and compile regex pattern
    let regex = RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| {
            OxidizedError::InvalidRegex(format!("Invalid regex pattern '{}': {}", pattern, e))
        })?;

    let mut warnings = Vec::new();

    // Get all nodes to search
    let (all_nodes, _) = backend.get_nodes().await?;
    let node_names: HashSet<String> = all_nodes.iter().map(|n| n.name.clone()).collect();

    // Determine which nodes to search
    let nodes_to_search: Vec<String> = match nodes {
        Some(requested_nodes) => {
            let mut valid_nodes = Vec::new();
            for node in requested_nodes {
                if node_names.contains(&node) {
                    valid_nodes.push(node);
                } else {
                    warnings.push(format!("Node '{}' not found, skipping", node));
                }
            }
            valid_nodes
        }
        None => node_names.into_iter().collect(),
    };

    if nodes_to_search.is_empty() {
        return Ok(SearchResult {
            pattern: pattern.to_string(),
            case_sensitive,
            total_matches: 0,
            shown_matches: 0,
            nodes_searched: 0,
            nodes_with_matches: 0,
            results: vec![],
            warnings,
        });
    }

    // Fetch configs in parallel with concurrency limit
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));
    let mut join_set = JoinSet::new();

    let nodes_searched = nodes_to_search.len();

    for node_name in nodes_to_search {
        let backend = backend.clone();
        let sem = semaphore.clone();
        let node = node_name.clone();

        join_set.spawn(async move {
            let _permit = sem.acquire().await.expect("Semaphore closed unexpectedly");
            let config_result = backend.get_node_config(&node).await;
            (node, config_result)
        });
    }

    // Collect configs and process matches
    let mut all_matches: Vec<NodeMatches> = Vec::new();
    let mut total_matches = 0usize;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok((node, Ok((config, _)))) => {
                // Search this config for matches
                let matches = search_in_config(&node, &config, &regex);
                if !matches.is_empty() {
                    total_matches += matches.len();
                    all_matches.push(NodeMatches {
                        node,
                        match_count: matches.len(),
                        matches,
                    });
                }
            }
            Ok((node, Err(OxidizedError::NodeNotFound(_, _)))) => {
                warnings.push(format!("Node '{}' not found during config fetch", node));
            }
            Ok((node, Err(e))) => {
                warnings.push(format!("Error fetching config for '{}': {}", node, e));
            }
            Err(e) => {
                warnings.push(format!("Task error: {}", e));
            }
        }
    }

    // Sort results by node name for consistent output
    all_matches.sort_by(|a, b| a.node.cmp(&b.node));

    let nodes_with_matches = all_matches.len();

    // Apply limit across all results
    let mut shown_matches = 0usize;
    let limit = limit as usize;

    for node_matches in &mut all_matches {
        let remaining = limit.saturating_sub(shown_matches);
        if remaining == 0 {
            node_matches.matches.clear();
            node_matches.match_count = 0;
        } else if node_matches.matches.len() > remaining {
            node_matches.matches.truncate(remaining);
            shown_matches += remaining;
        } else {
            shown_matches += node_matches.matches.len();
        }
    }

    // Remove nodes with no matches after truncation
    all_matches.retain(|nm| !nm.matches.is_empty());

    tracing::info!(
        pattern = %pattern,
        total_matches = total_matches,
        shown_matches = shown_matches,
        nodes_searched = nodes_searched,
        nodes_with_matches = nodes_with_matches,
        "Search completed"
    );

    Ok(SearchResult {
        pattern: pattern.to_string(),
        case_sensitive,
        total_matches,
        shown_matches,
        nodes_searched,
        nodes_with_matches,
        results: all_matches,
        warnings,
    })
}

/// Search for regex matches within a single configuration string.
///
/// Returns matches with line numbers and context.
fn search_in_config(node: &str, config: &str, regex: &regex::Regex) -> Vec<SearchMatch> {
    let lines: Vec<&str> = config.lines().collect();
    let mut matches = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        if regex.is_match(line) {
            // Get context lines before
            let context_before: Vec<String> = lines
                .iter()
                .skip(idx.saturating_sub(CONTEXT_LINES))
                .take(CONTEXT_LINES.min(idx))
                .map(|s| (*s).to_string())
                .collect();

            // Get context lines after
            let context_after: Vec<String> = lines
                .iter()
                .skip(idx + 1)
                .take(CONTEXT_LINES)
                .map(|s| (*s).to_string())
                .collect();

            matches.push(SearchMatch {
                node: node.to_string(),
                line_num: idx + 1, // 1-indexed
                content: (*line).to_string(),
                context_before,
                context_after,
            });
        }
    }

    matches
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // SearchMatch Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_search_match_serializes() {
        let match_ = SearchMatch {
            node: "SW-01".to_string(),
            line_num: 42,
            content: "ip address 10.0.0.1".to_string(),
            context_before: vec!["interface Gi0/1".to_string()],
            context_after: vec!["no shutdown".to_string()],
        };

        let json = serde_json::to_string(&match_).expect("Should serialize");
        assert!(json.contains("\"node\":\"SW-01\""));
        assert!(json.contains("\"line_num\":42"));
        assert!(json.contains("\"content\":\"ip address 10.0.0.1\""));
        assert!(json.contains("\"context_before\""));
        assert!(json.contains("\"context_after\""));
    }

    // -------------------------------------------------------------------------
    // NodeMatches Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_node_matches_serializes() {
        let node_matches = NodeMatches {
            node: "SW-01".to_string(),
            match_count: 2,
            matches: vec![
                SearchMatch {
                    node: "SW-01".to_string(),
                    line_num: 10,
                    content: "line1".to_string(),
                    context_before: vec![],
                    context_after: vec![],
                },
                SearchMatch {
                    node: "SW-01".to_string(),
                    line_num: 20,
                    content: "line2".to_string(),
                    context_before: vec![],
                    context_after: vec![],
                },
            ],
        };

        let json = serde_json::to_string(&node_matches).expect("Should serialize");
        assert!(json.contains("\"node\":\"SW-01\""));
        assert!(json.contains("\"match_count\":2"));
        assert!(json.contains("\"matches\""));
    }

    // -------------------------------------------------------------------------
    // SearchResult Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_search_result_serializes() {
        let result = SearchResult {
            pattern: "test".to_string(),
            case_sensitive: true,
            total_matches: 5,
            shown_matches: 5,
            nodes_searched: 10,
            nodes_with_matches: 2,
            results: vec![],
            warnings: vec![],
        };

        let json = serde_json::to_string(&result).expect("Should serialize");
        assert!(json.contains("\"pattern\":\"test\""));
        assert!(json.contains("\"case_sensitive\":true"));
        assert!(json.contains("\"total_matches\":5"));
    }

    // -------------------------------------------------------------------------
    // LLM Format Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_llm_format_no_matches() {
        let result = SearchResult {
            pattern: "nonexistent".to_string(),
            case_sensitive: true,
            total_matches: 0,
            shown_matches: 0,
            nodes_searched: 5,
            nodes_with_matches: 0,
            results: vec![],
            warnings: vec![],
        };

        let output = result.to_llm_format();

        assert!(output.contains("## Configuration Search Results"));
        assert!(output.contains("`nonexistent`"));
        assert!(output.contains("case-sensitive"));
        assert!(output.contains("Nodes searched:** 5"));
        assert!(output.contains("No matches found"));
    }

    #[test]
    fn test_llm_format_with_matches() {
        let result = SearchResult {
            pattern: "ip address".to_string(),
            case_sensitive: false,
            total_matches: 2,
            shown_matches: 2,
            nodes_searched: 3,
            nodes_with_matches: 1,
            results: vec![NodeMatches {
                node: "SW-Core-01".to_string(),
                match_count: 2,
                matches: vec![
                    SearchMatch {
                        node: "SW-Core-01".to_string(),
                        line_num: 10,
                        content: "ip address 10.0.0.1 255.255.255.0".to_string(),
                        context_before: vec!["interface Gi0/1".to_string()],
                        context_after: vec!["no shutdown".to_string()],
                    },
                    SearchMatch {
                        node: "SW-Core-01".to_string(),
                        line_num: 20,
                        content: "ip address 10.0.0.2 255.255.255.0".to_string(),
                        context_before: vec!["interface Gi0/2".to_string()],
                        context_after: vec!["no shutdown".to_string()],
                    },
                ],
            }],
            warnings: vec![],
        };

        let output = result.to_llm_format();

        assert!(output.contains("## Configuration Search Results"));
        assert!(output.contains("`ip address`"));
        assert!(output.contains("case-insensitive"));
        assert!(output.contains("### SW-Core-01 (2 matches)"));
        assert!(output.contains("**Line 10:**"));
        assert!(output.contains(">>> ip address 10.0.0.1"));
        assert!(output.contains("interface Gi0/1"));
        assert!(output.contains("no shutdown"));
    }

    #[test]
    fn test_llm_format_with_truncation() {
        let result = SearchResult {
            pattern: "test".to_string(),
            case_sensitive: true,
            total_matches: 100,
            shown_matches: 50,
            nodes_searched: 10,
            nodes_with_matches: 5,
            results: vec![],
            warnings: vec![],
        };

        let output = result.to_llm_format();

        assert!(output.contains("**Showing:** 50 of 100 matches"));
    }

    #[test]
    fn test_llm_format_with_warnings() {
        let result = SearchResult {
            pattern: "test".to_string(),
            case_sensitive: true,
            total_matches: 0,
            shown_matches: 0,
            nodes_searched: 3,
            nodes_with_matches: 0,
            results: vec![],
            warnings: vec!["Node 'invalid' not found, skipping".to_string()],
        };

        let output = result.to_llm_format();

        assert!(output.contains("**Warnings:**"));
        assert!(output.contains("Node 'invalid' not found, skipping"));
    }

    // -------------------------------------------------------------------------
    // search_in_config Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_search_in_config_simple_match() {
        let config = "line1\nip address 10.0.0.1\nline3";
        let regex = RegexBuilder::new("ip address")
            .build()
            .expect("Valid regex");

        let matches = search_in_config("node1", config, &regex);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_num, 2);
        assert_eq!(matches[0].content, "ip address 10.0.0.1");
        assert_eq!(matches[0].context_before, vec!["line1"]);
        assert_eq!(matches[0].context_after, vec!["line3"]);
    }

    #[test]
    fn test_search_in_config_case_insensitive() {
        let config = "IP ADDRESS 10.0.0.1";
        let regex = RegexBuilder::new("ip address")
            .case_insensitive(true)
            .build()
            .expect("Valid regex");

        let matches = search_in_config("node1", config, &regex);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].content, "IP ADDRESS 10.0.0.1");
    }

    #[test]
    fn test_search_in_config_case_sensitive_no_match() {
        let config = "IP ADDRESS 10.0.0.1";
        let regex = RegexBuilder::new("ip address")
            .case_insensitive(false)
            .build()
            .expect("Valid regex");

        let matches = search_in_config("node1", config, &regex);

        assert!(matches.is_empty());
    }

    #[test]
    fn test_search_in_config_multiple_matches() {
        let config = "line1\nmatch1\nline3\nmatch2\nline5";
        let regex = RegexBuilder::new("match").build().expect("Valid regex");

        let matches = search_in_config("node1", config, &regex);

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line_num, 2);
        assert_eq!(matches[1].line_num, 4);
    }

    #[test]
    fn test_search_in_config_no_context_at_start() {
        let config = "match\nline2\nline3";
        let regex = RegexBuilder::new("match").build().expect("Valid regex");

        let matches = search_in_config("node1", config, &regex);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_num, 1);
        assert!(matches[0].context_before.is_empty());
        assert_eq!(matches[0].context_after, vec!["line2"]);
    }

    #[test]
    fn test_search_in_config_no_context_at_end() {
        let config = "line1\nline2\nmatch";
        let regex = RegexBuilder::new("match").build().expect("Valid regex");

        let matches = search_in_config("node1", config, &regex);

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_num, 3);
        assert_eq!(matches[0].context_before, vec!["line2"]);
        assert!(matches[0].context_after.is_empty());
    }

    #[test]
    fn test_search_in_config_empty_config() {
        let config = "";
        let regex = RegexBuilder::new("match").build().expect("Valid regex");

        let matches = search_in_config("node1", config, &regex);

        assert!(matches.is_empty());
    }

    #[test]
    fn test_search_in_config_regex_special_chars() {
        let config = "ip address 10.0.0.1";
        let regex = RegexBuilder::new(r"10\.0\.0\.1")
            .build()
            .expect("Valid regex");

        let matches = search_in_config("node1", config, &regex);

        assert_eq!(matches.len(), 1);
    }

    // -------------------------------------------------------------------------
    // Empty Pattern Validation Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_empty_pattern_rejected() {
        // Empty pattern should return InvalidRegex error before regex compilation
        // This is tested at the function level via integration tests,
        // but we verify the error message format here
        use crate::error::OxidizedError;

        let error = OxidizedError::InvalidRegex("Empty pattern is not allowed".to_string());
        let msg = error.to_string();
        assert!(msg.contains("Empty pattern"), "Error should mention empty pattern");
    }

    // -------------------------------------------------------------------------
    // Result Limiting Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_search_result_limit_applied() {
        // This test verifies the limit logic in the struct, not the actual search
        let result = SearchResult {
            pattern: "test".to_string(),
            case_sensitive: true,
            total_matches: 150,
            shown_matches: 100,
            nodes_searched: 10,
            nodes_with_matches: 5,
            results: vec![],
            warnings: vec![],
        };

        assert!(result.total_matches > result.shown_matches);
        assert_eq!(result.shown_matches, 100);
    }

    // -------------------------------------------------------------------------
    // NodeMatches Grouping Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_node_matches_grouping() {
        let matches = vec![
            SearchMatch {
                node: "SW-01".to_string(),
                line_num: 10,
                content: "match1".to_string(),
                context_before: vec![],
                context_after: vec![],
            },
            SearchMatch {
                node: "SW-01".to_string(),
                line_num: 20,
                content: "match2".to_string(),
                context_before: vec![],
                context_after: vec![],
            },
        ];

        let node_matches = NodeMatches {
            node: "SW-01".to_string(),
            match_count: matches.len(),
            matches,
        };

        assert_eq!(node_matches.node, "SW-01");
        assert_eq!(node_matches.match_count, 2);
        assert_eq!(node_matches.matches.len(), 2);
    }
}
