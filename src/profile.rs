//! Profile file reader — reads `profile/index.md` and syncs the full content
//! (including YAML frontmatter) to SQLite so the scraper can read it.
//! # ponytail: frontmatter stays — LLMs understand it, and the JS preview uses
//! # it for the CV header. No reason to strip it.

use anyhow::Result;
use sqlx::SqlitePool;

/// Read the full profile file content (including YAML frontmatter).
/// Uses PROFILE_DIR env or defaults to `./profile`.
pub fn read_profile() -> Result<String> {
    let dir = std::env::var("PROFILE_DIR").unwrap_or_else(|_| "./profile".into());
    Ok(std::fs::read_to_string(format!("{dir}/index.md"))?)
}

/// Extract `name` and `title` from YAML frontmatter (between `---` delimiters).
/// Returns empty strings if frontmatter is absent or fields are missing.
/// Does not use a YAML crate — simple line-by-line parsing matching the existing pattern.
pub fn extract_name_title(content: &str) -> (String, String) {
    let fm = match content.strip_prefix("---") {
        Some(rest) => match rest.find("---") {
            Some(end) => &rest[..end],
            None => return (String::new(), String::new()),
        },
        None => return (String::new(), String::new()),
    };

    let mut name = String::new();
    let mut title = String::new();

    for line in fm.lines() {
        let trimmed = line.trim();
        if let Some((key, value)) = trimmed.split_once(':') {
            let val = value.trim().trim_matches('"').trim_matches('\'').to_string();
            match key.trim() {
                "name" => name = val,
                "title" => title = val,
                _ => {}
            }
        }
    }

    (name, title)
}

/// Sync the full profile (frontmatter + body) to the settings table.
pub async fn sync_profile_to_db(pool: &SqlitePool) -> Result<()> {
    let content = read_profile()?;
    sqlx::query(
        "INSERT INTO settings (id, master_cv) VALUES (1, ?)
         ON CONFLICT(id) DO UPDATE SET master_cv = excluded.master_cv",
    )
    .bind(&content)
    .execute(pool)
    .await?;
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
}
