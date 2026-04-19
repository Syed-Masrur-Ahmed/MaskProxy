use std::collections::{HashMap, HashSet};

use anyhow::anyhow;
use async_trait::async_trait;
use bytes::Bytes;
use pingora_core::upstreams::peer::HttpPeer;
use pingora_core::{Error, ErrorType::*, Result};
use pingora_http::ResponseHeader;
use pingora_proxy::{ProxyHttp, Session};
use reqwest::Client;
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

use crate::masker::ner::NER;
use crate::masker::Masker;
use crate::rehydrator::{Rehydrator, SseRehydrator};
use crate::router::{Router, UpstreamTarget};
use crate::state::redis::RedisState;

#[derive(Clone, Debug, Default)]
pub struct RequestContext {
    pub session_id: String,
    pub token_map: HashMap<String, String>,
    pub upstream: Option<ResolvedUpstream>,
    pub provider: String,
    pub provider_api_key: Option<String>,
    pub request_body: Option<Bytes>,
    pub request_body_replaced: bool,
    pub response_buffer: Vec<u8>,
    /// True when the upstream response is SSE (text/event-stream).
    pub is_sse: bool,
    /// SSE-aware streaming rehydrator — parses SSE events, extracts content
    /// deltas, and handles partial placeholders spanning multiple events.
    pub sse_rehydrator: SseRehydrator,
    // ── Logging fields ────────────────────────────────────────────────────
    pub request_start: Option<std::time::Instant>,
    pub model_hint: String,
    pub route: String,
    pub pii_detected_count: usize,
    pub pii_types: Vec<String>,
    pub masked_prompt_snippet: String,
    pub upstream_status_code: u16,
    pub raw_api_key: String,
}

impl RequestContext {
    pub fn new() -> Self {
        Self {
            session_id: format!("req-{}", Uuid::new_v4()),
            token_map: HashMap::new(),
            upstream: None,
            provider: "openai".to_string(),
            provider_api_key: None,
            request_body: None,
            request_body_replaced: false,
            response_buffer: Vec::new(),
            is_sse: false,
            sse_rehydrator: SseRehydrator::new(),
            request_start: Some(std::time::Instant::now()),
            model_hint: String::new(),
            route: String::new(),
            pii_detected_count: 0,
            pii_types: Vec::new(),
            masked_prompt_snippet: String::new(),
            upstream_status_code: 0,
            raw_api_key: String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedUpstream {
    address: String,
    host: String,
    tls: bool,
}

struct PreparedRequest {
    upstream: UpstreamTarget,
    request_body: Bytes,
    token_map: HashMap<String, String>,
}

#[derive(Clone)]
pub struct MaskProxy {
    pub redis: RedisState,
    pub router: Router,
    pub masker: Masker,
    pub rehydrator: Rehydrator,
    pub http_client: Client,
    pub backend_api_url: String,
}

impl MaskProxy {
    pub fn new(redis: RedisState, ner: NER, router: Router) -> Self {
        let masker = Masker::new(ner.clone());
        let rehydrator = Rehydrator::new();
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build reqwest client");

        Self {
            redis,
            router,
            masker,
            rehydrator,
            http_client,
            backend_api_url: "http://localhost:8000".to_string(),
        }
    }

    pub fn with_backend_api_url(mut self, api_backend_url: impl Into<String>) -> Self {
        self.backend_api_url = api_backend_url.into();
        self
    }

    pub fn create_ctx(&self) -> RequestContext {
        let _ = self;
        RequestContext::new()
    }

    async fn prepare_request(&self, body_text: &str) -> anyhow::Result<PreparedRequest> {
        let prompt = extract_prompt_text(body_text);
        let upstream = self.router.route(&prompt).await?;

        match upstream.clone() {
            UpstreamTarget::Cloud(_) => {
                let masked = self.masker.mask(body_text).await?;
                Ok(PreparedRequest {
                    upstream,
                    request_body: Bytes::from(masked.masked_body),
                    token_map: masked.token_map,
                })
            }
            UpstreamTarget::Local(_) => Ok(PreparedRequest {
                upstream,
                request_body: Bytes::from(body_text.to_string()),
                token_map: HashMap::new(),
            }),
        }
    }
}

const MAPPING_TTL_SECONDS: u64 = 3600;
const PROVIDER_KEY_TTL_SECONDS: u64 = 300;
const MAX_REQUEST_BODY_BYTES: usize = 1 * 1024 * 1024;
const MAX_RESPONSE_BUFFER_BYTES: usize = 50 * 1024 * 1024;

fn extract_prompt_text(body: &str) -> String {
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(body) else {
        return String::new();
    };

    let mut fragments = Vec::new();

    if let Some(messages) = payload
        .get("messages")
        .and_then(serde_json::Value::as_array)
    {
        for message in messages {
            if let Some(content) = message.get("content") {
                collect_content_fragments(content, &mut fragments);
            }
        }
    }

    if let Some(prompt) = payload.get("prompt") {
        match prompt {
            serde_json::Value::String(text) => fragments.push(text.clone()),
            serde_json::Value::Array(items) => {
                for item in items {
                    if let Some(text) = item.as_str() {
                        fragments.push(text.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    fragments.join("\n")
}

fn extract_model_hint(session: &Session) -> String {
    session
        .req_header()
        .headers
        .get("x-maskproxy-model")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("gpt-4o")
        .to_string()
}

fn collect_content_fragments(value: &serde_json::Value, fragments: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => fragments.push(text.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                match item {
                    serde_json::Value::String(text) => fragments.push(text.clone()),
                    serde_json::Value::Object(map) => {
                        if let Some(serde_json::Value::String(text)) = map.get("text") {
                            fragments.push(text.clone());
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

fn infer_provider(model: &str) -> &'static str {
    // The Rust port mirrors the current model-family shorthand used in the
    // Python proxy. These prefixes intentionally target known provider naming
    // conventions, not arbitrary future model IDs.
    if model.starts_with("gpt-") || model.starts_with("o1-") || model.starts_with("o3-") {
        "openai"
    } else if model.starts_with("claude-") {
        "anthropic"
    } else if model.starts_with("gemini-") {
        "gemini"
    } else {
        "openai"
    }
}

fn provider_base_url(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" => Some("https://api.openai.com"),
        "anthropic" => Some("https://api.anthropic.com"),
        "gemini" => Some("https://generativelanguage.googleapis.com"),
        _ => None,
    }
}

fn should_override_cloud_upstream(current_url: &str) -> bool {
    let Ok(parsed) = Url::parse(current_url) else {
        return false;
    };

    matches!(
        parsed.host_str(),
        Some("api.openai.com")
            | Some("api.anthropic.com")
            | Some("generativelanguage.googleapis.com")
    )
}

fn append_chunk_with_limit(buffer: &mut Vec<u8>, chunk: &[u8], limit: usize) -> bool {
    if buffer.len().saturating_add(chunk.len()) > limit {
        return false;
    }
    buffer.extend_from_slice(chunk);
    true
}

fn resolve_upstream(
    target: UpstreamTarget,
) -> std::result::Result<ResolvedUpstream, url::ParseError> {
    // The upstream base URL only contributes scheme/host/port. Pingora preserves
    // the downstream request URI, so callers must send the full path they want
    // upstream (for example `/v1/chat/completions`).
    let raw = match target {
        UpstreamTarget::Cloud(url) | UpstreamTarget::Local(url) => url,
    };

    let parsed = Url::parse(&raw)?;
    let host = parsed
        .host_str()
        .ok_or(url::ParseError::EmptyHost)?
        .to_string();
    let port = parsed
        .port_or_known_default()
        .ok_or(url::ParseError::InvalidPort)?;
    let tls = parsed.scheme() == "https";

    Ok(ResolvedUpstream {
        address: format!("{host}:{port}"),
        host,
        tls,
    })
}

/// Extract unique PII type names from token_map keys.
/// Keys have the format `<<MASK:TYPE_N:MASK>>`, e.g. `<<MASK:EMAIL_1:MASK>>`.
fn extract_pii_types(token_map: &HashMap<String, String>) -> Vec<String> {
    let mut types: HashSet<String> = HashSet::new();
    for key in token_map.keys() {
        if let Some(inner) = key.strip_prefix("<<MASK:").and_then(|s| s.strip_suffix(":MASK>>")) {
            // inner = "EMAIL_1", "PERSON_2", "PHONE_NUMBER_1", etc.
            // Strip the trailing _N (digit suffix).
            if let Some(last_underscore) = inner.rfind('_') {
                let suffix = &inner[last_underscore + 1..];
                if suffix.chars().all(|c| c.is_ascii_digit()) {
                    types.insert(inner[..last_underscore].to_string());
                    continue;
                }
            }
            types.insert(inner.to_string());
        }
    }
    types.into_iter().collect()
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

async fn send_cors_preflight(session: &mut Session) -> Result<()> {
    let mut response = ResponseHeader::build(204, None)
        .map_err(|error| Error::because(InternalError, "failed to build preflight response", error))?;
    for (name, value) in cors_headers() {
        response.insert_header(name, value).map_err(|error| {
            Error::because(InternalError, "failed to set CORS header", error)
        })?;
    }
    response.insert_header("content-length", "0").map_err(|error| {
        Error::because(InternalError, "failed to set content-length", error)
    })?;
    session.write_response_header(Box::new(response), false).await?;
    session.write_response_body(Some(Bytes::new()), true).await?;
    Ok(())
}

fn cors_headers() -> [(&'static str, &'static str); 4] {
    [
        ("Access-Control-Allow-Origin", "*"),
        ("Access-Control-Allow-Methods", "POST, OPTIONS"),
        ("Access-Control-Allow-Headers", "Content-Type, Authorization, x-maskproxy-model"),
        ("Access-Control-Max-Age", "86400"),
    ]
}

async fn send_json_error(session: &mut Session, status: u16, message: &str) -> Result<()> {
    let mut response = ResponseHeader::build(status, None)
        .map_err(|error| Error::because(InternalError, "failed to build error response", error))?;
    response
        .insert_header("content-type", "application/json")
        .map_err(|error| {
            Error::because(InternalError, "failed to set error content type", error)
        })?;

    let body = Bytes::from(format!(r#"{{"error":"{}"}}"#, message));
    response
        .insert_header("content-length", body.len().to_string())
        .map_err(|error| {
            Error::because(InternalError, "failed to set error content length", error)
        })?;

    session
        .write_response_header(Box::new(response), false)
        .await?;
    session.write_response_body(Some(body), true).await?;
    Ok(())
}

impl MaskProxy {
    async fn resolve_provider_key(
        &self,
        user_id: &str,
        provider: &str,
        raw_proxy_key: &str,
    ) -> anyhow::Result<String> {
        let cache_key = format!("provider_key:{user_id}:{provider}");
        if let Some(cached) = self.redis.get_value(&cache_key).await? {
            return Ok(cached);
        }

        let response = self
            .http_client
            .get(format!(
                "{}/v1/provider-keys/resolve?provider={provider}",
                self.backend_api_url
            ))
            .header("authorization", format!("Bearer {raw_proxy_key}"))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "provider key lookup failed with status {}",
                response.status()
            ));
        }

        let body: serde_json::Value = response.json().await?;
        let api_key = body
            .get("api_key")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow!("missing api_key in backend response"))?
            .to_string();

        let _ = self
            .redis
            .set_value(&cache_key, &api_key, PROVIDER_KEY_TTL_SECONDS)
            .await;

        Ok(api_key)
    }
}

#[async_trait]
impl ProxyHttp for MaskProxy {
    type CTX = RequestContext;

    fn new_ctx(&self) -> Self::CTX {
        self.create_ctx()
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool>
    where
        Self::CTX: Send + Sync,
    {
        if session.req_header().method.as_str() == "OPTIONS" {
            send_cors_preflight(session).await?;
            return Ok(true);
        }

        let auth_header = session
            .req_header()
            .headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string();

        if !auth_header.starts_with("Bearer mp_") {
            send_json_error(session, 401, "Missing or invalid MaskProxy API key").await?;
            return Ok(true);
        }

        let raw_proxy_key = auth_header["Bearer ".len()..].to_string();
        ctx.raw_api_key = raw_proxy_key.clone();
        let hashed_key = sha256_hex(&raw_proxy_key);
        let user_id = match self
            .redis
            .get_value(&format!("api_key_valid:{hashed_key}"))
            .await
        {
            Ok(Some(user_id)) => user_id,
            Ok(None) => {
                send_json_error(session, 401, "API key not found or expired").await?;
                return Ok(true);
            }
            Err(error) => {
                let _ = send_json_error(session, 502, "Auth service unavailable").await;
                tracing::error!("failed to validate proxy API key in redis: {error}");
                return Ok(true);
            }
        };

        let model_hint = extract_model_hint(session);
        ctx.model_hint = model_hint.clone();
        ctx.provider = infer_provider(&model_hint).to_string();

        session.downstream_session.enable_retry_buffering();

        let mut body = Vec::new();
        while let Some(chunk) = session
            .downstream_session
            .read_request_body()
            .await
            .map_err(|error| {
                Error::because(ReadError, "failed reading downstream request body", error)
            })?
        {
            if !append_chunk_with_limit(&mut body, &chunk, MAX_REQUEST_BODY_BYTES) {
                send_json_error(session, 413, "Request body too large").await?;
                return Ok(true);
            }
        }

        let body_text = String::from_utf8(body).map_err(|error| {
            Error::because(InvalidHTTPHeader, "request body was not valid UTF-8", error)
        })?;

        let prepared = self.prepare_request(&body_text).await.map_err(|error| {
            Error::because(HTTPStatus(503), "failed to prepare upstream request", error)
        })?;
        let mut resolved = resolve_upstream(prepared.upstream.clone())
            .map_err(|error| Error::because(InternalError, "invalid upstream URL", error))?;

        // Populate logging metadata
        ctx.pii_detected_count = prepared.token_map.len();
        ctx.pii_types = extract_pii_types(&prepared.token_map);
        let masked_body_str = String::from_utf8_lossy(&prepared.request_body);
        let prompt_text = extract_prompt_text(&masked_body_str);
        ctx.masked_prompt_snippet = prompt_text.chars().take(200).collect();

        match prepared.upstream {
            UpstreamTarget::Cloud(current_cloud_url) => {
                ctx.route = "cloud".to_string();
                let provider_api_key = match self
                    .resolve_provider_key(&user_id, &ctx.provider, &raw_proxy_key)
                    .await
                {
                    Ok(key) => key,
                    Err(error) => {
                        let _ =
                            send_json_error(session, 502, "Failed to resolve provider key").await;
                        tracing::error!("failed to resolve provider key: {error}");
                        return Ok(true);
                    }
                };

                if should_override_cloud_upstream(&current_cloud_url) {
                    if let Some(provider_url) = provider_base_url(&ctx.provider) {
                        resolved =
                            resolve_upstream(UpstreamTarget::Cloud(provider_url.to_string()))
                                .map_err(|error| {
                                    Error::because(
                                        InternalError,
                                        "invalid provider base URL",
                                        error,
                                    )
                                })?;
                    }
                }

                if !prepared.token_map.is_empty() {
                    self.redis
                        .save_mapping(&ctx.session_id, &prepared.token_map, MAPPING_TTL_SECONDS)
                        .await
                        .map_err(|error| {
                            Error::because(
                                InternalError,
                                "failed to persist session mapping",
                                error,
                            )
                        })?;
                }

                ctx.provider_api_key = Some(provider_api_key);
                ctx.token_map = prepared.token_map;
                ctx.request_body = Some(prepared.request_body);
            }
            UpstreamTarget::Local(_) => {
                ctx.route = "local".to_string();
                ctx.request_body = Some(prepared.request_body);
            }
        }

        ctx.upstream = Some(resolved);
        Ok(false)
    }

    async fn upstream_peer(
        &self,
        _session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let resolved = ctx.upstream.clone().ok_or_else(|| {
            Error::explain(InternalError, "request_filter did not resolve an upstream")
        })?;
        let peer = Box::new(HttpPeer::new(
            resolved.address.as_str(),
            resolved.tls,
            resolved.host.clone(),
        ));
        Ok(peer)
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut pingora_http::RequestHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()>
    where
        Self::CTX: Send + Sync,
    {
        if let Some(upstream) = &ctx.upstream {
            upstream_request
                .insert_header("Host", upstream.host.clone())
                .map_err(|error| {
                    Error::because(InternalError, "failed to set Host header", error)
                })?;
        }
        upstream_request.remove_header("authorization");
        upstream_request.remove_header("x-api-key");
        upstream_request.remove_header("x-maskproxy-model");
        // Prevent compressed responses so the proxy can read and rehydrate the body
        upstream_request.remove_header("accept-encoding");

        if let Some(provider_api_key) = &ctx.provider_api_key {
            match ctx.provider.as_str() {
                "anthropic" => {
                    upstream_request
                        .insert_header("x-api-key", provider_api_key.clone())
                        .map_err(|error| {
                            Error::because(
                                InternalError,
                                "failed to set Anthropic key header",
                                error,
                            )
                        })?;
                    upstream_request
                        .insert_header("anthropic-version", "2023-06-01")
                        .map_err(|error| {
                            Error::because(
                                InternalError,
                                "failed to set Anthropic version header",
                                error,
                            )
                        })?;
                }
                "gemini" => {
                    upstream_request
                        .insert_header("authorization", format!("Bearer {provider_api_key}"))
                        .map_err(|error| {
                            Error::because(InternalError, "failed to set Gemini auth header", error)
                        })?;
                }
                _ => {
                    upstream_request
                        .insert_header("authorization", format!("Bearer {provider_api_key}"))
                        .map_err(|error| {
                            Error::because(
                                InternalError,
                                "failed to set OpenAI authorization header",
                                error,
                            )
                        })?;
                }
            }
        }
        if let Some(body) = &ctx.request_body {
            upstream_request.remove_header("Content-Length");
            upstream_request
                .insert_header("Content-Length", body.len().to_string())
                .map_err(|error| {
                    Error::because(InternalError, "failed to set Content-Length header", error)
                })?;
        }
        Ok(())
    }

    async fn request_body_filter(
        &self,
        _session: &mut Session,
        body: &mut Option<Bytes>,
        _end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<()>
    where
        Self::CTX: Send + Sync,
    {
        if ctx.request_body_replaced {
            *body = None;
            return Ok(());
        }

        if let Some(replacement) = ctx.request_body.clone() {
            *body = Some(replacement);
            ctx.request_body_replaced = true;
        }

        Ok(())
    }

    async fn response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()>
    where
        Self::CTX: Send + Sync,
    {
        ctx.upstream_status_code = upstream_response.status.as_u16();

        for (name, value) in cors_headers() {
            let _ = upstream_response.insert_header(name, value);
        }

        // Detect SSE responses so response_body_filter can stream rehydration
        // instead of buffering the entire body.
        ctx.is_sse = upstream_response
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.contains("text/event-stream"))
            .unwrap_or(false);

        if !ctx.token_map.is_empty() {
            upstream_response.remove_header("Content-Length");
            let _ = upstream_response.insert_header("Transfer-Encoding", "chunked");
        }
        Ok(())
    }

    fn response_body_filter(
        &self,
        _session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<Option<std::time::Duration>>
    where
        Self::CTX: Send + Sync,
    {
        if ctx.token_map.is_empty() {
            return Ok(None);
        }

        if ctx.is_sse {
            // --- Streaming SSE path ---
            // The SseRehydrator parses SSE events, extracts content deltas,
            // and handles partial placeholders that span multiple events.
            let mut output = String::new();

            if let Some(chunk) = body.take() {
                let chunk_str = String::from_utf8_lossy(&chunk);
                output = ctx
                    .sse_rehydrator
                    .process_chunk(&chunk_str, &ctx.token_map);
            }

            if end_of_stream {
                output.push_str(&ctx.sse_rehydrator.flush(&ctx.token_map));
            }

            if !output.is_empty() {
                *body = Some(Bytes::from(output));
            }
        } else {
            // --- Buffered path (non-streaming responses) ---
            if let Some(chunk) = body.take() {
                if !append_chunk_with_limit(
                    &mut ctx.response_buffer,
                    &chunk,
                    MAX_RESPONSE_BUFFER_BYTES,
                ) {
                    return Err(Error::explain(
                        InternalError,
                        "upstream response exceeded buffer limit",
                    ));
                }
            }

            if end_of_stream {
                let text = String::from_utf8_lossy(&ctx.response_buffer);
                let rehydrated = match self.rehydrator.rehydrate_body(&text, &ctx.token_map) {
                    Ok(r) => r,
                    Err(error) => {
                        tracing::warn!(
                            session_id = %ctx.session_id,
                            %error,
                            "rehydrate_body failed (non-JSON upstream response?), \
                             falling back to text-level rehydration"
                        );
                        self.rehydrator.rehydrate_text(&text, &ctx.token_map)
                    }
                };
                *body = Some(Bytes::from(rehydrated));
                ctx.response_buffer.clear();
            }
        }

        Ok(None)
    }

    async fn logging(
        &self,
        _session: &mut Session,
        _e: Option<&pingora_core::Error>,
        ctx: &mut Self::CTX,
    ) {
        // Skip logging for requests that never reached upstream (e.g. CORS preflight, auth failures)
        if ctx.route.is_empty() || ctx.raw_api_key.is_empty() {
            return;
        }

        let latency_ms = ctx
            .request_start
            .map(|start| start.elapsed().as_millis() as u64)
            .unwrap_or(0);

        let payload = serde_json::json!({
            "session_id": ctx.session_id,
            "provider": ctx.provider,
            "model": ctx.model_hint,
            "pii_detected_count": ctx.pii_detected_count,
            "pii_types": ctx.pii_types,
            "route": ctx.route,
            "latency_ms": latency_ms,
            "masked_prompt": ctx.masked_prompt_snippet,
            "status_code": ctx.upstream_status_code,
        });

        let client = self.http_client.clone();
        let url = format!("{}/v1/logs", self.backend_api_url);
        let api_key = ctx.raw_api_key.clone();

        tokio::spawn(async move {
            let result = client
                .post(&url)
                .header("authorization", format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .json(&payload)
                .send()
                .await;
            match result {
                Ok(resp) if !resp.status().is_success() => {
                    tracing::warn!(
                        status = %resp.status(),
                        "failed to send request log to backend"
                    );
                }
                Err(error) => {
                    tracing::warn!(%error, "failed to send request log to backend");
                }
                _ => {}
            }
        });
    }
}

#[cfg(test)]
#[path = "proxy_tests.rs"]
mod tests;
