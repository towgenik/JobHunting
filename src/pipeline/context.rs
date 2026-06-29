use serde_json::Value;
use crate::AppState;

pub fn build_wiki_context(app: &AppState, _jd: &str, master_cv: &str) -> String {
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
