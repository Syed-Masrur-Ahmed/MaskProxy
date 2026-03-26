// masker/ner.rs
// Named Entity Recognition via GLiNER / DeBERTa-v3 loaded as an ONNX model.
// Uses the `ort` crate (ONNX Runtime bindings for Rust).
//
// Flow:
//   1. Load quantized INT8 ONNX model from disk at startup (once)
//   2. For each text string:
//       a. Preprocess — tokenize text, convert to token IDs, build attention mask
//       b. Run inference — feed tensors to ONNX session, get logit outputs
//       c. Postprocess — argmax over logits, decode BIO tags, extract entity spans
//   3. Return Vec<Entity> with text, type, start/end byte offsets
//
// Entity types detected: PERSON, EMAIL, PHONE, ORG, LOCATION, SSN, CREDIT_CARD
//
// Note: ONNX inference is CPU-bound. Run on a blocking thread via tokio::task::spawn_blocking
//       to avoid blocking the async Tokio runtime.

// TODO: define Entity struct { text: String, entity_type: String, start: usize, end: usize }
// TODO: define NER struct (holds ort::Session)
// TODO: impl NER
//   - fn new(model_path: &str) -> Result<Self>   (load ONNX session from file)
//   - async fn detect_entities(text: &str) -> Result<Vec<Entity>>
//       - spawn_blocking to run ONNX inference off the async thread
//       - fn preprocess(text) -> Result<InputTensors>
//       - fn postprocess(text, outputs) -> Result<Vec<Entity>>
