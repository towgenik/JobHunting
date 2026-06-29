use axum::response::{IntoResponse, Response};
use crate::profile;

// GET /profile/print?file=... — A4 print page for a profile file
pub async fn profile_print(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    let file = params.get("file").map(|s| s.as_str()).unwrap_or("index.md");
    if file.contains("..") {
        return (axum::http::StatusCode::BAD_REQUEST, "Invalid file path").into_response();
    }
    let content = profile::read_profile_file(file).unwrap_or_default();
    let (name, title) = profile::extract_name_title(&content);
    let mut body = if let Some(rest) = content.strip_prefix("---") {
        if let Some(idx) = rest.find("---") {
            rest[idx+3..].trim().to_string()
        } else { content.clone() }
    } else { content.clone() };

    // Follow wikilinks: collect linked .md files from the profile dir, append their content
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    seen.insert(file.to_string());
    let files = profile::list_profile_files().unwrap_or_default();
    for f in &files {
        if seen.contains(&f.path) { continue; }
        if !body.contains(&format!("[[{}]]", f.path.trim_end_matches(".md")))
           && !body.contains(&format!("[[{}|", f.path.trim_end_matches(".md")))
        { continue; }
        seen.insert(f.path.clone());
        if let Ok(linked) = profile::read_profile_file(&f.path) {
            let linked_body = if let Some(rest) = linked.strip_prefix("---") {
                if let Some(idx) = rest.find("---") { rest[idx+3..].trim().to_string() }
                else { linked }
            } else { linked };
            body.push_str("\n\n");
            body.push_str(&linked_body);
        }
    }

    let header_html = if !name.is_empty() {
        format!("<header style=\"text-align:center;margin-bottom:1.5em;padding-bottom:1em;border-bottom:1.5px solid #333\"><div style=\"font-size:18pt;font-weight:700;letter-spacing:2pt;text-transform:uppercase;margin-bottom:.15em\">{}</div><div style=\"font-size:11pt;color:#555;font-style:italic\">{}</div></header>", name.replace("&", "&amp;").replace("<", "&lt;"), title.replace("&", "&amp;").replace("<", "&lt;"))
    } else { String::new() };

    let body_escaped = body.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;");
    let html = format!(r#"<!DOCTYPE html><html><head><meta charset="UTF-8"><title>{name}</title>
<style>@page{{size:A4;margin:1.5cm}}*{{margin:0;padding:0;box-sizing:border-box}}
body{{font-family:Georgia,serif;font-size:11pt;line-height:1.45;color:#222;max-width:794px;margin:0 auto;padding:1.5cm}}
h1{{font-size:14pt;margin:1.2em 0 .4em;border-bottom:1px solid #999;padding-bottom:.15em;text-transform:uppercase;letter-spacing:1pt}}
h2{{font-size:12pt;margin:1em 0 .3em;border-bottom:1px solid #ccc}}
ul{{margin:0 0 .6em 1.2em;list-style:square}}li{{margin-bottom:.15em}}
p{{margin:0 0 .5em}}strong{{color:#111}}
.no-print{{text-align:center;margin-bottom:1cm}}@media print{{.no-print{{display:none}}}}
</style></head><body><div class="no-print"><button onclick="window.print()">Print / Save as PDF</button></div>
{header_html}
<div id="content">{body_escaped}</div>
<script src="https://cdn.jsdelivr.net/npm/marked/marked.min.js"></script>
<script>
let md=document.getElementById('content').textContent;
md=md.replace(/<!--[\s\S]*?-->/g,"");
md=md.replace(/\[\[[^\]]+\|([^\]]+)\]\]/g,"$1");
md=md.replace(/\[\[([^\]]+)\]\]/g,"$1");
document.getElementById('content').innerHTML=marked.parse(md);
</script></body></html>"#, name = name.replace("&", "&amp;"), body_escaped = body_escaped, header_html = header_html);
    (axum::http::StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html).into_response()
}

