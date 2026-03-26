// state/lancedb.rs
// LanceDB vector store client for semantic routing.
// LanceDB is an embedded vector database — it runs in-process, no separate server.
//
// Responsibilities:
//   - Open (or create) a LanceDB database on the local filesystem at startup
//   - Provide a table of routing rules (prompt embedding → cloud/local label)
//   - Query the table with a new embedding to find the nearest routing rule
//
// Table schema (routing_rules):
//   - vector:  Float32[384]   (BGE-Small-v1.5 embedding dimension)
//   - label:   String         ("cloud" or "local")
//   - example: String         (original example prompt, for debugging)
//
// Seeding:
//   On first startup, if the table is empty, seed it with default routing rules
//   defined in a config file (models/routing_rules.json or similar).
//
// v1 scope: routing always returns cloud (OpenAI). LanceDB is wired up but
//           the routing decision is bypassed until Phase 2.

// TODO: define LanceDbState struct (holds lancedb::Connection)
// TODO: impl LanceDbState
//   - async fn new(db_path: &str) -> Result<Self>
//       - open or create LanceDB at db_path
//       - create routing_rules table if it doesn't exist
//       - seed default rules if table is empty
//   - async fn query_nearest(embedding: Vec<f32>, top_k: usize) -> Result<Vec<RoutingRule>>
//       - run nearest-neighbour search on routing_rules table
//       - return top_k results with label + distance
//   - async fn insert_rule(embedding: Vec<f32>, label: &str, example: &str) -> Result<()>
