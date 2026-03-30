use std::fs;
use std::path::PathBuf;

use super::{ensure_semantic_routing_paths_exist, is_likely_local_upstream};
use uuid::Uuid;

fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!("maskproxy-main-tests-{}", Uuid::new_v4()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn local_upstream_detection_accepts_private_hosts() {
    assert!(is_likely_local_upstream("http://localhost:8001"));
    assert!(is_likely_local_upstream("http://127.0.0.1:8001"));
    assert!(is_likely_local_upstream("http://10.0.0.4:8001"));
    assert!(is_likely_local_upstream("http://192.168.1.5:8001"));
    assert!(is_likely_local_upstream("http://172.20.0.7:8001"));
    assert!(is_likely_local_upstream("http://router.local:8001"));
}

#[test]
fn local_upstream_detection_rejects_public_hosts() {
    assert!(!is_likely_local_upstream("https://api.openai.com"));
    assert!(!is_likely_local_upstream("https://example.com"));
    assert!(!is_likely_local_upstream("not-a-url"));
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
