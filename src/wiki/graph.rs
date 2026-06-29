//! Wiki graph engine — loads `profile/**/*.md`, parses frontmatter + body +
//! `[[wikilinks]]`, and provides search/list/get operations for the query agent.
//!
//! ponytail: in-memory graph. The wiki is small (<100 nodes). No DB cache,
//! no embeddings, no token counting. The model self-limits via declared budget.

use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A single wiki node (one .md file).
#[derive(Debug, Clone)]
pub struct WikiNode {
    pub path:      PathBuf,
    pub title:     String,
    pub summary:   String,       // first 200 chars of body
    pub body:      String,       // full markdown body (no frontmatter)
    pub links:     Vec<String>,  // outgoing [[wikilinks]] (resolved names)
    pub frontmatter: HashMap<String, String>,
    pub updated_at: Option<std::time::SystemTime>,
}

/// The in-memory knowledge graph.
#[derive(Debug, Clone)]
pub struct WikiGraph {
    pub nodes:     HashMap<String, WikiNode>,  // key: lowercase title
    pub index:     String,                     // System/index.md body
    pub raw_dir:   PathBuf,                    // profile/raw/
    pub wiki_dir:  PathBuf,                    // profile/wiki/
}

impl WikiGraph {
    /// Walk `profile/` and load all .md files. Returns an empty graph if dir missing.
    pub fn load(profile_dir: &Path) -> Result<Self> {
        let mut nodes = HashMap::new();
        let wiki_dir = profile_dir.join("wiki");
        let raw_dir = profile_dir.join("raw");

        // Load index.md (legacy + System/index.md)
        let index_body = Self::read_file_body(&profile_dir.join("index.md"));
        let system_index = Self::read_file_body(&profile_dir.join("System").join("index.md"));
        let index = if !system_index.is_empty() { system_index } else { index_body };

        // Walk wiki/ and profile root .md files (but not System/ or raw/)
        let mut md_files = Vec::new();
        Self::walk_md(&profile_dir, &profile_dir, &mut md_files);
        // Also walk wiki/ if it exists separately
        if wiki_dir.exists() {
            Self::walk_md(&wiki_dir, &wiki_dir, &mut md_files);
        }

        for path in &md_files {
            // Skip System/ and raw/ directories
            let rel = path.strip_prefix(profile_dir).unwrap_or(path);
            let rel_str = rel.to_string_lossy();
            if rel_str.starts_with("System") || rel_str.starts_with("raw") {
                continue;
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let (frontmatter, body) = Self::parse_file(&content);
            let title = frontmatter.get("name")
                .or_else(|| frontmatter.get("title"))
                .cloned()
                .unwrap_or_else(|| {
                    path.file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                });

            let links = Self::extract_wikilinks(&body);
            let summary = body.chars().take(200).collect::<String>();
            let updated_at = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());

            let node = WikiNode {
                path: path.clone(),
                title: title.clone(),
                summary,
                body,
                links,
                frontmatter,
                updated_at,
            };
            nodes.insert(title.to_lowercase(), node);
        }

        // Resolve link names to lowercase for consistent lookup
        for node in nodes.values_mut() {
            node.links = node.links.iter().map(|l| l.to_lowercase()).collect();
        }

        Ok(Self { nodes, index, raw_dir, wiki_dir })
    }

    /// Get a node by name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<&WikiNode> {
        self.nodes.get(&name.to_lowercase())
    }

    /// List all nodes with title + summary. Filter by substring if provided.
    pub fn list(&self, filter: Option<&str>) -> Vec<(&str, &str)> {
        let mut result: Vec<_> = self.nodes.values()
            .map(|n| (n.title.as_str(), n.summary.as_str()))
            .collect();
        if let Some(q) = filter {
            let q_lower = q.to_lowercase();
            result.retain(|(title, summary)| {
                title.to_lowercase().contains(&q_lower)
                    || summary.to_lowercase().contains(&q_lower)
            });
        }
        result.sort_by(|a, b| a.0.cmp(b.0));
        result
    }

    /// Search nodes by query (case-insensitive substring on title + body).
    pub fn search(&self, query: &str) -> Vec<&WikiNode> {
        let q = query.to_lowercase();
        let mut results: Vec<_> = self.nodes.values()
            .filter(|n| {
                n.title.to_lowercase().contains(&q)
                    || n.body.to_lowercase().contains(&q)
                    || n.summary.to_lowercase().contains(&q)
            })
            .collect();
        results.sort_by(|a, b| a.title.cmp(&b.title));
        results
    }

    /// Get outgoing links from a node.
    pub fn get_links(&self, name: &str) -> Vec<&str> {
        self.get(name)
            .map(|n| n.links.iter().map(|l| l.as_str()).collect())
            .unwrap_or_default()
    }

    /// Get nodes that link TO a given node (backlinks).
    pub fn get_backlinks(&self, name: &str) -> Vec<&str> {
        let target = name.to_lowercase();
        self.nodes.values()
            .filter(|n| n.links.contains(&target))
            .map(|n| n.title.as_str())
            .collect()
    }

    /// Read the index file body.
    pub fn index_body(&self) -> &str {
        &self.index
    }

    /// Total node count.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the graph is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    // -- private helpers --

    fn walk_md(dir: &Path, base: &Path, files: &mut Vec<PathBuf>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                Self::walk_md(&path, base, files);
            } else if path.extension().map_or(false, |e| e == "md") {
                files.push(path);
            }
        }
    }

    fn read_file_body(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_default()
    }

    /// Parse a .md file into (frontmatter, body).
    fn parse_file(content: &str) -> (HashMap<String, String>, String) {
        let fm = crate::profile::parse_frontmatter(content);
        let body = crate::profile::strip_frontmatter(content).to_string();
        (fm, body)
    }

    /// Extract all [[wikilinks]] from content. Returns link targets (before |).
    fn extract_wikilinks(content: &str) -> Vec<String> {
        let mut links = Vec::new();
        let mut remaining = content;
        while let Some(start) = remaining.find("[[") {
            let after = &remaining[start + 2..];
            if let Some(end) = after.find("]]") {
                let link = &after[..end];
                // Skip image embeds (preceded by !)
                if start > 0 && remaining.as_bytes().get(start - 1) == Some(&b'!') {
                    remaining = &after[end + 2..];
                    continue;
                }
                let target = link.split_once('|').map(|(t, _)| t).unwrap_or(link);
                links.push(target.to_string());
                remaining = &after[end + 2..];
            } else {
                break;
            }
        }
        links
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_wikilinks_basic() {
        let links = WikiGraph::extract_wikilinks("See [[Rust]] and [[Docker|containers]]");
        assert_eq!(links, vec!["Rust", "Docker"]);
    }

    #[test]
    fn extract_wikilinks_skips_images() {
        let links = WikiGraph::extract_wikilinks("![[portfolio/img.svg]] and [[Rust]]");
        assert_eq!(links, vec!["Rust"]);
    }

    #[test]
    fn extract_wikilinks_empty() {
        let links = WikiGraph::extract_wikilinks("no links here");
        assert!(links.is_empty());
    }

    #[test]
    fn load_graph_from_profile() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("profile");
        if !dir.exists() { return; }
        let graph = WikiGraph::load(&dir).expect("should load");
        assert!(!graph.is_empty(), "graph should have nodes");
        // Should at least have index.md content
        assert!(!graph.index_body().is_empty(), "index should not be empty");
    }
}
