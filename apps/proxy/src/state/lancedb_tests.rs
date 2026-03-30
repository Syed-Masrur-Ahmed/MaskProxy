use std::env;
use std::fs;
use std::path::PathBuf;

use uuid::Uuid;

use super::*;

fn temp_lancedb_path() -> PathBuf {
    let path = env::temp_dir().join(format!("maskproxy-lancedb-{}", Uuid::new_v4()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[tokio::test]
async fn query_nearest_returns_empty_for_new_table() {
    let path = temp_lancedb_path();
    let store = LanceDbState::new(path.to_str().unwrap(), "route_examples", 2)
        .await
        .unwrap();

    let results = store.query_nearest(&[1.0, 0.0], 3).await.unwrap();

    assert!(results.is_empty());
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn rebuild_and_query_returns_ranked_matches() {
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

    let results = store.query_nearest(&[0.99, 0.01], 2).await.unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].text, "Patient discharge summary");
    assert_eq!(results[0].target, RouteTarget::Local);
    assert!(results[0].score > results[1].score);
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn rebuild_overwrites_existing_rows() {
    let path = temp_lancedb_path();
    let mut store = LanceDbState::new(path.to_str().unwrap(), "route_examples", 2)
        .await
        .unwrap();

    store
        .rebuild_from_examples(&[RouteExampleRow {
            text: "Old route".to_string(),
            target: RouteTarget::Cloud,
            vector: vec![0.0, 1.0],
        }])
        .await
        .unwrap();

    store
        .rebuild_from_examples(&[RouteExampleRow {
            text: "Replacement route".to_string(),
            target: RouteTarget::Local,
            vector: vec![1.0, 0.0],
        }])
        .await
        .unwrap();

    let results = store.query_nearest(&[1.0, 0.0], 3).await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].text, "Replacement route");
    assert_eq!(results[0].target, RouteTarget::Local);
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn zero_vector_dim_rejected() {
    let path = temp_lancedb_path();
    let result = LanceDbState::new(path.to_str().unwrap(), "route_examples", 0).await;
    assert!(result.is_err());
    assert!(
        result
            .err()
            .unwrap()
            .to_string()
            .contains("greater than zero"),
        "expected 'greater than zero' error"
    );
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn rebuild_rejects_wrong_vector_dim() {
    let path = temp_lancedb_path();
    let mut store = LanceDbState::new(path.to_str().unwrap(), "route_examples", 2)
        .await
        .unwrap();
    let result = store
        .rebuild_from_examples(&[RouteExampleRow {
            text: "mismatched dim".to_string(),
            target: RouteTarget::Cloud,
            vector: vec![1.0, 0.0, 0.5],
        }])
        .await;
    assert!(result.is_err());
    assert!(
        result.err().unwrap().to_string().contains("dimension"),
        "expected dimension mismatch error"
    );
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn query_rejects_wrong_embedding_dim() {
    let path = temp_lancedb_path();
    let store = LanceDbState::new(path.to_str().unwrap(), "route_examples", 2)
        .await
        .unwrap();
    let result = store.query_nearest(&[1.0, 0.0, 0.5], 3).await;
    assert!(result.is_err());
    assert!(
        result.err().unwrap().to_string().contains("dimension"),
        "expected dimension mismatch error"
    );
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn query_top_k_zero_returns_empty() {
    let path = temp_lancedb_path();
    let store = LanceDbState::new(path.to_str().unwrap(), "route_examples", 2)
        .await
        .unwrap();
    let results = store.query_nearest(&[1.0, 0.0], 0).await.unwrap();
    assert!(results.is_empty());
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn query_limit_respected() {
    let path = temp_lancedb_path();
    let mut store = LanceDbState::new(path.to_str().unwrap(), "route_examples", 2)
        .await
        .unwrap();
    store
        .rebuild_from_examples(&[
            RouteExampleRow {
                text: "Row A".to_string(),
                target: RouteTarget::Cloud,
                vector: vec![1.0, 0.0],
            },
            RouteExampleRow {
                text: "Row B".to_string(),
                target: RouteTarget::Local,
                vector: vec![0.0, 1.0],
            },
            RouteExampleRow {
                text: "Row C".to_string(),
                target: RouteTarget::Cloud,
                vector: vec![0.7, 0.7],
            },
        ])
        .await
        .unwrap();

    let results = store.query_nearest(&[1.0, 0.0], 1).await.unwrap();
    assert_eq!(results.len(), 1);
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn identical_vectors_score_near_one() {
    let path = temp_lancedb_path();
    let mut store = LanceDbState::new(path.to_str().unwrap(), "route_examples", 2)
        .await
        .unwrap();
    store
        .rebuild_from_examples(&[RouteExampleRow {
            text: "Exact match".to_string(),
            target: RouteTarget::Local,
            vector: vec![1.0, 0.0],
        }])
        .await
        .unwrap();

    let results = store.query_nearest(&[1.0, 0.0], 1).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(
        results[0].score > 0.99,
        "identical vectors should have similarity ~1.0, got {}",
        results[0].score
    );
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn orthogonal_vectors_score_near_zero() {
    let path = temp_lancedb_path();
    let mut store = LanceDbState::new(path.to_str().unwrap(), "route_examples", 2)
        .await
        .unwrap();
    store
        .rebuild_from_examples(&[RouteExampleRow {
            text: "Orthogonal".to_string(),
            target: RouteTarget::Cloud,
            vector: vec![0.0, 1.0],
        }])
        .await
        .unwrap();

    let results = store.query_nearest(&[1.0, 0.0], 1).await.unwrap();
    assert_eq!(results.len(), 1);
    assert!(
        results[0].score < 0.05,
        "orthogonal vectors should have similarity ~0.0, got {}",
        results[0].score
    );
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn similarity_ordering_matches_cosine() {
    let path = temp_lancedb_path();
    let mut store = LanceDbState::new(path.to_str().unwrap(), "route_examples", 2)
        .await
        .unwrap();
    store
        .rebuild_from_examples(&[
            RouteExampleRow {
                text: "Near".to_string(),
                target: RouteTarget::Local,
                vector: vec![0.95, 0.05],
            },
            RouteExampleRow {
                text: "Far".to_string(),
                target: RouteTarget::Cloud,
                vector: vec![0.05, 0.95],
            },
        ])
        .await
        .unwrap();

    let results = store.query_nearest(&[1.0, 0.0], 2).await.unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].text, "Near");
    assert_eq!(results[1].text, "Far");
    assert!(
        results[0].score > results[1].score,
        "near score {} should be > far score {}",
        results[0].score,
        results[1].score
    );
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn multiple_rebuilds_do_not_accumulate() {
    let path = temp_lancedb_path();
    let mut store = LanceDbState::new(path.to_str().unwrap(), "route_examples", 2)
        .await
        .unwrap();

    for i in 0..5 {
        store
            .rebuild_from_examples(&[RouteExampleRow {
                text: format!("Iteration {i}"),
                target: RouteTarget::Cloud,
                vector: vec![1.0, 0.0],
            }])
            .await
            .unwrap();
    }

    let results = store.query_nearest(&[1.0, 0.0], 100).await.unwrap();
    assert_eq!(results.len(), 1, "should only have latest rebuild's rows");
    assert_eq!(results[0].text, "Iteration 4");
    let _ = fs::remove_dir_all(path);
}

#[tokio::test]
async fn separate_table_names_independent() {
    let path = temp_lancedb_path();
    let mut store_a = LanceDbState::new(path.to_str().unwrap(), "table_a", 2)
        .await
        .unwrap();
    let mut store_b = LanceDbState::new(path.to_str().unwrap(), "table_b", 2)
        .await
        .unwrap();

    store_a
        .rebuild_from_examples(&[RouteExampleRow {
            text: "From A".to_string(),
            target: RouteTarget::Local,
            vector: vec![1.0, 0.0],
        }])
        .await
        .unwrap();
    store_b
        .rebuild_from_examples(&[RouteExampleRow {
            text: "From B".to_string(),
            target: RouteTarget::Cloud,
            vector: vec![0.0, 1.0],
        }])
        .await
        .unwrap();

    let results_a = store_a.query_nearest(&[1.0, 0.0], 10).await.unwrap();
    let results_b = store_b.query_nearest(&[0.0, 1.0], 10).await.unwrap();

    assert_eq!(results_a.len(), 1);
    assert_eq!(results_a[0].text, "From A");
    assert_eq!(results_b.len(), 1);
    assert_eq!(results_b[0].text, "From B");
    let _ = fs::remove_dir_all(path);
}
