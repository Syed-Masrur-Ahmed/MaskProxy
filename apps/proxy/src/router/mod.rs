// router/mod.rs
// Semantic router — decides whether a request should go to the cloud (OpenAI)
// or a local inference server (llama.cpp / vLLM with Phi-3.5 Mini).
//
// Flow:
//   1. Receive prompt text
//   2. Embed prompt using BGE-Small-v1.5 (fast, lightweight embedding model)
//   3. Query LanceDB vector store with the embedding
//   4. LanceDB returns the nearest routing rule + its label (cloud / local)
//   5. Return the upstream target URL
//
// Routing rules are seeded into LanceDB at startup (from config).
// Example rules:
//   - "write me a poem"        → local   (simple, no need for GPT-4)
//   - "analyse this contract"  → cloud   (complex reasoning needed)
//
// Note: Embedding inference is CPU-bound — use tokio::task::spawn_blocking.
// Note: LanceDB is an embedded DB (no separate server), runs in-process.
//
// v1 scope: cloud routing only (local sidecar skipped for v1 per scope cuts)

// TODO: define UpstreamTarget enum { Cloud(String), Local(String) }
// TODO: define Router struct (holds LanceDB connection, embedding model session)
// TODO: impl Router
//   - async fn new(lancedb_path: &str, model_path: &str) -> Result<Self>
//   - async fn route(prompt: &str) -> Result<UpstreamTarget>
//       - embed prompt via BGE-Small-v1.5
//       - query LanceDB for nearest routing rule
//       - return UpstreamTarget based on result label
