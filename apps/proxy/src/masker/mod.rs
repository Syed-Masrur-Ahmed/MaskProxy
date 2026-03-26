// masker/mod.rs
// Orchestrates PII detection and token replacement for outgoing requests.
//
// Flow:
//   1. Receive raw request body (JSON string from OpenAI request)
//   2. Parse JSON → extract message content strings
//   3. Call ner::NER::detect_entities() → get list of PII spans
//   4. Replace each span with a deterministic token: [PERSON_1], [EMAIL_1], etc.
//   5. Return masked JSON body + the token→real-value map
//
// The token map is returned to proxy.rs which stores it in:
//   - RequestContext (in-memory, for this request lifetime)
//   - Redis (persistent, TTL-scoped, for streaming rehydration)

pub mod ner;

// TODO: define MaskResult struct { masked_body: String, token_map: HashMap<String, String> }
// TODO: define Masker struct (holds NER instance)
// TODO: impl Masker
//   - fn new(ner: NER) -> Self
//   - async fn mask(body: &str) -> Result<MaskResult>
//       - parse JSON body to extract message content
//       - call self.ner.detect_entities(text)
//       - iterate entities, build token names, replace in text
//       - reconstruct JSON with masked content
//       - return MaskResult
