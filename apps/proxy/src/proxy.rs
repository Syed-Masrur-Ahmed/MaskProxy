// proxy.rs
// Implements Pingora's ProxyHttp trait, wiring together auth, masking, rehydration,
// and routing into a request/response pipeline.
//
// The core flow:
//   1. request_filter       — validate API key, resolve provider, fetch provider API key
//   2. upstream_peer        — return target upstream (OpenAI/Anthropic/Gemini)
//   3. upstream_request_filter — swap auth header, remove Content-Length
//   4. request_body_filter  — buffer & mask PII in request body
//   5. response_filter      — remove Content-Length
//   6. response_body_filter — buffer & rehydrate PII tokens in response

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use bytes::Bytes;
use http::StatusCode;
use pingora_core::upstreams::peer::HttpPeer;
use pingora_core::Result as PingoraResult;
use pingora_http::{RequestHeader, ResponseHeader};
use pingora_proxy::{ProxyHttp, Session};
use sha2::{Digest, Sha256};
use tracing::{error, instrument, warn};
use uuid::Uuid;

use crate::masker::{Masker, MaskResult};
use crate::masker::ner::NER;
use crate::rehydrator::Rehydrator;
use crate::router::Router;
use crate::state::redis::RedisState;

// ─────────────────────────────────────────────────────────────────────────────
// RequestContext: Per-request mutable state, passed through Pingora hooks
// ─────────────────────────────────────────────────────────────────────────────

pub struct RequestContext {
    /// Unique ID for this request (UUID v4), used as Redis key prefix for token maps
    pub session_id: String,

    /// Token map from masking: "[PERSON_1]" -> "John Smith", etc.
    /// Populated during request_body_filter, consumed in response_body_filter
    pub token_map: HashMap<String, String>,

    /// Inferred provider: "openai", "anthropic", "gemini"
    pub provider: String,

    /// Real (decrypted) provider API key, resolved during request_filter
    pub provider_api_key: String,

    /// Upstream host for this provider: "api.openai.com", etc.
    pub upstream_host: String,

    /// Model name from X-MaskProxy-Model header (e.g., "gpt-4o")
    pub model: String,

    /// Buffer for accumulating request body chunks across request_body_filter calls
    pub request_body_buf: Vec<u8>,

    /// Buffer for accumulating response body chunks across response_body_filter calls
    pub response_body_buf: Vec<u8>,
}

impl Default for RequestContext {
    fn default() -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            token_map: HashMap::new(),
            provider: "openai".to_string(),
            provider_api_key: String::new(),
            upstream_host: "api.openai.com".to_string(),
            model: "gpt-4o".to_string(),
            request_body_buf: Vec::new(),
            response_body_buf: Vec::new(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MaskProxy: Orchestrator holding all subsystems
// ─────────────────────────────────────────────────────────────────────────────

pub struct MaskProxy {
    redis: Arc<RedisState>,
    masker: Arc<Masker>,
    rehydrator: Arc<Rehydrator>,
    router: Arc<Router>,
    http_client: reqwest::Client,
    api_backend_url: String,
}

impl MaskProxy {
    /// Create a new MaskProxy instance.
    /// Called from main.rs with: MaskProxy::new(redis, ner, router, api_backend_url)
    pub fn new(
        redis: RedisState,
        ner: NER,
        router: Router,
        api_backend_url: String,
    ) -> Self {
        Self {
            redis: Arc::new(redis),
            masker: Arc::new(Masker::new(ner)),
            rehydrator: Arc::new(Rehydrator::new()),
            router: Arc::new(router),
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to build reqwest client"),
            api_backend_url,
        }
    }

    /// SHA-256 hash a string and return lowercase hex
    fn sha256_hex(input: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Resolve the real provider API key.
    /// Tries Redis cache first, falls back to FastAPI backend call.
    #[instrument(skip_all, fields(user_id, provider))]
    async fn resolve_provider_key(
        &self,
        user_id: &str,
        provider: &str,
        raw_mp_key: &str,
    ) -> Result<String> {
        let cache_key = format!("provider_key:{}:{}", user_id, provider);

        // 1. Try Redis cache
        match self.redis.get_value(&cache_key).await {
            Ok(Some(cached)) => {
                return Ok(cached);
            }
            Err(e) => {
                warn!("Redis cache lookup failed: {}", e);
            }
            _ => {}
        }

        // 2. Cache miss — call FastAPI backend
        let url = format!(
            "{}/v1/provider-keys?provider={}",
            self.api_backend_url, provider
        );

        let response = self
            .http_client
            .get(&url)
            .header("authorization", format!("Bearer mp_{}", raw_mp_key))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "FastAPI returned {} for provider key lookup",
                response.status()
            ));
        }

        // 3. Parse response: { "api_key": "sk-..." }
        let body: serde_json::Value = response.json().await?;
        let api_key = body["api_key"]
            .as_str()
            .ok_or_else(|| anyhow!("Missing api_key in FastAPI response"))?
            .to_string();

        // 4. Cache in Redis with 300s TTL
        if let Err(e) = self
            .redis
            .set_value(&cache_key, &api_key, 300)
            .await
        {
            warn!("Failed to cache provider key in Redis: {}", e);
            // Non-fatal — continue with the key we got
        }

        Ok(api_key)
    }

    /// Send a JSON error response and short-circuit the request pipeline.
    async fn send_error(
        session: &mut Session,
        status: u16,
        message: &str,
    ) -> PingoraResult<()> {
        use http::StatusCode;

        let status_code = StatusCode::from_u16(status)
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        let mut resp = ResponseHeader::build(status_code, None)?;
        resp.insert_header("content-type", "application/json")?;

        let body = format!(r#"{{"error": "{}"}}"#, message);
        let body_bytes = Bytes::from(body);

        resp.insert_header("content-length", body_bytes.len().to_string())?;

        session.write_response_header(Box::new(resp), false).await?;
        session.write_response_body(Some(body_bytes), true).await?;

        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ProxyHttp trait implementation for Pingora 0.4
// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl ProxyHttp for MaskProxy {
    type CTX = RequestContext;

    fn new_ctx(&self) -> Self::CTX {
        RequestContext::default()
    }

    /// Hook 1: request_filter
    /// Authenticate the API key, infer the provider, and resolve the real provider key.
    #[instrument(skip_all)]
    async fn request_filter(
        &self,
        session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> PingoraResult<bool> {
        // 1. Extract and validate Authorization header
        let auth_header = session
            .req_header()
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !auth_header.starts_with("Bearer mp_") {
            Self::send_error(session, 401, "Missing or invalid MaskProxy API key").await?;
            return Ok(true); // short-circuit
        }

        let raw_key = &auth_header["Bearer mp_".len()..];

        // 2. SHA-256 hash the raw key and look up in Redis
        let key_hash = Self::sha256_hex(raw_key);
        let user_id = match self.redis.get_value(&format!("api_key_valid:{}", key_hash)).await {
            Ok(Some(uid)) => uid,
            Ok(None) => {
                Self::send_error(session, 401, "API key not found or expired").await?;
                return Ok(true); // short-circuit
            }
            Err(e) => {
                error!("Redis error during auth: {}", e);
                Self::send_error(session, 502, "Auth service unavailable").await?;
                return Ok(true); // short-circuit
            }
        };

        // 3. Read optional X-MaskProxy-Model header
        if let Some(model_header) = session.req_header().headers.get("x-maskproxy-model") {
            if let Ok(model_str) = model_header.to_str() {
                ctx.model = model_str.to_string();
            }
        }

        // 4. Infer provider and host from model name
        ctx.provider = crate::infer_provider(&ctx.model).to_string();
        ctx.upstream_host = crate::provider_base_url(&ctx.provider).to_string();

        // 5. Resolve the real provider API key
        match self
            .resolve_provider_key(&user_id, &ctx.provider, raw_key)
            .await
        {
            Ok(key) => {
                ctx.provider_api_key = key;
            }
            Err(e) => {
                error!("Failed to resolve provider key: {}", e);
                Self::send_error(session, 502, "Failed to resolve provider key").await?;
                return Ok(true); // short-circuit
            }
        }

        Ok(false) // continue pipeline
    }

    /// Hook 2: upstream_peer
    /// Determine the target upstream server from ctx.upstream_host
    async fn upstream_peer(
        &self,
        _session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> PingoraResult<Box<HttpPeer>> {
        let host_port = format!("{}:443", ctx.upstream_host);
        let peer = HttpPeer::new(host_port, true, ctx.upstream_host.clone());
        Ok(Box::new(peer))
    }

    /// Hook 3: upstream_request_filter
    /// Modify headers going to the upstream server:
    ///   - Swap Authorization header (mp_ key -> real provider key)
    ///   - Remove Content-Length (body will change size after masking)
    ///   - Set Host header
    ///   - Add provider-specific headers (e.g., Anthropic API version)
    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut Self::CTX,
    ) -> PingoraResult<()> {
        // Swap Authorization header
        upstream_request.insert_header(
            "authorization",
            format!("Bearer {}", ctx.provider_api_key),
        )?;

        // Remove Content-Length — it will be invalid after masking.
        // Pingora will use Transfer-Encoding: chunked without it.
        upstream_request.remove_header("content-length");

        // Set Host header for the upstream
        upstream_request.insert_header("host", &ctx.upstream_host)?;

        // Provider-specific headers
        if ctx.provider == "anthropic" {
            upstream_request.insert_header("anthropic-version", "2023-06-01")?;
        }

        Ok(())
    }

    /// Hook 4: request_body_filter
    /// Buffer incoming request body chunks, mask PII on end-of-stream, emit full body.
    /// Intermediate chunks are suppressed (replaced with None).
    async fn request_body_filter(
        &self,
        _session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> PingoraResult<()> {
        // 1. Accumulate chunk (body.take() suppresses this chunk from going upstream)
        if let Some(chunk) = body.take() {
            ctx.request_body_buf.extend_from_slice(&chunk);
        }

        // 2. On end-of-stream: mask the complete body
        if end_of_stream {
            let raw_body = std::mem::take(&mut ctx.request_body_buf);
            let body_str = String::from_utf8(raw_body).map_err(|_| {
                pingora_core::Error::new_str("Request body is not valid UTF-8")
            })?;

            match self.masker.mask(&body_str).await {
                Ok(MaskResult {
                    masked_body,
                    token_map,
                }) => {
                    // Store token map in context
                    ctx.token_map = token_map.clone();

                    // Store in Redis for streaming rehydration (TTL 300s)
                    if let Err(e) = self
                        .redis
                        .store_token_map(&ctx.session_id, &token_map, 300)
                        .await
                    {
                        warn!("Failed to store token map in Redis: {}", e);
                        // Non-fatal: ctx.token_map is still populated for this request
                    }

                    // Emit masked body as the final chunk
                    *body = Some(Bytes::from(masked_body));
                }
                Err(e) => {
                    // Fail-open: forward unmasked body (availability > privacy for v1)
                    error!("Masker failed, forwarding unmasked body: {}", e);
                    *body = Some(Bytes::from(body_str));
                }
            }
        }
        // If not EOS: body is now None (chunk suppressed), nothing sent upstream

        Ok(())
    }

    /// Hook 5: response_filter
    /// Modify response headers before they go to the client.
    /// Remove Content-Length because the body will change size after rehydration.
    async fn response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        _ctx: &mut Self::CTX,
    ) -> PingoraResult<()> {
        // Remove Content-Length — rehydrated body will be a different size
        upstream_response.remove_header("content-length");

        Ok(())
    }

    /// Hook 6: response_body_filter (SYNC — Pingora defines this as `fn`, not `async fn`)
    /// Buffer incoming response body chunks, rehydrate PII tokens on end-of-stream.
    /// Intermediate chunks are suppressed (replaced with None).
    fn response_body_filter(
        &self,
        _session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> PingoraResult<Option<std::time::Duration>> {
        // 1. Accumulate chunk
        if let Some(chunk) = body.take() {
            ctx.response_body_buf.extend_from_slice(&chunk);
        }

        // 2. On end-of-stream: rehydrate if we have a token map
        if end_of_stream {
            let raw_response = std::mem::take(&mut ctx.response_body_buf);
            let response_str = String::from_utf8(raw_response).map_err(|_| {
                pingora_core::Error::new_str("Response body is not valid UTF-8")
            })?;

            if !ctx.token_map.is_empty() {
                match self.rehydrator.rehydrate(&response_str, &ctx.token_map) {
                    Ok(rehydrated) => {
                        *body = Some(Bytes::from(rehydrated));
                    }
                    Err(e) => {
                        // Fail-open: return masked response (client still gets a response)
                        error!("Rehydrator failed, returning masked response: {}", e);
                        *body = Some(Bytes::from(response_str));
                    }
                }
            } else {
                // No masking occurred — pass through original
                *body = Some(Bytes::from(response_str));
            }
        }

        Ok(None)
    }
}
