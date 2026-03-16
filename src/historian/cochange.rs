use std::collections::HashMap;

use anyhow::Result;

use crate::historian::commits::CommitInfo;
use crate::store::graph::GraphStore;

#[derive(Debug, Clone)]
pub struct CoChange {
    pub file_a: String,
    pub file_b: String,
    pub count: u32,
    pub confidence: f64,      // 0.0 - 1.0
    pub last_commit_ts: i64,  // unix timestamp of the most recent commit that included this pair
}

/// Analyze co-change relationships from a list of commits.
/// Returns pairs of files that changed together, sorted by count descending.
/// Confidence is normalized so the most co-changed pair has confidence 1.0.
pub fn analyze_cochanges(commits: &[CommitInfo]) -> Vec<CoChange> {
    // Track (count, last_commit_ts) per pair
    let mut pair_data: HashMap<(String, String), (u32, i64)> = HashMap::new();

    for commit in commits {
        let paths: Vec<&str> = commit
            .files_changed
            .iter()
            .map(|f| f.path.as_str())
            .collect();

        // Generate all unordered pairs with canonical ordering (file_a < file_b)
        for i in 0..paths.len() {
            for j in (i + 1)..paths.len() {
                let (a, b) = if paths[i] < paths[j] {
                    (paths[i].to_string(), paths[j].to_string())
                } else {
                    (paths[j].to_string(), paths[i].to_string())
                };
                let entry = pair_data.entry((a, b)).or_insert((0, commit.timestamp));
                entry.0 += 1;
                if commit.timestamp > entry.1 {
                    entry.1 = commit.timestamp;
                }
            }
        }
    }

    if pair_data.is_empty() {
        return Vec::new();
    }

    let max_count = pair_data.values().map(|(c, _)| *c).max().unwrap_or(1) as f64;

    let mut result: Vec<CoChange> = pair_data
        .into_iter()
        .map(|((file_a, file_b), (count, last_commit_ts))| CoChange {
            file_a,
            file_b,
            confidence: count as f64 / max_count,
            count,
            last_commit_ts,
        })
        .collect();

    result.sort_by(|a, b| b.count.cmp(&a.count));
    result
}

/// Write co-change edges into the graph store.
/// For each CoChange, look up both files by path; if both exist, add a CoChangesWith edge.
pub fn write_cochange_edges(store: &mut GraphStore, cochanges: &[CoChange]) -> Result<()> {
    for cc in cochanges {
        let entity_a = match store.find_entity_by_path(&cc.file_a) {
            Some(e) => e,
            None => continue,
        };
        let entity_b = match store.find_entity_by_path(&cc.file_b) {
            Some(e) => e,
            None => continue,
        };
        store.add_cochange_edge(
            &entity_a.id,
            &entity_b.id,
            cc.confidence,
            cc.last_commit_ts,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::historian::commits::{ChangeKind, CommitInfo, FileChange};

    fn make_commit(hash: &str, files: &[&str]) -> CommitInfo {
        CommitInfo {
            hash: hash.to_string(),
            author: "test".to_string(),
            email: "test@test.com".to_string(),
            timestamp: 0,
            message: "test".to_string(),
            files_changed: files
                .iter()
                .map(|f| FileChange {
                    path: f.to_string(),
                    kind: ChangeKind::Modified,
                })
                .collect(),
        }
    }

    #[test]
    fn test_cochange_frequency() {
        let commits = vec![
            make_commit("a", &["a.rs", "b.rs"]),
            make_commit("b", &["a.rs", "b.rs"]),
            make_commit("c", &["a.rs", "b.rs"]),
            make_commit("d", &["a.rs", "b.rs"]),
            make_commit("e", &["a.rs", "b.rs"]),
            make_commit("f", &["a.rs", "c.rs"]),
        ];

        let cochanges = analyze_cochanges(&commits);

        // a.rs + b.rs should have count 5, a.rs + c.rs should have count 1
        let ab = cochanges.iter().find(|c| {
            (c.file_a == "a.rs" && c.file_b == "b.rs") || (c.file_a == "b.rs" && c.file_b == "a.rs")
        });
        assert!(ab.is_some());
        assert_eq!(ab.unwrap().count, 5);

        let ac = cochanges.iter().find(|c| {
            (c.file_a == "a.rs" && c.file_b == "c.rs") || (c.file_a == "c.rs" && c.file_b == "a.rs")
        });
        assert!(ac.is_some());
        assert_eq!(ac.unwrap().count, 1);
    }

    #[test]
    fn test_cochange_confidence_ordering() {
        let commits = vec![
            make_commit("a", &["x.rs", "y.rs"]),
            make_commit("b", &["x.rs", "y.rs"]),
            make_commit("c", &["x.rs", "z.rs"]),
        ];

        let cochanges = analyze_cochanges(&commits);

        // x+y (count=2) should have higher confidence than x+z (count=1)
        let xy = cochanges
            .iter()
            .find(|c| {
                (c.file_a == "x.rs" && c.file_b == "y.rs")
                    || (c.file_a == "y.rs" && c.file_b == "x.rs")
            })
            .unwrap();
        let xz = cochanges
            .iter()
            .find(|c| {
                (c.file_a == "x.rs" && c.file_b == "z.rs")
                    || (c.file_a == "z.rs" && c.file_b == "x.rs")
            })
            .unwrap();

        assert!(xy.confidence > xz.confidence);
    }

    #[test]
    fn test_empty_commits() {
        let cochanges = analyze_cochanges(&[]);
        assert!(cochanges.is_empty());
    }
}
