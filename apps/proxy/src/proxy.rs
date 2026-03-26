// proxy.rs
// Defines MaskProxy and implements Pingora's ProxyHttp trait on it.
// This is the core of the proxy — it wires together masking, rehydration,
// routing, and state into Pingora's request/response lifecycle hooks.
//
// Hooks implemented:
//   - new_ctx              — create fresh per-request context
//   - upstream_peer        — decide which upstream server to forward to
//                            (OpenAI vs local inference via semantic router)
//   - upstream_request_filter — intercept outgoing request, mask PII in body
//   - response_filter      — intercept incoming response, rehydrate PII tokens
//
// Per-request context (CTX):
//   - session_id           — unique ID for this request (used as Redis key prefix)
//   - token_map            — PII token → real value mappings built during masking

// TODO: import Arc, pingora types, async_trait
// TODO: define RequestContext struct (session_id, token_map)
// TODO: define MaskProxy struct (holds Redis pool, NER session, Router, reqwest Client)
// TODO: impl ProxyHttp for MaskProxy
//   - type CTX = RequestContext
//   - fn new_ctx
//   - async fn upstream_peer  (call router to choose OpenAI vs local)
//   - async fn upstream_request_filter  (call masker, store tokens in ctx + Redis)
//   - async fn response_filter          (call rehydrator, read tokens from ctx)
