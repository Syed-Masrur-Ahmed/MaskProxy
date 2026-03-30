mod embedding;

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::state::lancedb::RouteMatch;

pub use embedding::{load_route_examples, OnnxTextEmbeddingProvider};

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

pub trait EmbeddingProvider: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
}

#[async_trait]
pub trait SemanticRouteStore: Send + Sync {
    async fn query(&self, embedding: &[f32], limit: usize) -> Result<Vec<RouteMatch>>;
}

impl<T> EmbeddingProvider for Arc<T>
where
    T: EmbeddingProvider + ?Sized,
{
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        (**self).embed(text)
    }
}

#[async_trait]
impl<T> SemanticRouteStore for Arc<T>
where
    T: SemanticRouteStore + ?Sized,
{
    async fn query(&self, embedding: &[f32], limit: usize) -> Result<Vec<RouteMatch>> {
        (**self).query(embedding, limit).await
    }
}

pub struct SemanticRouter<P, S> {
    embedding_provider: P,
    route_store: S,
    similarity_threshold: f32,
    default_target: RouteTarget,
    top_k: usize,
}

impl<P, S> SemanticRouter<P, S>
where
    P: EmbeddingProvider,
    S: SemanticRouteStore,
{
    pub fn new(
        embedding_provider: P,
        route_store: S,
        similarity_threshold: f32,
        default_target: RouteTarget,
        top_k: usize,
    ) -> Self {
        Self {
            embedding_provider,
            route_store,
            similarity_threshold,
            default_target,
            top_k,
        }
    }

    // Text passed to semantic routing is raw and unmasked. All routing backends
    // must run locally and must not send prompt text to external services.
    pub async fn decide(&self, text: &str) -> Result<RouteDecision> {
        if text.trim().is_empty() {
            return Ok(RouteDecision {
                target: self.default_target.clone(),
                reason: "semantic_default_target",
                matched_keywords: Vec::new(),
                matched_example: None,
                score: None,
            });
        }

        let embedding = self.embedding_provider.embed(text)?;
        if embedding.is_empty() {
            return Ok(RouteDecision {
                target: self.default_target.clone(),
                reason: "semantic_default_target",
                matched_keywords: Vec::new(),
                matched_example: None,
                score: None,
            });
        }

        let matches = self.route_store.query(&embedding, self.top_k).await?;
        let Some(best_match) = matches.first() else {
            return Ok(RouteDecision {
                target: self.default_target.clone(),
                reason: "semantic_default_target",
                matched_keywords: Vec::new(),
                matched_example: None,
                score: None,
            });
        };

        if best_match.score < self.similarity_threshold {
            return Ok(RouteDecision {
                target: self.default_target.clone(),
                reason: "semantic_default_target",
                matched_keywords: Vec::new(),
                matched_example: Some(best_match.text.clone()),
                score: Some(best_match.score),
            });
        }

        Ok(RouteDecision {
            target: best_match.target.clone(),
            reason: "semantic_match",
            matched_keywords: Vec::new(),
            matched_example: Some(best_match.text.clone()),
            score: Some(best_match.score),
        })
    }
}

pub type SharedEmbeddingProvider = Arc<dyn EmbeddingProvider>;
pub type SharedSemanticRouteStore = Arc<dyn SemanticRouteStore>;
type RuntimeSemanticRouter = SemanticRouter<SharedEmbeddingProvider, SharedSemanticRouteStore>;

#[derive(Clone)]
enum RoutingMode {
    Keyword {
        local_keywords: Vec<String>,
        default_target: RouteTarget,
    },
    Semantic(Arc<RuntimeSemanticRouter>),
}

#[derive(Clone)]
pub struct Router {
    cloud_base_url: String,
    local_base_url: Option<String>,
    mode: RoutingMode,
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
            mode: RoutingMode::Keyword {
                local_keywords: local_keywords
                    .into_iter()
                    .map(|keyword| keyword.trim().to_lowercase())
                    .filter(|keyword| !keyword.is_empty())
                    .collect(),
                default_target,
            },
        }
    }

    pub fn with_semantic(
        cloud_base_url: impl Into<String>,
        local_base_url: Option<String>,
        embedding_provider: Arc<dyn EmbeddingProvider>,
        route_store: Arc<dyn SemanticRouteStore>,
        similarity_threshold: f32,
        default_target: RouteTarget,
        top_k: usize,
    ) -> Self {
        Self {
            cloud_base_url: cloud_base_url.into(),
            local_base_url,
            mode: RoutingMode::Semantic(Arc::new(SemanticRouter::new(
                embedding_provider,
                route_store,
                similarity_threshold,
                default_target,
                top_k,
            ))),
        }
    }

    pub fn decide(&self, prompt: &str) -> RouteDecision {
        match &self.mode {
            RoutingMode::Keyword {
                local_keywords,
                default_target,
            } => {
                let normalized = prompt.to_lowercase();
                let matched_keywords: Vec<String> = local_keywords
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
                    target: default_target.clone(),
                    reason: "default_target",
                    matched_keywords: Vec::new(),
                    matched_example: None,
                    score: None,
                }
            }
            RoutingMode::Semantic(_) => RouteDecision {
                target: RouteTarget::Cloud,
                reason: "router_requires_async_decision",
                matched_keywords: Vec::new(),
                matched_example: None,
                score: None,
            },
        }
    }

    async fn decide_async(&self, prompt: &str) -> Result<RouteDecision> {
        match &self.mode {
            RoutingMode::Keyword { .. } => Ok(self.decide(prompt)),
            RoutingMode::Semantic(router) => router.decide(prompt).await,
        }
    }

    pub async fn route(&self, prompt: &str) -> Result<UpstreamTarget> {
        let decision = self.decide_async(prompt).await?;
        Ok(match decision.target {
            RouteTarget::Local => {
                UpstreamTarget::Local(self.local_base_url.clone().ok_or_else(|| {
                    anyhow::anyhow!("Local route selected but no local upstream is configured")
                })?)
            }
            RouteTarget::Cloud => UpstreamTarget::Cloud(self.cloud_base_url.clone()),
        })
    }
}

#[cfg(test)]
mod tests;
