use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use anyhow::Result;

use crate::store::graph::GraphStore;
use crate::store::schema::{EdgeKind, EntityKind};

#[derive(Debug, Clone)]
pub struct OwnershipEntry {
    pub author: String,
    pub email: String,
    pub line_count: usize,
    pub percentage: f64,
}

/// Run `git blame --porcelain` on `file_path` within `repo_path` and return
/// ownership entries sorted by line count descending.
pub fn who_owns(repo_path: &Path, file_path: &str) -> Result<Vec<OwnershipEntry>> {
    // Validate file_path: reject paths starting with '-' (git option injection)
    if file_path.starts_with('-') {
        anyhow::bail!("invalid file path: {}", file_path);
    }

    let output = Command::new("git")
        .args(["blame", "--porcelain", "--", file_path])
        .current_dir(repo_path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git blame failed for '{}': {}", file_path, stderr);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    parse_blame_output(&text)
}

fn parse_blame_output(text: &str) -> Result<Vec<OwnershipEntry>> {
    // Porcelain format per hunk:
    //   <sha> <orig_line> <final_line> [<num_lines>]
    //   author <name>
    //   author-mail <email>
    //   ... (other headers)
    //   \t<line content>
    //
    // We collect (author, email) for each line. When we see an "author " header
    // we note the author; when we see "author-mail" we note the email; the tab-
    // prefixed line signals one output line attributed to the current hunk author.

    // We'll accumulate per-hunk metadata then count lines.
    let mut line_authors: Vec<(String, String)> = Vec::new();

    let mut current_author = String::new();
    let mut current_email = String::new();

    for line in text.lines() {
        if let Some(name) = line.strip_prefix("author ") {
            current_author = name.to_string();
        } else if let Some(mail) = line.strip_prefix("author-mail ") {
            // Strip surrounding angle brackets if present: <email@host>
            let mail = mail.trim_start_matches('<').trim_end_matches('>');
            current_email = mail.to_string();
        } else if line.starts_with('\t') {
            // This is the actual source line — one per blamed line
            line_authors.push((current_author.clone(), current_email.clone()));
        }
    }

    if line_authors.is_empty() {
        // Could be an empty file — return empty ownership
        return Ok(Vec::new());
    }

    let total = line_authors.len();

    // Aggregate counts per (author, email) pair
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for (author, email) in line_authors {
        *counts.entry((author, email)).or_insert(0) += 1;
    }

    let mut entries: Vec<OwnershipEntry> = counts
        .into_iter()
        .map(|((author, email), line_count)| OwnershipEntry {
            author,
            email,
            percentage: line_count as f64 / total as f64 * 100.0,
            line_count,
        })
        .collect();

    entries.sort_by(|a, b| b.line_count.cmp(&a.line_count));

    Ok(entries)
}

/// For each File entity in the store, run `who_owns` and write OwnedBy edges
/// from the File to a Person entity (creating Person if needed).
pub fn write_ownership_edges(store: &mut GraphStore, repo_path: &Path) -> Result<()> {
    // Collect file entities first to avoid borrow conflicts
    let file_entities: Vec<(String, String)> = store
        .entities()
        .values()
        .filter(|e| e.kind == EntityKind::File)
        .filter_map(|e| e.path.as_ref().map(|p| (e.id.clone(), p.clone())))
        .collect();

    for (file_id, file_path) in file_entities {
        let ownership = match who_owns(repo_path, &file_path) {
            Ok(o) => o,
            Err(_) => continue, // skip files that can't be blamed (e.g. untracked)
        };

        for entry in ownership {
            // Find or create the Person entity; use email as the unique path key
            let person_id = match store.find_entity_by_path(&entry.email) {
                Some(existing) => existing.id,
                None => store.add_entity(
                    EntityKind::Person,
                    &entry.author,
                    Some(&entry.email),
                    None,
                )?,
            };

            let confidence = entry.percentage / 100.0;
            // Ignore duplicate-edge errors (file already has this owner edge)
            let _ = store.add_edge(&file_id, &person_id, EdgeKind::OwnedBy, confidence);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_who_owns_returns_authors() {
        let repo_path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let ownership = who_owns(repo_path, "src/lib.rs").unwrap();

        assert!(!ownership.is_empty(), "Should find at least one author");

        // Percentages should sum to ~100
        let total: f64 = ownership.iter().map(|o| o.percentage).sum();
        assert!(
            (total - 100.0).abs() < 1.0,
            "Percentages should sum to ~100, got {total}"
        );
    }

    #[test]
    fn test_ownership_sorted_by_contribution() {
        let repo_path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let ownership = who_owns(repo_path, "src/lib.rs").unwrap();

        // Should be sorted descending by line count
        for w in ownership.windows(2) {
            assert!(w[0].line_count >= w[1].line_count);
        }
    }

    #[test]
    fn test_who_owns_nonexistent_file() {
        let repo_path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let result = who_owns(repo_path, "nonexistent.rs");
        assert!(result.is_err());
    }
}
