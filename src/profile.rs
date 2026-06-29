//! Profile file reader — reads `profile/index.md` and syncs the full content
//! (including YAML frontmatter) to SQLite so the scraper can read it.
//! # ponytail: frontmatter stays — LLMs understand it, and the JS preview uses
//! # it for the CV header. No reason to strip it.

use anyhow::Result;
use sqlx::SqlitePool;
use std::path::PathBuf;

/// Read the full profile file content (including YAML frontmatter).
/// Uses PROFILE_DIR env or defaults to `./profile`.
#[allow(dead_code)] // used in tests
pub fn read_profile() -> Result<String> {
    let dir = std::env::var("PROFILE_DIR").unwrap_or_else(|_| "./profile".into());
    Ok(std::fs::read_to_string(format!("{dir}/index.md"))?)
}

/// Extract `name` and `title` from YAML frontmatter (between `---` delimiters).
/// Returns empty strings if frontmatter is absent or fields are missing.
/// Does not use a YAML crate — simple line-by-line parsing matching the existing pattern.
pub fn extract_name_title(content: &str) -> (String, String) {
    let map = parse_frontmatter(content);
    (map.get("name").cloned().unwrap_or_default(),
     map.get("title").cloned().unwrap_or_default())
}

/// Parse YAML frontmatter between `---` delimiters into a flat key→value map.
/// Handles inline arrays (`key: [a, b]`) as the raw string, and multi-line
/// lists (`key:\n  - item`) as newline-separated values.
/// Does not use a YAML crate — simple line-by-line parsing.
pub fn parse_frontmatter(content: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let fm = match content.strip_prefix("---") {
        Some(rest) => match rest.find("---") {
            Some(end) => &rest[..end],
            None => return map,
        },
        None => return map,
    };

    let mut current_key = String::new();
    let mut current_val = String::new();
    let mut in_list = false;

    for line in fm.lines() {
        let trimmed = line.trim();
        if let Some((key, value)) = trimmed.split_once(':') {
            // Flush previous key
            if !current_key.is_empty() {
                map.insert(current_key.clone(), current_val.trim().to_string());
            }
            current_key = key.trim().to_string();
            current_val = value.trim().trim_matches('"').trim_matches('\'').to_string();
            in_list = false;

            // Inline list: key: [item1, item2] — store as-is
            if current_val == "[" || current_val.is_empty() {
                in_list = true;
                current_val.clear();
            }
        } else if in_list && trimmed.starts_with('-') {
            let item = trimmed[1..].trim().trim_matches('"').trim_matches('\'');
            if !current_val.is_empty() {
                current_val.push_str(", ");
            }
            current_val.push_str(item);
        }
    }
    // Flush last key
    if !current_key.is_empty() {
        map.insert(current_key, current_val.trim().to_string());
    }
    map
}

/// Sync the full profile (frontmatter + body) to the settings table.
/// Reads index.md + portfolio.md, expands wikilinks, strips image embeds.
/// ponytail: wiki vault scaffolding — concatenate all profile .md files
/// into one blob so the LLM gets the full picture. Future: per-section
/// retrieval so agents don't eat the whole context window.
pub async fn sync_profile_to_db(pool: &SqlitePool) -> Result<()> {
    let dir = profile_dir();
    let mut body = String::new();

    // 1. Read index.md (master CV source)
    let index_path = dir.join("index.md");
    if index_path.exists() {
        body = std::fs::read_to_string(&index_path)?;
    }

    // 2. Append portfolio.md body (skip frontmatter if present)
    let portfolio_path = dir.join("portfolio.md");
    if portfolio_path.exists() {
        let pf = std::fs::read_to_string(&portfolio_path)?;
        let pf_body = strip_frontmatter(&pf);
        body.push_str("\n\n# Portfolio\n\n");
        body.push_str(pf_body);
    }

    // 3. Expand wikilinks: [[target|label]] → label, [[target]] → target
    //    Also strip image embeds: ![[...]] → (removed)
    let expanded = expand_wikilinks(&body);

    sqlx::query(
        "INSERT INTO settings (id, master_cv) VALUES (1, ?)
         ON CONFLICT(id) DO UPDATE SET master_cv = excluded.master_cv",
    )
    .bind(&expanded)
    .execute(pool)
    .await?;
    Ok(())
}

/// Strip YAML frontmatter from content. Returns body after the closing `---`.
pub fn strip_frontmatter(content: &str) -> &str {
    if let Some(rest) = content.strip_prefix("---") {
        if let Some(idx) = rest.find("---") {
            return rest[idx + 3..].trim();
        }
    }
    content
}

/// Expand wikilinks and strip image embeds.
/// `[[target|label]]` → `label`, `[[target]]` → `target`, `![[...]]` → removed.
fn expand_wikilinks(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '!' && chars.peek() == Some(&'[') {
            // Image embed: ![[...]] — skip entirely
            chars.next(); // skip [
            if chars.peek() == Some(&'[') {
                chars.next(); // skip second [
                while let Some(ch) = chars.next() {
                    if ch == ']' {
                        if chars.peek() == Some(&']') {
                            chars.next();
                            break;
                        }
                    }
                }
            } else {
                result.push('!');
                result.push('[');
            }
        } else if c == '[' && chars.peek() == Some(&'[') {
            // Wikilink: [[target|label]] or [[target]]
            chars.next(); // skip second [
            let mut link = String::new();
            while let Some(ch) = chars.next() {
                if ch == ']' {
                    if chars.peek() == Some(&']') {
                        chars.next();
                        break;
                    }
                }
                link.push(ch);
            }
            // Use label if present, otherwise target
            let display = link.split_once('|').map(|(_, l)| l).unwrap_or(&link);
            result.push_str(display);
        } else {
            result.push(c);
        }
    }
    result
}

/// A profile file entry for the sidebar listing.
pub struct ProfileFile {
    pub path: String,
    pub name: String,
}

pub fn profile_dir() -> PathBuf {
    PathBuf::from(std::env::var("PROFILE_DIR").unwrap_or_else(|_| "./profile".into()))
}

fn walk_md_files(dir: &PathBuf, base: &PathBuf, files: &mut Vec<ProfileFile>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_md_files(&path, base, files)?;
        } else if path.extension().map_or(false, |e| e == "md") {
            let rel = path.strip_prefix(base).unwrap_or(&path);
            files.push(ProfileFile {
                path: rel.to_string_lossy().to_string(),
                name: path.file_stem().unwrap_or_default().to_string_lossy().to_string(),
            });
        }
    }
    Ok(())
}

/// List all .md files recursively under profile/ dir.
pub fn list_profile_files() -> Result<Vec<ProfileFile>> {
    let base = profile_dir();
    let mut files = Vec::new();
    walk_md_files(&base, &base, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// Read a specific profile file (path relative to profile/ dir).
pub fn read_profile_file(path: &str) -> Result<String> {
    anyhow::ensure!(!path.contains(".."), "invalid path");
    Ok(std::fs::read_to_string(profile_dir().join(path))?)
}

/// Write content to a specific profile file.
pub fn write_profile_file(path: &str, content: &str) -> Result<()> {
    anyhow::ensure!(!path.contains(".."), "invalid path");
    std::fs::write(profile_dir().join(path), content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_profile_returns_full_content() {
        // Uses the actual profile/index.md in the project tree.
        // Verifies it contains both frontmatter delimiters and body content.
        let content = read_profile().expect("profile/index.md should be readable");
        assert!(content.contains("---"), "should contain frontmatter delimiters");
        assert!(content.contains("name:"), "should contain frontmatter fields");
        // Should have body content (at minimum the Summary heading)
        assert!(content.contains("# Summary"), "should contain CV body");
    }

    #[test]
    fn read_profile_produces_non_empty_string() {
        let content = read_profile().unwrap();
        assert!(!content.is_empty());
        // The file should have at least the frontmatter block
        assert!(content.len() > 50, "profile should be >50 chars, got {}", content.len());
    }

    #[test]
    fn extract_name_title_happy_path() {
        let content = "---\nname: \"Farrel Ilham Shaputra\"\ntitle: \"Software Engineer — Backend & Infrastructure\"\n---\n\nBody here";
        let (name, title) = extract_name_title(content);
        assert_eq!(name, "Farrel Ilham Shaputra");
        assert_eq!(title, "Software Engineer — Backend & Infrastructure");
    }

    #[test]
    fn extract_name_title_no_frontmatter() {
        let content = "# Just body\nno frontmatter here";
        let (name, title) = extract_name_title(content);
        assert_eq!(name, "");
        assert_eq!(title, "");
    }

    #[test]
    fn extract_name_title_empty_frontmatter() {
        let content = "---\n---\nbody";
        let (name, title) = extract_name_title(content);
        assert_eq!(name, "");
        assert_eq!(title, "");
    }

    #[test]
    fn list_profile_files_includes_index_md() {
        let files = list_profile_files().unwrap();
        assert!(files.iter().any(|f| f.path == "index.md"), "should include profile/index.md");
    }

    #[test]
    fn read_profile_file_reads_index_md() {
        let content = read_profile_file("index.md").unwrap();
        assert!(content.contains("# Summary"), "should contain CV body");
    }

    #[test]
    fn strip_frontmatter_removes_delimiters() {
        let content = "---\nname: test\n---\nBody here";
        assert_eq!(strip_frontmatter(content), "Body here");
    }

    #[test]
    fn strip_frontmatter_no_frontmatter() {
        let content = "Just body";
        assert_eq!(strip_frontmatter(content), "Just body");
    }

    #[test]
    fn expand_wikilinks_label() {
        assert_eq!(expand_wikilinks("[[target|display text]]"), "display text");
    }

    #[test]
    fn expand_wikilinks_no_label() {
        assert_eq!(expand_wikilinks("[[target]]"), "target");
    }

    #[test]
    fn expand_wikilinks_strips_images() {
        assert_eq!(expand_wikilinks("![[portfolio/api-gateway.svg]]"), "");
    }

    #[test]
    fn expand_wikilinks_mixed() {
        let input = "See [[portfolio#jobhunting|JobHunting]] and ![[img.svg]] here.";
        assert_eq!(expand_wikilinks(input), "See JobHunting and  here.");
    }
}
