use std::fs;
use std::path::PathBuf;

use super::{ensure_ner_health, ensure_semantic_routing_paths_exist, validate_local_upstream_base_url};
use crate::masker::ner::NER;
use uuid::Uuid;

fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!("maskproxy-main-tests-{}", Uuid::new_v4()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn local_upstream_validation_accepts_private_hosts() {
    assert!(validate_local_upstream_base_url(Some("http://localhost:8001")).is_ok());
    assert!(validate_local_upstream_base_url(Some("http://127.0.0.1:8001")).is_ok());
    assert!(validate_local_upstream_base_url(Some("http://10.0.0.4:8001")).is_ok());
    assert!(validate_local_upstream_base_url(Some("http://192.168.1.5:8001")).is_ok());
    assert!(validate_local_upstream_base_url(Some("http://172.20.0.7:8001")).is_ok());
}

#[test]
fn local_upstream_validation_rejects_public_or_invalid_hosts() {
    assert!(validate_local_upstream_base_url(Some("http://8.8.8.8:8001")).is_err());
    assert!(validate_local_upstream_base_url(Some("http://1.1.1.1:8001")).is_err());
    assert!(validate_local_upstream_base_url(Some("not-a-url")).is_err());
}

#[tokio::test]
async fn ner_health_check_allows_disabled_ner() {
    ensure_ner_health(&NER::disabled()).await.unwrap();
}

#[test]
fn semantic_path_validation_requires_all_files() {
    let dir = temp_dir();
    let model = dir.join("model.onnx");
    let tokenizer = dir.join("tokenizer.json");
    let examples = dir.join("routes.json");

    fs::write(&model, b"model").unwrap();
    fs::write(&tokenizer, b"tokenizer").unwrap();

    let error = ensure_semantic_routing_paths_exist(&model, &tokenizer, &examples).unwrap_err();

    assert!(error.to_string().contains("examples file not found"));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn semantic_path_validation_accepts_existing_files() {
    let dir = temp_dir();
    let model = dir.join("model.onnx");
    let tokenizer = dir.join("tokenizer.json");
    let examples = dir.join("routes.json");

    fs::write(&model, b"model").unwrap();
    fs::write(&tokenizer, b"tokenizer").unwrap();
    fs::write(&examples, b"[]").unwrap();

    ensure_semantic_routing_paths_exist(&model, &tokenizer, &examples).unwrap();
    let _ = fs::remove_dir_all(dir);
}
