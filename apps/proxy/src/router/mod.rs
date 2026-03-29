use anyhow::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteTarget {
    Cloud,
    Local,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RouteDecision {
    pub target: RouteTarget,
    pub reason: &'static str,
    pub matched_keywords: Vec<String>,
    pub matched_example: Option<String>,
    pub score: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpstreamTarget {
    Cloud(String),
    Local(String),
}

#[derive(Clone, Debug)]
pub struct Router {
    cloud_base_url: String,
    local_base_url: Option<String>,
    local_keywords: Vec<String>,
    default_target: RouteTarget,
}

impl Router {
    // Pingora comes first in the Rust port. The embedding/LanceDB path plugs in
    // later behind the same route contract.
    pub async fn new(
        cloud_base_url: impl Into<String>,
        local_base_url: Option<String>,
        local_keywords: Vec<String>,
        default_target: RouteTarget,
    ) -> Result<Self> {
        Ok(Self::with_keyword_fallback(
            cloud_base_url,
            local_base_url,
            local_keywords,
            default_target,
        ))
    }

    pub fn with_keyword_fallback(
        cloud_base_url: impl Into<String>,
        local_base_url: Option<String>,
        local_keywords: Vec<String>,
        default_target: RouteTarget,
    ) -> Self {
        Self {
            cloud_base_url: cloud_base_url.into(),
            local_base_url,
            local_keywords: local_keywords
                .into_iter()
                .map(|keyword| keyword.trim().to_lowercase())
                .filter(|keyword| !keyword.is_empty())
                .collect(),
            default_target,
        }
    }

    pub fn decide(&self, prompt: &str) -> RouteDecision {
        let normalized = prompt.to_lowercase();
        let matched_keywords: Vec<String> = self
            .local_keywords
            .iter()
            .filter(|keyword| normalized.contains(keyword.as_str()))
            .cloned()
            .collect();

        if !matched_keywords.is_empty() {
            return RouteDecision {
                target: RouteTarget::Local,
                reason: "keyword_match",
                matched_keywords,
                matched_example: None,
                score: None,
            };
        }

        RouteDecision {
            target: self.default_target.clone(),
            reason: "default_target",
            matched_keywords: Vec::new(),
            matched_example: None,
            score: None,
        }
    }

    pub async fn route(&self, prompt: &str) -> Result<UpstreamTarget> {
        let decision = self.decide(prompt);
        Ok(match decision.target {
            RouteTarget::Local => UpstreamTarget::Local(
                self.local_base_url.clone().ok_or_else(|| {
                    anyhow::anyhow!("Local route selected but no local upstream is configured")
                })?,
            ),
            RouteTarget::Cloud => UpstreamTarget::Cloud(self.cloud_base_url.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{RouteTarget, Router, UpstreamTarget};

    #[test]
    fn keyword_router_prefers_local_on_match() {
        let router = Router::with_keyword_fallback(
            "https://api.openai.com",
            Some("http://localhost:8001".to_string()),
            vec!["medical".to_string(), "patient".to_string()],
            RouteTarget::Cloud,
        );

        let decision = router.decide("Summarize this patient discharge note");

        assert_eq!(decision.target, RouteTarget::Local);
        assert_eq!(decision.reason, "keyword_match");
        assert_eq!(decision.matched_keywords, vec!["patient".to_string()]);
    }

    #[test]
    fn keyword_router_falls_back_to_default_target() {
        let router = Router::with_keyword_fallback(
            "https://api.openai.com",
            Some("http://localhost:8001".to_string()),
            vec!["medical".to_string()],
            RouteTarget::Cloud,
        );

        let decision = router.decide("What is the capital of Peru?");

        assert_eq!(decision.target, RouteTarget::Cloud);
        assert_eq!(decision.reason, "default_target");
    }

    #[tokio::test]
    async fn route_returns_local_url_when_keyword_matches() {
        let router = Router::with_keyword_fallback(
            "https://api.openai.com",
            Some("http://localhost:8001".to_string()),
            vec!["medical".to_string()],
            RouteTarget::Cloud,
        );

        let upstream = router.route("Please summarize this medical note").await.unwrap();

        assert_eq!(
            upstream,
            UpstreamTarget::Local("http://localhost:8001".to_string())
        );
    }

    #[tokio::test]
    async fn route_errors_when_local_keyword_matches_without_local_url() {
        let router = Router::with_keyword_fallback(
            "https://api.openai.com",
            None,
            vec!["medical".to_string()],
            RouteTarget::Cloud,
        );

        let error = router.route("Please summarize this medical note").await.unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Local route selected but no local upstream is configured")
        );
    }
}
