use anyhow::Result;
use serde_json::json;

/// GitHub API client for posting PR comments and check results.
///
/// This is a lightweight client that constructs the API request payloads.
/// Actual HTTP transport is left to the caller (e.g. `ureq` or `curl`) to
/// keep the dependency footprint minimal.
pub struct GitHubClient {
    pub token: String,
    pub repo_owner: String,
    pub repo_name: String,
    pub api_base: String,
}

impl GitHubClient {
    pub fn new(token: String, repo_owner: String, repo_name: String) -> Self {
        Self {
            token,
            repo_owner,
            repo_name,
            api_base: "https://api.github.com".to_string(),
        }
    }

    /// Build the JSON payload for creating a PR comment via the Issues API.
    pub fn build_comment_payload(&self, pr_number: u64, body: &str) -> CommentRequest {
        CommentRequest {
            url: format!(
                "{}/repos/{}/{}/issues/{}/comments",
                self.api_base, self.repo_owner, self.repo_name, pr_number
            ),
            body: json!({ "body": body }),
            auth_header: format!("Bearer {}", self.token),
        }
    }

    /// Build the JSON payload for creating a check run via the Checks API.
    pub fn build_check_run_payload(
        &self,
        head_sha: &str,
        name: &str,
        summary: &str,
        conclusion: &str,
    ) -> CheckRunRequest {
        CheckRunRequest {
            url: format!(
                "{}/repos/{}/{}/check-runs",
                self.api_base, self.repo_owner, self.repo_name
            ),
            body: json!({
                "name": name,
                "head_sha": head_sha,
                "status": "completed",
                "conclusion": conclusion,
                "output": {
                    "title": name,
                    "summary": summary
                }
            }),
            auth_header: format!("Bearer {}", self.token),
        }
    }
}

/// A prepared comment request (URL + JSON body + auth).
#[derive(Debug, Clone)]
pub struct CommentRequest {
    pub url: String,
    pub body: serde_json::Value,
    pub auth_header: String,
}

/// A prepared check run request.
#[derive(Debug, Clone)]
pub struct CheckRunRequest {
    pub url: String,
    pub body: serde_json::Value,
    pub auth_header: String,
}

/// Map a risk level to a GitHub check run conclusion.
pub fn risk_to_conclusion(risk: &super::RiskLevel) -> &'static str {
    match risk {
        super::RiskLevel::Low => "success",
        super::RiskLevel::Medium => "neutral",
        super::RiskLevel::High => "failure",
        super::RiskLevel::Critical => "failure",
    }
}

/// Validate a GitHub token format (basic sanity check).
pub fn validate_token(token: &str) -> Result<()> {
    if token.is_empty() {
        anyhow::bail!("GitHub token is empty");
    }
    if token.len() < 10 {
        anyhow::bail!("GitHub token is too short");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::github::RiskLevel;

    #[test]
    fn test_build_comment_payload() {
        let client = GitHubClient::new(
            "ghp_test123456".to_string(),
            "emberloom".to_string(),
            "cartograph".to_string(),
        );
        let req = client.build_comment_payload(42, "Test comment");
        assert!(req.url.contains("/issues/42/comments"));
        assert_eq!(req.body["body"], "Test comment");
        assert!(req.auth_header.starts_with("Bearer "));
    }

    #[test]
    fn test_build_check_run_payload() {
        let client = GitHubClient::new(
            "ghp_test123456".to_string(),
            "emberloom".to_string(),
            "cartograph".to_string(),
        );
        let req = client.build_check_run_payload("abc123", "Cartograph", "All good", "success");
        assert!(req.url.contains("/check-runs"));
        assert_eq!(req.body["head_sha"], "abc123");
        assert_eq!(req.body["conclusion"], "success");
    }

    #[test]
    fn test_risk_to_conclusion() {
        assert_eq!(risk_to_conclusion(&RiskLevel::Low), "success");
        assert_eq!(risk_to_conclusion(&RiskLevel::Medium), "neutral");
        assert_eq!(risk_to_conclusion(&RiskLevel::High), "failure");
        assert_eq!(risk_to_conclusion(&RiskLevel::Critical), "failure");
    }

    #[test]
    fn test_validate_token() {
        assert!(validate_token("").is_err());
        assert!(validate_token("short").is_err());
        assert!(validate_token("ghp_verylongtoken123").is_ok());
    }
}
