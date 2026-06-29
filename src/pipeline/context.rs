use serde_json::Value;
use crate::AppState;

pub fn build_wiki_context(app: &AppState, _jd: &str, master_cv: &str, ctx_window: u32) -> String {
    let wiki = app.wiki.read().unwrap_or_else(|e| e.into_inner());
    let Some(graph) = wiki.as_ref() else {
        return master_cv.to_string();
    };
    if graph.is_empty() {
        return master_cv.to_string();
    }

    let mut ctx = String::new();
    ctx.push_str("# Career Knowledge Base — Index\n\n");
    ctx.push_str(graph.index_body());
    ctx.push_str("\n\n# Relevant Wiki Nodes\n\n");

    // Include all node bodies — the model picks what's relevant to the JD.
    for (title, _) in graph.list(None) {
        if let Some(node) = graph.get(title) {
            ctx.push_str(&format!("## {}\n\n{}\n\n", node.title, node.body));
        }
    }

    // Truncate to ctx_window budget (~4 chars per token). Keep the index header.
    let max_chars = (ctx_window as usize) * 4;
    if ctx.len() > max_chars {
        if let Some(pos) = ctx.find("# Relevant Wiki Nodes") {
            let header = &ctx[..pos];
            let body_max = max_chars.saturating_sub(header.len());
            if body_max < ctx.len() - pos {
                let mut truncated = String::with_capacity(max_chars);
                truncated.push_str(header);
                truncated.push_str(&ctx[pos..pos + body_max]);
                truncated.push_str("\n\n[context truncated to fit ctx_window budget]");
                ctx = truncated;
            }
        }
    }

    ctx
}

pub fn build_prompt(task: &str, context: &Value) -> String {
    let ctx = serde_json::to_string_pretty(context).unwrap_or_default();
    format!(
        r###"
### CONTEXT
{ctx}
### TASK
{task}
"###
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn build_prompt_contains_context_and_task() {
        let context = json!({"job_description": "Engineer role", "master_cv": "CV text"});
        let prompt  = build_prompt("Do the thing", &context);
        assert!(prompt.contains("### CONTEXT"), "missing CONTEXT section");
        assert!(prompt.contains("### TASK"),    "missing TASK section");
        assert!(prompt.contains("Do the thing"), "task text not in prompt");
        assert!(prompt.contains("job_description"), "context not serialized into prompt");
    }
}


// ---------------------------------------------------------------------------
// Helpers
