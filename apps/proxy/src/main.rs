mod proxy;
mod masker;
mod rehydrator;
mod router;
mod state;

use std::env;

use anyhow::Result;
use pingora::proxy::http_proxy_service;
use pingora::server::Server;
use pingora::server::configuration::Opt;

use crate::masker::ner::NER;
use crate::proxy::MaskProxy;
use crate::router::Router;
use crate::state::redis::RedisState;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

pub struct ProxyConfig {
    pub port: u16,
    pub redis_url: String,
    pub api_backend_url: String,
    pub ner_model_path: String,
    pub embedding_model_path: String,
    pub lancedb_path: String,
    pub log_level: String,
}

impl ProxyConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            port: env::var("PROXY_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse::<u16>()?,

            redis_url: env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),

            // FastAPI backend — used when provider key is not in Redis cache
            api_backend_url: env::var("API_BACKEND_URL")
                .unwrap_or_else(|_| "http://localhost:8000".to_string()),

            ner_model_path: env::var("NER_MODEL_PATH")
                .expect("NER_MODEL_PATH is required (path to GLiNER ONNX file)"),

            embedding_model_path: env::var("EMBEDDING_MODEL_PATH")
                .expect("EMBEDDING_MODEL_PATH is required (path to BGE-Small ONNX file)"),

            lancedb_path: env::var("LANCEDB_PATH")
                .unwrap_or_else(|_| "./data/routing.lance".to_string()),

            log_level: env::var("LOG_LEVEL")
                .unwrap_or_else(|_| "INFO".to_string()),
        })
    }
}

// ---------------------------------------------------------------------------
// Provider inference (Option 1 — infer from model name, no extra header needed)
// ---------------------------------------------------------------------------

// Called by proxy.rs during upstream_request_filter to determine which
// upstream URL to forward to, and which provider key to look up in Redis.
pub fn infer_provider(model: &str) -> &'static str {
    if model.starts_with("gpt-") || model.starts_with("o1") || model.starts_with("o3") {
        "openai"
    } else if model.starts_with("claude-") {
        "anthropic"
    } else if model.starts_with("gemini-") {
        "gemini"
    } else {
        "openai" // default fallback
    }
}

// Maps a provider name to its base API URL.
// proxy.rs calls this to set the upstream host for Pingora.
pub fn provider_base_url(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "api.anthropic.com",
        "gemini"    => "generativelanguage.googleapis.com",
        _           => "api.openai.com", // openai + default
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    // 1. Load config from environment
    let config = ProxyConfig::from_env()
        .expect("Failed to load config from environment");

    // 2. Initialize structured logging
    tracing_subscriber::fmt()
        .with_env_filter(&config.log_level)
        .init();

    tracing::info!("Starting MaskProxy on port {}", config.port);

    // 3. Build async dependencies using a temporary Tokio runtime.
    //    We do this before handing control to Pingora, which manages
    //    its own internal runtime from server.run_forever() onwards.
    let rt = tokio::runtime::Runtime::new()
        .expect("Failed to create startup Tokio runtime");

    let redis = rt.block_on(async {
        RedisState::new(&config.redis_url)
            .await
            .expect("Failed to connect to Redis")
    });

    let router = rt.block_on(async {
        Router::new(&config.embedding_model_path, &config.lancedb_path)
            .await
            .expect("Failed to initialise router")
    });

    // Drop the startup runtime — Pingora takes over async from here
    drop(rt);

    // 4. Build synchronous dependencies (ONNX model load is synchronous)
    let ner = NER::new(&config.ner_model_path)
        .expect("Failed to load NER model");

    // 5. Construct the proxy handler
    let proxy = MaskProxy::new(redis, ner, router, config.api_backend_url);

    // 6. Set up the Pingora server and register the proxy service
    let opt = Opt::default();
    let mut server = Server::new(Some(opt))
        .expect("Failed to create Pingora server");
    server.bootstrap();

    let mut proxy_service = http_proxy_service(&server.configuration, proxy);
    proxy_service.add_tcp(&format!("0.0.0.0:{}", config.port));
    server.add_service(proxy_service);

    tracing::info!("MaskProxy listening on 0.0.0.0:{}", config.port);

    // 7. Blocks here — Pingora runs the proxy until the process exits
    server.run_forever();
}
