use anyhow::Result;
use std::path::Path;
use super::helpers::chrono_now;

// ---------------------------------------------------------------------------
// Lint — static walk over wiki graph, writes .lint-report.md
// ---------------------------------------------------------------------------

/// Run lint: check for orphans, dangling links, thin nodes.
/// Writes report to `profile/wiki/.lint-report.md`.
pub async fn lint(profile_dir: &Path) -> Result<()> {
    let graph = crate::wiki::WikiGraph::load(profile_dir)?;
    let mut findings = Vec::new();

    // 1. Orphans: no backlinks AND not in System/index.md
    let index_body = graph.index_body().to_string();
    for node in graph.nodes.values() {
        let backlinks = graph.get_backlinks(&node.title);
        let in_index = index_body.contains(&format!("[[{}]]", node.title))
            || index_body.contains(&format!("[[{}|", node.title));
        if backlinks.is_empty() && !in_index {
            findings.push(format!(
                "## Orphan: {}\n\nNo backlinks and not referenced in System/index.md.\n\
                 Add `[[{}]]` to the appropriate section in System/index.md.\n",
                node.title, node.title
            ));
        }
    }

    // 2. Dangling links: [[target]] where target doesn't exist
    for node in graph.nodes.values() {
        for link in &node.links {
            if graph.get(link).is_none() {
                findings.push(format!(
                    "## Dangling link in {}: [[{}]]\n\nTarget node doesn't exist. \
                     Either create the node or remove the link.\n",
                    node.title, link
                ));
            }
        }
    }

    // 3. Thin nodes: body < 100 chars
    for node in graph.nodes.values() {
        if node.body.trim().len() < 100 {
            findings.push(format!(
                "## Thin node: {}\n\nBody is only {} chars. Expand with more detail or merge into another node.\n",
                node.title, node.body.trim().len()
            ));
        }
    }

    let report = if findings.is_empty() {
        "# Wiki Lint Report\n\nNo issues found. The wiki is healthy.\n".to_string()
    } else {
        format!(
            "# Wiki Lint Report\n\nGenerated: {}\nTotal issues: {}\n\n{}\n",
            chrono_now(),
            findings.len(),
            findings.join("\n---\n\n")
        )
    };

    let report_path = profile_dir.join("wiki").join(".lint-report.md");
    std::fs::create_dir_all(report_path.parent().unwrap_or(profile_dir))?;
    std::fs::write(&report_path, report)?;
    Ok(())
}

/// Read the last lint report.
pub fn read_lint_report(profile_dir: &Path) -> Result<String> {
    let path = profile_dir.join("wiki").join(".lint-report.md");
    if !path.exists() {
        return Ok("No lint report yet. Run `POST /wiki/lint` first.".into());
    }
    Ok(std::fs::read_to_string(path)?)
}
