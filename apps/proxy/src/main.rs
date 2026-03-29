mod proxy;
mod masker;
mod rehydrator;
mod router;
mod state;

use std::env;

use anyhow::Result;
use clap::Parser;
use pingora_core::server::configuration::Opt;
use pingora_core::server::Server;
use pingora_proxy::http_proxy_service;
use rustls::crypto::ring::default_provider;

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
    pub cloud_upstream_base_url: String,
    pub local_upstream_base_url: Option<String>,
    pub routing_local_keywords: Vec<String>,
    pub routing_default_target: String,
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

            api_backend_url: env::var("API_BACKEND_URL")
                .unwrap_or_else(|_| "http://localhost:8000".to_string()),

            ner_model_path: env::var("NER_MODEL_PATH")
                .unwrap_or_default(),

            cloud_upstream_base_url: env::var("CLOUD_UPSTREAM_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com".to_string()),

            local_upstream_base_url: env::var("LOCAL_UPSTREAM_BASE_URL")
                .ok()
                .and_then(|value| {
                    let trimmed = value.trim().to_string();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(trimmed)
                    }
                }),

            routing_local_keywords: env::var("ROUTING_LOCAL_KEYWORDS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect(),

            routing_default_target: env::var("ROUTING_DEFAULT_TARGET")
                .unwrap_or_else(|_| "cloud".to_string()),

            log_level: env::var("LOG_LEVEL")
                .unwrap_or_else(|_| "INFO".to_string()),
        })
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    // 1. Load config from environment
    let config = ProxyConfig::from_env()?;

    // 2. Initialize structured logging
    tracing_subscriber::fmt()
        .with_env_filter(&config.log_level)
        .init();

    tracing::info!("Starting MaskProxy on port {}", config.port);

    // 3. Build async dependencies up front before handing control to Pingora.
    let rt = tokio::runtime::Runtime::new()
        .expect("Failed to create startup Tokio runtime");

    let redis = rt.block_on(async {
        RedisState::new(&config.redis_url)
            .await
            .expect("Failed to connect to Redis")
    });

    let router = rt.block_on(async {
        Router::new(
            &config.cloud_upstream_base_url,
            config.local_upstream_base_url.clone(),
            config.routing_local_keywords.clone(),
            if config.routing_default_target.eq_ignore_ascii_case("local") {
                crate::router::RouteTarget::Local
            } else {
                crate::router::RouteTarget::Cloud
            },
        )
            .await
            .expect("Failed to initialise router")
    });

    // Drop the startup runtime before Pingora takes over the process runtime.
    drop(rt);

    // 4. Build synchronous dependencies (ONNX model load is synchronous)
    let ner = NER::new(&config.ner_model_path)
        .expect("Failed to load NER model");

    // 5. Construct the proxy handler
    let proxy = MaskProxy::new(redis, ner, router);
    let proxy = proxy.with_backend_api_url(config.api_backend_url.clone());

    let opt = Opt::parse();
    let mut server = Server::new(Some(opt))?;
    server.bootstrap();

    let mut proxy_service = http_proxy_service(&server.configuration, proxy);
    proxy_service.add_tcp(&format!("0.0.0.0:{}", config.port));
    server.add_service(proxy_service);

    tracing::info!("MaskProxy listening on 0.0.0.0:{}", config.port);
    server.run_forever();
}
