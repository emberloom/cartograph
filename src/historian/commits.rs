use anyhow::Result;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub hash: String,
    pub author: String,
    pub email: String,
    pub timestamp: i64,
    pub message: String,
    pub files_changed: Vec<FileChange>,
}

/// Mine git commits from the repository at `repo_path`.
/// Returns commits in reverse chronological order (newest first).
/// If `limit` is provided, returns at most that many commits.
pub fn mine_commits(repo_path: &Path, limit: Option<usize>) -> Result<Vec<CommitInfo>> {
    use std::process::Command;

    // Use git log --name-status to get commit metadata and file changes.
    // Format: hash NUL author NUL email NUL timestamp NUL message NUL
    // followed by --name-status output, then a separator.
    //
    // We use a two-pass approach:
    // 1. Get commit list with metadata via --format
    // 2. Get file changes via --name-status in the same run using a record separator

    // Cap commit limit to prevent memory exhaustion on large repos
    const MAX_COMMITS: usize = 10_000;
    let effective_limit = limit.map(|n| n.min(MAX_COMMITS)).unwrap_or(MAX_COMMITS);

    let output = Command::new("git")
        .args([
            "log",
            "--name-status",
            "--no-merges",
            "--format=COMMIT_START%n%H%n%an%n%ae%n%at%n%s",
            &format!("-{}", effective_limit),
            "--",
        ])
        .current_dir(repo_path)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git log failed: {}", stderr);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    parse_git_log_output(&text)
}

fn parse_git_log_output(text: &str) -> Result<Vec<CommitInfo>> {
    let mut commits = Vec::new();
    let mut current_commit: Option<CommitInfo> = None;
    // State machine: header lines come first, then file-change lines
    // After COMMIT_START we expect: hash, author, email, timestamp, message (5 lines)
    let mut header_lines_remaining = 0usize;
    let mut header_buf: Vec<String> = Vec::new();

    for line in text.lines() {
        if line == "COMMIT_START" {
            // Save any previous commit
            if let Some(c) = current_commit.take() {
                commits.push(c);
            }
            header_lines_remaining = 5;
            header_buf.clear();
            continue;
        }

        if header_lines_remaining > 0 {
            header_buf.push(line.to_string());
            header_lines_remaining -= 1;
            if header_lines_remaining == 0 {
                // Build commit from buffer: [hash, author, email, timestamp, message]
                let hash = header_buf[0].clone();
                let author = header_buf[1].clone();
                let email = header_buf[2].clone();
                let timestamp: i64 = header_buf[3].parse().unwrap_or(0);
                let message = header_buf[4].clone();
                current_commit = Some(CommitInfo {
                    hash,
                    author,
                    email,
                    timestamp,
                    message,
                    files_changed: Vec::new(),
                });
            }
            continue;
        }

        // File change lines: tab-separated status + path(s)
        // M\tpath, A\tpath, D\tpath, R100\told\tnew, C100\told\tnew
        if let Some(ref mut commit) = current_commit {
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.splitn(3, '\t').collect();
            if parts.is_empty() {
                continue;
            }
            let status = parts[0];
            let kind = if status.starts_with('A') {
                ChangeKind::Added
            } else if status.starts_with('D') {
                ChangeKind::Deleted
            } else if status.starts_with('R') || status.starts_with('C') {
                // Rename/copy: treat the new path as Modified
                ChangeKind::Modified
            } else {
                ChangeKind::Modified
            };

            // For rename/copy, parts[2] is the new path; otherwise parts[1]
            let path = if (status.starts_with('R') || status.starts_with('C')) && parts.len() >= 3 {
                parts[2].to_string()
            } else if parts.len() >= 2 {
                parts[1].to_string()
            } else {
                continue;
            };

            commit.files_changed.push(FileChange { path, kind });
        }
    }

    // Don't forget the last commit
    if let Some(c) = current_commit.take() {
        commits.push(c);
    }

    Ok(commits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_mine_commits_returns_data() {
        let repo_path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let commits = mine_commits(repo_path, None).unwrap();

        assert!(!commits.is_empty(), "Should find at least one commit");

        let first = &commits[0];
        assert!(!first.hash.is_empty());
        assert!(!first.author.is_empty());
        assert!(
            !first.files_changed.is_empty(),
            "Commits should list changed files"
        );
    }

    #[test]
    fn test_mine_commits_with_limit() {
        let repo_path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let commits = mine_commits(repo_path, Some(2)).unwrap();
        assert!(commits.len() <= 2);
    }

    #[test]
    fn test_commit_info_has_file_changes() {
        let repo_path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let commits = mine_commits(repo_path, Some(5)).unwrap();

        // At least some commits should have file changes
        let has_changes = commits.iter().any(|c| !c.files_changed.is_empty());
        assert!(has_changes, "Some commits should have file changes");
    }

    #[test]
    fn test_commit_fields_populated() {
        let repo_path = Path::new(env!("CARGO_MANIFEST_DIR"));
        let commits = mine_commits(repo_path, Some(3)).unwrap();

        for commit in &commits {
            assert!(!commit.hash.is_empty(), "hash should not be empty");
            assert!(!commit.author.is_empty(), "author should not be empty");
            assert!(!commit.email.is_empty(), "email should not be empty");
            assert!(commit.timestamp > 0, "timestamp should be positive");
        }
    }

    #[test]
    fn test_parse_git_log_output() {
        let sample = "\
COMMIT_START
abc123def456abc123def456abc123def456abc1
Alice
alice@example.com
1700000000
Initial commit
A\tsrc/main.rs
A\tCargo.toml
COMMIT_START
def456abc123def456abc123def456abc123def4
Bob
bob@example.com
1699999000
Second commit
M\tsrc/main.rs
D\told_file.txt
";
        let commits = parse_git_log_output(sample).unwrap();
        assert_eq!(commits.len(), 2);

        assert_eq!(commits[0].hash, "abc123def456abc123def456abc123def456abc1");
        assert_eq!(commits[0].author, "Alice");
        assert_eq!(commits[0].email, "alice@example.com");
        assert_eq!(commits[0].timestamp, 1700000000);
        assert_eq!(commits[0].message, "Initial commit");
        assert_eq!(commits[0].files_changed.len(), 2);
        assert_eq!(commits[0].files_changed[0].path, "src/main.rs");
        assert_eq!(commits[0].files_changed[0].kind, ChangeKind::Added);
        assert_eq!(commits[0].files_changed[1].path, "Cargo.toml");
        assert_eq!(commits[0].files_changed[1].kind, ChangeKind::Added);

        assert_eq!(commits[1].author, "Bob");
        assert_eq!(commits[1].files_changed.len(), 2);
        assert_eq!(commits[1].files_changed[0].kind, ChangeKind::Modified);
        assert_eq!(commits[1].files_changed[1].kind, ChangeKind::Deleted);
    }
}
