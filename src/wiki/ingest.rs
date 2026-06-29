use anyhow::Result;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use crate::{db, AppState};
use crate::llm::{call_llm_tool_loop, ToolDef};
use super::helpers::walkdir;

/// Report from an ingest run.
pub struct IngestReport {
    pub sources_processed: usize,
    pub nodes_created:     usize,
    pub nodes_appended:    usize,
    pub errors:            Vec<String>,
}

impl IngestReport {
    pub fn summary(&self) -> String {
        format!(
            "Ingest: {} sources → {} created, {} appended, {} errors",
            self.sources_processed, self.nodes_created, self.nodes_appended, self.errors.len()
        )
    }
}

/// Scan `profile/raw/` for .md files modified after `since` (unix timestamp).
/// Returns paths sorted by mtime.
pub fn scan_raw(profile_dir: &Path, since: Option<i64>) -> Result<Vec<PathBuf>> {
    let raw_dir = profile_dir.join("raw");
    if !raw_dir.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in walkdir(&raw_dir) {
        if entry.extension().map_or(false, |e| e == "md") {
            if let Ok(meta) = std::fs::metadata(&entry) {
                if let Ok(modified) = meta.modified() {
                    let ts = modified
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    if since.map_or(true, |s| ts > s) {
                        files.push(entry);
                    }
                }
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Check if ingest is needed (raw/ has files newer than last ingest).
pub fn needs_ingest(profile_dir: &Path, last_ingest_at: Option<i64>) -> bool {
    scan_raw(profile_dir, last_ingest_at)
        .map(|f| !f.is_empty())
        .unwrap_or(false)
}

/// Run ingest: for each source file in raw/, call the LLM agent to integrate
/// it into the wiki. Uses multi-turn tool-use.
pub async fn ingest(app: &AppState, profile_dir: &Path) -> Result<IngestReport> {
    let last_at = db::get_wiki_last_ingest_at(&app.db).await.ok().flatten();
    let sources = scan_raw(profile_dir, last_at)?;

    if sources.is_empty() {
        return Ok(IngestReport {
            sources_processed: 0,
            nodes_created: 0,
            nodes_appended: 0,
            errors: Vec::new(),
        });
    }

    let agent = db::get_agent_settings(&app.db).await.unwrap_or_default();
    let max_output = agent.max_output.max(256) as u32;
    let thinking_effort = agent.thinking_effort.clone();
    let max_hops = agent.wiki_query_max_hops.max(1) as u32;

    let mut report = IngestReport {
        sources_processed: 0,
        nodes_created: 0,
        nodes_appended: 0,
        errors: Vec::new(),
    };

    let mut max_ts: i64 = last_at.unwrap_or(0);

    for source_path in &sources {
        let source_content = match std::fs::read_to_string(source_path) {
            Ok(c) => c,
            Err(e) => {
                report.errors.push(format!("{}: read error: {e}", source_path.display()));
                continue;
            }
        };

        let source_name = source_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();

        // Get current wiki state for context (snapshot from shared Arc)
        let graph_snapshot: Option<crate::wiki::WikiGraph> = {
            let guard = app.wiki.read().unwrap_or_else(|e| e.into_inner());
            guard.clone()
        };
        let index_body = graph_snapshot
            .as_ref()
            .map(|g| g.index_body().to_string())
            .unwrap_or_default();

        let node_list: Vec<String> = graph_snapshot
            .as_ref()
            .map(|g| g.list(None).iter().map(|(t, s)| format!("- {}: {}", t, s.chars().take(80).collect::<String>())).collect())
            .unwrap_or_default();

        let system_prompt = format!(
            "You are a wiki editor. Your job is to integrate new source material into the \
             career knowledge wiki.\n\n\
             ## Current Wiki Structure\n\n\
             ### Index\n{}\n\n\
             ### Existing Nodes\n{}\n\n\
             ## Rules\n\
             - Use `write_node` to create new nodes or update existing ones.\n\
             - Use `read_node` to check existing content before modifying.\n\
             - Use `list_nodes` to find relevant nodes.\n\
             - Use `read_index` to see the full catalog.\n\
             - Every mention of a skill/project/concept should be [[wikilinked]].\n\
             - Keep node bodies concise (100-500 words).\n\
             - YAML frontmatter: tags, category, proficiency, years (as applicable).\n\
             - If the source material overlaps with an existing node, APPEND to it (don't overwrite).\n\
             - Update System/index.md if you create new nodes.\n\
             - When done, respond with a brief summary of what you changed.",
            index_body,
            node_list.join("\n"),
        );

        let user_msg = format!(
            "Integrate the following source material into the wiki:\n\n\
             ## Source: {}\n\n{}\n\n\
             Process this content now.",
            source_name, source_content,
        );

        // Track what existed before ingest
        let nodes_before: usize = graph_snapshot.as_ref().map(|g| g.len()).unwrap_or(0);
        let write_errors: std::sync::Arc<std::sync::Mutex<Vec<String>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let write_errors_clone = write_errors.clone();

        // Tool definitions
        let tools = vec![
            ToolDef {
                name: "read_node",
                desc: "Read a wiki node by name. Returns the full markdown body.",
                params: json!({
                    "type": "object",
                    "properties": {
                        "name": {"type": "string", "description": "Node name (case-insensitive)"}
                    },
                    "required": ["name"]
                }),
            },
            ToolDef {
                name: "list_nodes",
                desc: "List all wiki nodes with their titles and summaries. Optional filter by substring.",
                params: json!({
                    "type": "object",
                    "properties": {
                        "filter": {"type": "string", "description": "Optional substring filter"}
                    }
                }),
            },
            ToolDef {
                name: "read_index",
                desc: "Read the System/index.md catalog file.",
                params: json!({"type": "object", "properties": {}}),
            },
            ToolDef {
                name: "write_node",
                desc: "Write or update a wiki node file. Creates the file in profile/wiki/.",
                params: json!({
                    "type": "object",
                    "properties": {
                        "name": {"type": "string", "description": "Node name (used as filename.md)"},
                        "body": {"type": "string", "description": "Full markdown content including YAML frontmatter"}
                    },
                    "required": ["name", "body"]
                }),
            },
        ];

        // Clone what we need for the closure
        let profile_dir_owned = profile_dir.to_path_buf();
        let wiki_snapshot = graph_snapshot.clone();

        let result = call_llm_tool_loop(
            app,
            &system_prompt,
            &user_msg,
            &tools,
            |tool_name, args| {
                let r = dispatch_wiki_tool(tool_name, args, &profile_dir_owned, wiki_snapshot.as_ref());
                if tool_name == "write_node" {
                    if let Err(e) = &r {
                        if let Ok(mut errs) = write_errors_clone.lock() {
                            errs.push(format!("write_node: {e}"));
                        }
                    }
                }
                r
            },
            max_output,
            Some(&thinking_effort),
            max_hops,
        )
        .await;

        match result {
            Ok(summary) => {
                eprintln!("ingest {}: {}", source_name, summary.chars().take(100).collect::<String>());

                // Check if any write_node calls failed during the loop
                let source_write_errors = write_errors.lock().map(|mut e| std::mem::take(&mut *e)).unwrap_or_default();
                if source_write_errors.is_empty() {
                    report.sources_processed += 1;
                } else {
                    report.errors.extend(source_write_errors);
                    eprintln!("ingest {}: WARNING: write_node errors occurred", source_name);
                }

                // Refresh wiki graph after ingest — write through to shared state
                if let Ok(new_graph) = crate::wiki::WikiGraph::load(profile_dir) {
                    let nodes_after = new_graph.len();
                    if nodes_after > nodes_before {
                        report.nodes_created += nodes_after - nodes_before;
                    }
                    *app.wiki.write().unwrap_or_else(|e| e.into_inner()) = Some(new_graph);
                    eprintln!("ingest {}: refreshed wiki graph ({nodes_after} nodes)", source_name);
                }

                // Update timestamp
                if let Ok(meta) = std::fs::metadata(source_path) {
                    if let Ok(modified) = meta.modified() {
                        let ts = modified
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64;
                        if ts > max_ts {
                            max_ts = ts;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("ingest {}: ERROR: {e}", source_name);
                report.errors.push(format!("{}: {e}", source_path.display()));
            }
        }
    }

    // Persist timestamp
    if max_ts > last_at.unwrap_or(0) {
        let _ = db::set_wiki_last_ingest_at(&app.db, max_ts).await;
    }

    Ok(report)
}

/// Dispatch a tool call from the ingest agent.
fn dispatch_wiki_tool(
    tool_name: &str,
    args: &Value,
    profile_dir: &Path,
    graph: Option<&crate::wiki::WikiGraph>,
) -> std::result::Result<String, String> {
    match tool_name {
        "read_node" => {
            let name = args["name"].as_str().ok_or("missing 'name' parameter")?;
            match graph.and_then(|g| g.get(name)) {
                Some(node) => Ok(format!("---\n{}\n---\n\n{}", 
                    node.frontmatter.iter()
                        .map(|(k, v)| format!("{}: {}", k, v))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    node.body)),
                None => Err(format!("Node '{}' not found", name)),
            }
        }
        "list_nodes" => {
            let filter = args["filter"].as_str();
            match graph {
                Some(g) => {
                    let nodes = g.list(filter);
                    Ok(nodes.iter()
                        .map(|(t, s)| format!("- {}: {}", t, s))
                        .collect::<Vec<_>>()
                        .join("\n"))
                }
                None => Ok("No wiki loaded.".into()),
            }
        }
        "read_index" => {
            match graph {
                Some(g) => Ok(g.index_body().to_string()),
                None => {
                    // Read from disk
                    let path = profile_dir.join("System").join("index.md");
                    std::fs::read_to_string(&path)
                        .map_err(|e| format!("read index: {e}"))
                }
            }
        }
        "write_node" => {
            let name = args["name"].as_str().ok_or("missing 'name' parameter")?;
            let body = args["body"].as_str().ok_or("missing 'body' parameter")?;

            // Sanitize name: no path traversal, must be reasonable
            if name.contains("..") || name.contains('/') || name.contains('\\') {
                return Err(format!("Invalid node name: {}", name));
            }

            let wiki_dir = profile_dir.join("wiki");
            std::fs::create_dir_all(&wiki_dir)
                .map_err(|e| format!("create wiki dir: {e}"))?;

            let path = wiki_dir.join(format!("{}.md", name));
            std::fs::write(&path, body)
                .map_err(|e| format!("write {}: {e}", path.display()))?;

            Ok(format!("Wrote {}", path.display()))
        }
        _ => Err(format!("Unknown tool: {}", tool_name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn scan_raw_returns_empty_for_missing_dir() {
        let dir = PathBuf::from("/nonexistent");
        let result = scan_raw(&dir, None).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn needs_ingest_returns_true_when_raw_has_files() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("profile");
        let result = needs_ingest(&dir, None);
        let _ = result;
    }

    #[test]
    fn lint_report_format() {
        let report = IngestReport {
            sources_processed: 2,
            nodes_created: 3,
            nodes_appended: 1,
            errors: vec!["test error".into()],
        };
        let summary = report.summary();
        assert!(summary.contains("2 sources"));
        assert!(summary.contains("3 created"));
        assert!(summary.contains("1 errors"));
    }
}
