use super::{
    append_chunk_with_limit, extract_prompt_text, infer_provider, provider_base_url,
    resolve_upstream, sha256_hex, should_override_cloud_upstream, MaskProxy, UpstreamTarget,
};
use anyhow::Result;
use async_trait::async_trait;
use crate::masker::ner::NER;
use crate::masker::PrivacyConfig;
use crate::router::{EmbeddingProvider, RouteTarget, Router, SemanticRouteStore};
use crate::state::lancedb::RouteMatch;
use crate::state::redis::RedisState;

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

async fn build_test_proxy(router: Router) -> MaskProxy {
    let redis = RedisState::new("redis://127.0.0.1:6379")
        .await
        .expect("redis client should construct without connecting");
    MaskProxy::new(redis, NER::disabled(), router)
}

#[test]
fn extract_prompt_text_collects_messages_and_prompt_fragments() {
    let body = serde_json::json!({
        "messages": [
            {"content": "hello"},
            {"content": [{"text": "world"}]}
        ],
        "prompt": ["from", "proxy"]
    });

    let extracted = extract_prompt_text(&body.to_string());

    assert_eq!(extracted, "hello\nworld\nfrom\nproxy");
}

#[test]
fn infer_provider_uses_current_model_family_prefixes() {
    assert_eq!(infer_provider("gpt-4o"), "openai");
    assert_eq!(infer_provider("o1-preview"), "openai");
    assert_eq!(infer_provider("o3-mini"), "openai");
    assert_eq!(infer_provider("claude-3-5-sonnet"), "anthropic");
    assert_eq!(infer_provider("gemini-2.0-flash"), "gemini");
}

#[test]
fn provider_base_url_returns_expected_public_hosts() {
    assert_eq!(provider_base_url("openai"), Some("https://api.openai.com"));
    assert_eq!(
        provider_base_url("anthropic"),
        Some("https://api.anthropic.com")
    );
    assert_eq!(
        provider_base_url("gemini"),
        Some("https://generativelanguage.googleapis.com")
    );
    assert_eq!(provider_base_url("unknown"), None);
}

#[test]
fn override_cloud_upstream_only_for_known_public_provider_hosts() {
    assert!(should_override_cloud_upstream(
        "https://api.openai.com/v1/chat/completions"
    ));
    assert!(should_override_cloud_upstream(
        "https://api.anthropic.com/v1/messages"
    ));
    assert!(!should_override_cloud_upstream(
        "http://127.0.0.1:18081/v1/chat/completions"
    ));
    assert!(!should_override_cloud_upstream(
        "http://localhost:8088/v1/chat/completions"
    ));
    assert!(!should_override_cloud_upstream(
        "https://example.internal/v1/chat/completions"
    ));
}

#[test]
fn proxy_key_hashing_uses_full_maskproxy_key() {
    assert_ne!(sha256_hex("mp_test_key"), sha256_hex("test_key"));
}

#[test]
fn extract_prompt_text_collects_message_content_and_prompt() {
    let body = r#"{
        "messages": [
            {"role": "user", "content": "Summarize this patient note"},
            {"role": "assistant", "content": [{"type": "text", "text": "Draft reply"}]}
        ],
        "prompt": ["Classify this as urgent"]
    }"#;

    let extracted = extract_prompt_text(body);

    assert!(extracted.contains("Summarize this patient note"));
    assert!(extracted.contains("Draft reply"));
    assert!(extracted.contains("Classify this as urgent"));
}

#[test]
fn extract_prompt_text_returns_empty_string_for_invalid_json() {
    assert_eq!(extract_prompt_text("not-json"), "");
}

#[test]
fn resolve_upstream_parses_https_target() {
    let resolved = resolve_upstream(UpstreamTarget::Cloud(
        "https://api.openai.com/v1/chat/completions".into(),
    ))
    .expect("upstream should parse");

    assert_eq!(resolved.address, "api.openai.com:443");
    assert_eq!(resolved.host, "api.openai.com");
    assert!(resolved.tls);
}

#[test]
fn append_chunk_with_limit_accepts_chunk_within_limit() {
    let mut buffer = b"abc".to_vec();

    let appended = append_chunk_with_limit(&mut buffer, b"def", 6);

    assert!(appended);
    assert_eq!(buffer, b"abcdef");
}

#[test]
fn append_chunk_with_limit_rejects_chunk_beyond_limit() {
    let mut buffer = b"abc".to_vec();

    let appended = append_chunk_with_limit(&mut buffer, b"def", 5);

    assert!(!appended);
    assert_eq!(buffer, b"abc");
}

#[tokio::test]
async fn semantic_local_prepare_request_keeps_original_body() {
    let router = Router::with_semantic(
        "https://api.openai.com",
        Some("http://localhost:8001".to_string()),
        std::sync::Arc::new(FakeEmbeddingProvider {
            embedding: vec![1.0, 0.0],
        }),
        std::sync::Arc::new(FakeRouteStore {
            matches: vec![RouteMatch {
                text: "patient note".to_string(),
                target: RouteTarget::Local,
                score: 0.95,
            }],
        }),
        0.8,
        RouteTarget::Cloud,
        3,
    );
    let proxy = build_test_proxy(router).await;
    let body = serde_json::json!({
        "messages": [{"role": "user", "content": "Call Alice at 415-555-1234"}]
    })
    .to_string();

    let prepared = proxy.prepare_request(&body, &PrivacyConfig::default()).await.unwrap();

    assert_eq!(
        prepared.upstream,
        UpstreamTarget::Local("http://localhost:8001".to_string())
    );
    assert_eq!(prepared.request_body, bytes::Bytes::from(body.clone()));
    assert!(prepared.token_map.is_empty());
    assert!(String::from_utf8_lossy(&prepared.request_body).contains("415-555-1234"));
}

#[tokio::test]
async fn semantic_cloud_prepare_request_masks_body() {
    let router = Router::with_semantic(
        "https://api.openai.com",
        Some("http://localhost:8001".to_string()),
        std::sync::Arc::new(FakeEmbeddingProvider {
            embedding: vec![0.0, 1.0],
        }),
        std::sync::Arc::new(FakeRouteStore {
            matches: vec![RouteMatch {
                text: "general trivia".to_string(),
                target: RouteTarget::Cloud,
                score: 0.96,
            }],
        }),
        0.8,
        RouteTarget::Cloud,
        3,
    );
    let proxy = build_test_proxy(router).await;
    let body = serde_json::json!({
        "messages": [{"role": "user", "content": "Email alice@example.com"}]
    })
    .to_string();

    let prepared = proxy.prepare_request(&body, &PrivacyConfig::default()).await.unwrap();
    let prepared_text = String::from_utf8_lossy(&prepared.request_body);

    assert_eq!(
        prepared.upstream,
        UpstreamTarget::Cloud("https://api.openai.com".to_string())
    );
    assert!(prepared_text.contains("<<MASK:EMAIL_1:MASK>>"));
    assert!(!prepared_text.contains("alice@example.com"));
    assert_eq!(
        prepared.token_map.get("<<MASK:EMAIL_1:MASK>>"),
        Some(&"alice@example.com".to_string())
    );
}
