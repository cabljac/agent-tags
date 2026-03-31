/**
 * @agents
 * Reference graph built from @agents Related:/See: links.
 * Nodes = files with headers, edges = typed references.
 * Related: git-agent-tags/src/parser.rs, git-agent-tags/src/cache.rs, git-agent-tags/src/check.rs
 */

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub file: String,
    pub related: Vec<String>,
    pub see: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ReferenceGraph {
    /// file path → node
    nodes: HashMap<String, GraphNode>,
    /// file path → set of named tag names defined in that file
    #[serde(default)]
    tag_names: HashMap<String, HashSet<String>>,
    /// Reverse index: target file (base path, no fragment) → source files that reference it
    #[serde(skip)]
    reverse: HashMap<String, Vec<String>>,
}

impl ReferenceGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_node(&mut self, node: GraphNode) {
        for r in node.related.iter().chain(node.see.iter()) {
            if r.starts_with("http://") || r.starts_with("https://") {
                continue;
            }
            let base = r.split_once('#').map_or(r.as_str(), |(b, _)| b);
            self.reverse
                .entry(base.to_string())
                .or_default()
                .push(node.file.clone());
        }
        self.nodes.insert(node.file.clone(), node);
    }

    /// Register the set of named tags defined in a file, for fragment validation.
    pub fn register_tag_names(&mut self, file: &str, names: HashSet<String>) {
        if !names.is_empty() {
            self.tag_names.insert(file.to_string(), names);
        }
    }

    /// Files that this file points to (outgoing edges).
    pub fn dependencies(&self, file: &str) -> Vec<String> {
        match self.nodes.get(file) {
            Some(n) => {
                let mut deps = n.related.clone();
                deps.extend(n.see.clone());
                deps
            }
            None => vec![],
        }
    }

    /// Files that point to this file (incoming edges). O(1) via reverse index.
    pub fn dependents(&self, file: &str) -> Vec<String> {
        self.reverse.get(file).cloned().unwrap_or_default()
    }

    /// Files with headers but no incoming or outgoing links.
    #[allow(dead_code)]
    pub fn orphans(&self) -> Vec<String> {
        self.nodes
            .keys()
            .filter(|f| self.dependencies(f).is_empty() && self.dependents(f).is_empty())
            .cloned()
            .collect()
    }

    /// Edges pointing to files that don't exist or fragments that don't resolve.
    /// Returns (source_file, broken_ref).
    pub fn broken_refs(&self, existing_files: &HashSet<String>) -> Vec<(String, String)> {
        let mut broken = Vec::new();
        for node in self.nodes.values() {
            for r in node.related.iter().chain(node.see.iter()) {
                // Skip URLs
                if r.starts_with("http://") || r.starts_with("https://") {
                    continue;
                }
                // Split on # for fragment references
                let (base_path, fragment) = match r.split_once('#') {
                    Some((base, frag)) => (base, Some(frag)),
                    None => (r.as_str(), None),
                };

                let file_exists =
                    existing_files.contains(base_path) || self.nodes.contains_key(base_path);

                if !file_exists {
                    broken.push((node.file.clone(), r.clone()));
                } else if let Some(frag) = fragment {
                    // File exists — validate the fragment
                    let frag_valid = self
                        .tag_names
                        .get(base_path)
                        .map_or(false, |names| names.contains(frag));
                    if !frag_valid {
                        broken.push((node.file.clone(), r.clone()));
                    }
                }
            }
        }
        broken.sort();
        broken
    }

    pub fn all_files(&self) -> Vec<&str> {
        self.nodes.keys().map(|s| s.as_str()).collect()
    }

    pub fn get_node(&self, file: &str) -> Option<&GraphNode> {
        self.nodes.get(file)
    }

    #[allow(dead_code)]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_graph() -> ReferenceGraph {
        let mut g = ReferenceGraph::new();
        g.add_node(GraphNode {
            file: "auth.ts".into(),
            related: vec!["auth-guard.ts".into(), "types/auth.d.ts".into()],
            see: vec![],
        });
        g.add_node(GraphNode {
            file: "auth-guard.ts".into(),
            related: vec!["auth.ts".into()],
            see: vec![],
        });
        g.add_node(GraphNode {
            file: "orphan.ts".into(),
            related: vec![],
            see: vec![],
        });
        g
    }

    #[test]
    fn test_dependencies() {
        let g = make_graph();
        let deps = g.dependencies("auth.ts");
        assert!(deps.contains(&"auth-guard.ts".to_string()));
        assert!(deps.contains(&"types/auth.d.ts".to_string()));
    }

    #[test]
    fn test_dependents() {
        let g = make_graph();
        let deps = g.dependents("auth.ts");
        assert!(deps.contains(&"auth-guard.ts".to_string()));
    }

    #[test]
    fn test_orphans() {
        let g = make_graph();
        let orphans = g.orphans();
        assert!(orphans.contains(&"orphan.ts".to_string()));
        assert!(!orphans.contains(&"auth.ts".to_string()));
    }

    #[test]
    fn test_broken_refs() {
        let g = make_graph();
        let existing: HashSet<String> = ["auth.ts", "auth-guard.ts", "orphan.ts"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let broken = g.broken_refs(&existing);
        // types/auth.d.ts is not in existing or graph
        assert!(broken.iter().any(|(_, r)| r == "types/auth.d.ts"));
        // auth.ts and auth-guard.ts reference each other and both exist
        assert!(!broken.iter().any(|(_, r)| r == "auth.ts" || r == "auth-guard.ts"));
    }

    #[test]
    fn test_broken_refs_skips_urls() {
        let mut g = ReferenceGraph::new();
        g.add_node(GraphNode {
            file: "api.ts".into(),
            related: vec![],
            see: vec!["https://docs.example.com/api".into()],
        });
        let existing: HashSet<String> = ["api.ts"].iter().map(|s| s.to_string()).collect();
        let broken = g.broken_refs(&existing);
        assert!(broken.is_empty(), "URLs should not be flagged as broken");
    }

    #[test]
    fn test_dependents_via_see() {
        let mut g = ReferenceGraph::new();
        g.add_node(GraphNode {
            file: "consumer.ts".into(),
            related: vec![],
            see: vec!["spec.ts".into()],
        });
        g.add_node(GraphNode {
            file: "spec.ts".into(),
            related: vec![],
            see: vec![],
        });
        let deps = g.dependents("spec.ts");
        assert!(deps.contains(&"consumer.ts".to_string()));
    }

    #[test]
    fn test_fragment_ref_valid() {
        let mut g = ReferenceGraph::new();
        g.add_node(GraphNode {
            file: "consumer.ts".into(),
            related: vec!["auth.ts#token-check".into()],
            see: vec![],
        });
        g.register_tag_names("auth.ts", HashSet::from(["token-check".to_string()]));
        let existing: HashSet<String> = ["consumer.ts", "auth.ts"]
            .iter().map(|s| s.to_string()).collect();
        let broken = g.broken_refs(&existing);
        assert!(broken.is_empty(), "Valid fragment ref should not be broken");
    }

    #[test]
    fn test_fragment_ref_broken_fragment() {
        let mut g = ReferenceGraph::new();
        g.add_node(GraphNode {
            file: "consumer.ts".into(),
            related: vec!["auth.ts#nonexistent".into()],
            see: vec![],
        });
        g.register_tag_names("auth.ts", HashSet::from(["token-check".to_string()]));
        let existing: HashSet<String> = ["consumer.ts", "auth.ts"]
            .iter().map(|s| s.to_string()).collect();
        let broken = g.broken_refs(&existing);
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].1, "auth.ts#nonexistent");
    }

    #[test]
    fn test_fragment_ref_missing_file() {
        let mut g = ReferenceGraph::new();
        g.add_node(GraphNode {
            file: "consumer.ts".into(),
            related: vec!["missing.ts#tag".into()],
            see: vec![],
        });
        let existing: HashSet<String> = ["consumer.ts"]
            .iter().map(|s| s.to_string()).collect();
        let broken = g.broken_refs(&existing);
        assert_eq!(broken.len(), 1);
        assert_eq!(broken[0].1, "missing.ts#tag");
    }

    #[test]
    fn test_dependents_with_fragment_ref() {
        let mut g = ReferenceGraph::new();
        g.add_node(GraphNode {
            file: "consumer.ts".into(),
            related: vec!["auth.ts#token-check".into()],
            see: vec![],
        });
        g.add_node(GraphNode {
            file: "auth.ts".into(),
            related: vec![],
            see: vec![],
        });
        let deps = g.dependents("auth.ts");
        assert!(deps.contains(&"consumer.ts".to_string()));
    }

    #[test]
    fn test_no_self_loop_in_orphans() {
        // A file that references itself should not be an orphan
        let mut g = ReferenceGraph::new();
        g.add_node(GraphNode {
            file: "weird.ts".into(),
            related: vec!["weird.ts".into()],
            see: vec![],
        });
        let orphans = g.orphans();
        assert!(!orphans.contains(&"weird.ts".to_string()));
    }
}
