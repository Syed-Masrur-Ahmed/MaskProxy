use super::{
    append_chunk_with_limit, extract_prompt_text, infer_provider, provider_base_url,
    resolve_upstream, sha256_hex, should_override_cloud_upstream, UpstreamTarget,
};

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
