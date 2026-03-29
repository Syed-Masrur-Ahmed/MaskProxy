use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

use super::{
    EmbeddingProvider, RouteTarget, Router, SemanticRouteStore, SemanticRouter, UpstreamTarget,
};
use crate::state::lancedb::{LanceDbState, RouteExampleRow, RouteMatch};

#[derive(Clone)]
struct FakeEmbeddingProvider {
    embedding: Vec<f32>,
}

impl EmbeddingProvider for FakeEmbeddingProvider {
    fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(self.embedding.clone())
    }
}

#[derive(Clone)]
struct FakeRouteStore {
    matches: Vec<RouteMatch>,
}

#[async_trait]
impl SemanticRouteStore for FakeRouteStore {
    async fn query(&self, _embedding: &[f32], limit: usize) -> Result<Vec<RouteMatch>> {
        Ok(self.matches.iter().take(limit).cloned().collect())
    }
}

fn temp_lancedb_path() -> PathBuf {
    let path = env::temp_dir().join(format!("maskproxy-router-lancedb-{}", Uuid::new_v4()));
    fs::create_dir_all(&path).unwrap();
    path
}

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

    let upstream = router
        .route("Please summarize this medical note")
        .await
        .unwrap();

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

    let error = match router.route("Please summarize this medical note").await {
        Ok(_) => panic!("expected missing local upstream to return an error"),
        Err(error) => error,
    };

    assert!(error
        .to_string()
        .contains("Local route selected but no local upstream is configured"));
}

#[tokio::test]
async fn semantic_router_selects_best_match() {
    let router = SemanticRouter::new(
        FakeEmbeddingProvider {
            embedding: vec![1.0, 0.0],
        },
        FakeRouteStore {
            matches: vec![RouteMatch {
                text: "Patient discharge summary".to_string(),
                target: RouteTarget::Local,
                score: 0.92,
            }],
        },
        0.8,
        RouteTarget::Cloud,
        3,
    );

    let decision = router
        .decide("Classify this medical document")
        .await
        .unwrap();

    assert_eq!(decision.target, RouteTarget::Local);
    assert_eq!(decision.reason, "semantic_match");
    assert_eq!(
        decision.matched_example.as_deref(),
        Some("Patient discharge summary")
    );
    assert_eq!(decision.score, Some(0.92));
}

#[tokio::test]
async fn semantic_router_falls_back_below_threshold() {
    let router = SemanticRouter::new(
        FakeEmbeddingProvider {
            embedding: vec![0.6, 0.6],
        },
        FakeRouteStore {
            matches: vec![RouteMatch {
                text: "Financial risk assessment".to_string(),
                target: RouteTarget::Local,
                score: 0.72,
            }],
        },
        0.95,
        RouteTarget::Cloud,
        3,
    );

    let decision = router.decide("Ambiguous prompt").await.unwrap();

    assert_eq!(decision.target, RouteTarget::Cloud);
    assert_eq!(decision.reason, "semantic_default_target");
    assert_eq!(
        decision.matched_example.as_deref(),
        Some("Financial risk assessment")
    );
    assert_eq!(decision.score, Some(0.72));
}

#[tokio::test]
async fn semantic_router_uses_lancedb_similarity_scores() {
    let path = temp_lancedb_path();
    let mut store = LanceDbState::new(path.to_str().unwrap(), "route_examples", 2)
        .await
        .unwrap();
    store
        .rebuild_from_examples(&[
            RouteExampleRow {
                text: "Patient discharge summary".to_string(),
                target: RouteTarget::Local,
                vector: vec![1.0, 0.0],
            },
            RouteExampleRow {
                text: "General trivia question".to_string(),
                target: RouteTarget::Cloud,
                vector: vec![0.0, 1.0],
            },
        ])
        .await
        .unwrap();
    let router = SemanticRouter::new(
        FakeEmbeddingProvider {
            embedding: vec![0.99, 0.01],
        },
        store,
        0.8,
        RouteTarget::Cloud,
        3,
    );

    let decision = router
        .decide("Classify this medical document")
        .await
        .unwrap();

    assert_eq!(decision.target, RouteTarget::Local);
    assert_eq!(decision.reason, "semantic_match");
    assert_eq!(
        decision.matched_example.as_deref(),
        Some("Patient discharge summary")
    );
    assert!(decision.score.is_some_and(|score| score > 0.9));
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn semantic_router_empty_text_returns_default() {
    let router = SemanticRouter::new(
        FakeEmbeddingProvider {
            embedding: vec![1.0, 0.0],
        },
        FakeRouteStore {
            matches: vec![RouteMatch {
                text: "something".to_string(),
                target: RouteTarget::Local,
                score: 0.99,
            }],
        },
        0.8,
        RouteTarget::Cloud,
        3,
    );

    let decision = router.decide("").await.unwrap();
    assert_eq!(decision.target, RouteTarget::Cloud);
    assert_eq!(decision.reason, "semantic_default_target");

    let decision_ws = router.decide("   ").await.unwrap();
    assert_eq!(decision_ws.target, RouteTarget::Cloud);
    assert_eq!(decision_ws.reason, "semantic_default_target");
}

#[tokio::test]
async fn semantic_router_empty_embedding_returns_default() {
    let router = SemanticRouter::new(
        FakeEmbeddingProvider { embedding: vec![] },
        FakeRouteStore {
            matches: vec![RouteMatch {
                text: "something".to_string(),
                target: RouteTarget::Local,
                score: 0.99,
            }],
        },
        0.8,
        RouteTarget::Cloud,
        3,
    );

    let decision = router.decide("some prompt").await.unwrap();
    assert_eq!(decision.target, RouteTarget::Cloud);
    assert_eq!(decision.reason, "semantic_default_target");
}

#[tokio::test]
async fn semantic_router_no_matches_returns_default() {
    let router = SemanticRouter::new(
        FakeEmbeddingProvider {
            embedding: vec![1.0, 0.0],
        },
        FakeRouteStore { matches: vec![] },
        0.8,
        RouteTarget::Cloud,
        3,
    );

    let decision = router.decide("anything").await.unwrap();
    assert_eq!(decision.target, RouteTarget::Cloud);
    assert_eq!(decision.reason, "semantic_default_target");
    assert!(decision.matched_example.is_none());
    assert!(decision.score.is_none());
}

#[tokio::test]
async fn semantic_router_below_threshold_still_populates_example_and_score() {
    let router = SemanticRouter::new(
        FakeEmbeddingProvider {
            embedding: vec![1.0, 0.0],
        },
        FakeRouteStore {
            matches: vec![RouteMatch {
                text: "Some example".to_string(),
                target: RouteTarget::Local,
                score: 0.5,
            }],
        },
        0.9,
        RouteTarget::Cloud,
        3,
    );

    let decision = router.decide("test prompt").await.unwrap();
    assert_eq!(decision.target, RouteTarget::Cloud);
    assert_eq!(decision.reason, "semantic_default_target");
    assert_eq!(decision.matched_example.as_deref(), Some("Some example"));
    assert_eq!(decision.score, Some(0.5));
}

#[tokio::test]
async fn semantic_router_exactly_at_threshold_fails() {
    let router = SemanticRouter::new(
        FakeEmbeddingProvider {
            embedding: vec![1.0, 0.0],
        },
        FakeRouteStore {
            matches: vec![RouteMatch {
                text: "Borderline".to_string(),
                target: RouteTarget::Local,
                score: 0.8,
            }],
        },
        0.8,
        RouteTarget::Cloud,
        3,
    );

    let decision = router.decide("edge case").await.unwrap();
    assert_eq!(decision.target, RouteTarget::Local);
    assert_eq!(decision.reason, "semantic_match");
}

#[test]
fn keyword_router_case_insensitive() {
    let router = Router::with_keyword_fallback(
        "https://api.openai.com",
        Some("http://localhost:8001".to_string()),
        vec!["patient".to_string()],
        RouteTarget::Cloud,
    );

    let decision = router.decide("PATIENT NOTES FROM DR. SMITH");
    assert_eq!(decision.target, RouteTarget::Local);
}

#[test]
fn keyword_router_empty_keywords_always_default() {
    let router = Router::with_keyword_fallback(
        "https://api.openai.com",
        Some("http://localhost:8001".to_string()),
        vec![],
        RouteTarget::Cloud,
    );

    let decision = router.decide("patient medical discharge");
    assert_eq!(decision.target, RouteTarget::Cloud);
    assert_eq!(decision.reason, "default_target");
}

#[test]
fn keyword_router_whitespace_keywords_filtered() {
    let router = Router::with_keyword_fallback(
        "https://api.openai.com",
        Some("http://localhost:8001".to_string()),
        vec!["  ".to_string(), "".to_string(), "medical".to_string()],
        RouteTarget::Cloud,
    );

    let decision = router.decide("general question");
    assert_eq!(decision.target, RouteTarget::Cloud);
    let decision2 = router.decide("medical note");
    assert_eq!(decision2.target, RouteTarget::Local);
}
