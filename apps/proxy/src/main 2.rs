mod masker;
mod proxy;
mod rehydrator;
mod router;
mod state;

use std::env;
use std::net::IpAddr;
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, ensure, Context, Result};
use clap::Parser;
use pingora_core::server::configuration::Opt;
use pingora_core::server::Server;
use pingora_proxy::http_proxy_service;
use rustls::crypto::ring::default_provider;
use url::Url;

use crate::masker::ner::NER;
use crate::proxy::MaskProxy;
use crate::router::{
    load_route_examples, EmbeddingProvider, OnnxTextEmbeddingProvider, RouteTarget, Router,
};
use crate::state::lancedb::{LanceDbState, RouteExampleRow};
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
    pub routing_enabled: bool,
    pub routing_strategy: String,
    pub routing_local_keywords: Vec<String>,
    pub routing_default_target: String,
    pub routing_embedding_model_path: String,
    pub routing_embedding_tokenizer_path: String,
    pub routing_examples_path: String,
    pub routing_lancedb_path: String,
    pub routing_lancedb_table_name: String,
    pub routing_similarity_threshold: f32,
    pub routing_top_k: usize,
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
                .unwrap_or_else(|_| default_proxy_path("models/optimum-bert-base-NER/model.onnx")),

            cloud_upstream_base_url: env::var("CLOUD_UPSTREAM_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com".to_string()),

            local_upstream_base_url: env::var("LOCAL_UPSTREAM_BASE_URL").ok().and_then(|value| {
                let trimmed = value.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            }),

            routing_enabled: env_flag("ROUTING_ENABLED", false),

            routing_strategy: env::var("ROUTING_STRATEGY")
                .unwrap_or_else(|_| "keyword".to_string()),

            routing_local_keywords: env::var("ROUTING_LOCAL_KEYWORDS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect(),

            routing_default_target: env::var("ROUTING_DEFAULT_TARGET")
                .unwrap_or_else(|_| "cloud".to_string()),

            routing_embedding_model_path: env::var("ROUTING_EMBEDDING_MODEL_PATH")
                .unwrap_or_else(|_| default_proxy_path("models/optimum-all-MiniLM-L6-v2/model.onnx")),

            routing_embedding_tokenizer_path: env::var("ROUTING_EMBEDDING_TOKENIZER_PATH")
                .unwrap_or_else(|_| {
                    default_proxy_path("models/optimum-all-MiniLM-L6-v2/tokenizer.json")
                }),

            routing_examples_path: env::var("ROUTING_EXAMPLES_PATH")
                .unwrap_or_else(|_| default_proxy_path("models/optimum-all-MiniLM-L6-v2/routes.json")),

            routing_lancedb_path: env::var("ROUTING_LANCEDB_PATH")
                .unwrap_or_else(|_| default_proxy_path("data/semantic-routing.lancedb")),

            routing_lancedb_table_name: env::var("ROUTING_LANCEDB_TABLE_NAME")
                .unwrap_or_else(|_| "route_examples".to_string()),

            routing_similarity_threshold: env::var("ROUTING_SIMILARITY_THRESHOLD")
                .unwrap_or_else(|_| "0.8".to_string())
                .parse::<f32>()?,

            routing_top_k: env::var("ROUTING_TOP_K")
                .unwrap_or_else(|_| "3".to_string())
                .parse::<usize>()?,

            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "INFO".to_string()),
        })
    }
}

fn env_flag(name: &str, default: bool) -> bool {
    match env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
}

fn proxy_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn default_proxy_path(relative: &str) -> String {
    proxy_root().join(relative).display().to_string()
}

fn route_target_from_env(value: &str) -> RouteTarget {
    if value.eq_ignore_ascii_case("local") {
        RouteTarget::Local
    } else {
        RouteTarget::Cloud
    }
}

fn semantic_routing_enabled(config: &ProxyConfig) -> bool {
    config.routing_enabled && config.routing_strategy.eq_ignore_ascii_case("semantic")
}

fn ensure_semantic_routing_paths_exist(
    model_path: &Path,
    tokenizer_path: &Path,
    examples_path: &Path,
) -> Result<()> {
    ensure!(
        model_path.is_file(),
        "semantic routing model file not found: {}",
        model_path.display()
    );
    ensure!(
        tokenizer_path.is_file(),
        "semantic routing tokenizer file not found: {}",
        tokenizer_path.display()
    );
    ensure!(
        examples_path.is_file(),
        "semantic routing examples file not found: {}",
        examples_path.display()
    );
    Ok(())
}

fn is_private_or_loopback_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => ipv4.is_private() || ipv4.is_loopback(),
        IpAddr::V6(ipv6) => ipv6.is_loopback() || ipv6.is_unique_local(),
    }
}

fn validate_local_upstream_base_url(local_upstream: Option<&str>) -> Result<()> {
    let Some(local_upstream) = local_upstream else {
        return Ok(());
    };

    let parsed = Url::parse(local_upstream)
        .with_context(|| format!("LOCAL_UPSTREAM_BASE_URL is not a valid URL: {local_upstream}"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("LOCAL_UPSTREAM_BASE_URL is missing a host: {local_upstream}"))?;

    if host.eq_ignore_ascii_case("localhost") {
        return Ok(());
    }

    let port = parsed.port_or_known_default().ok_or_else(|| {
        anyhow!("LOCAL_UPSTREAM_BASE_URL must include a port or a known URL scheme")
    })?;
    let resolved_addrs: Vec<_> = (host, port)
        .to_socket_addrs()
        .with_context(|| {
            format!("LOCAL_UPSTREAM_BASE_URL host could not be resolved: {local_upstream}")
        })?
        .collect();

    ensure!(
        !resolved_addrs.is_empty(),
        "LOCAL_UPSTREAM_BASE_URL did not resolve to any addresses: {local_upstream}"
    );
    ensure!(
        resolved_addrs
            .iter()
            .all(|addr| is_private_or_loopback_ip(addr.ip())),
        "LOCAL_UPSTREAM_BASE_URL must resolve only to private or loopback addresses: {local_upstream} -> {:?}",
        resolved_addrs
    );

    Ok(())
}

async fn ensure_ner_health(ner: &NER) -> Result<()> {
    if ner.is_disabled() {
        tracing::warn!("NER is disabled; only regex-based masking will run");
        return Ok(());
    }

    let smoke_text = "John Smith can be reached by the care team.";
    let entities = ner.detect_entities(smoke_text).await?;
    ensure!(
        !entities.is_empty(),
        "NER startup health check failed: smoke test returned no entities"
    );

    tracing::info!(
        entities = entities.len(),
        "NER startup health check passed"
    );

    Ok(())
}

async fn build_router(config: &ProxyConfig) -> Result<Router> {
    let default_target = route_target_from_env(&config.routing_default_target);

    if !semantic_routing_enabled(config) {
        tracing::info!("Routing mode: keyword");
        return Router::new(
            &config.cloud_upstream_base_url,
            config.local_upstream_base_url.clone(),
            config.routing_local_keywords.clone(),
            default_target,
        )
        .await;
    }

    ensure!(
        (0.0..=1.0).contains(&config.routing_similarity_threshold),
        "ROUTING_SIMILARITY_THRESHOLD must be between 0.0 and 1.0"
    );
    ensure!(
        config.routing_top_k > 0,
        "ROUTING_TOP_K must be greater than zero when semantic routing is enabled"
    );

    let model_path = Path::new(&config.routing_embedding_model_path);
    let tokenizer_path = Path::new(&config.routing_embedding_tokenizer_path);
    let examples_path = Path::new(&config.routing_examples_path);
    ensure_semantic_routing_paths_exist(model_path, tokenizer_path, examples_path)?;

    let embedding_provider = Arc::new(OnnxTextEmbeddingProvider::new(model_path, tokenizer_path)?);
    let route_examples = load_route_examples(examples_path)?;

    let mut example_rows = Vec::with_capacity(route_examples.len());
    let mut vector_dim: Option<i32> = None;

    for example in route_examples {
        let vector = embedding_provider.embed(&example.text)?;
        ensure!(
            !vector.is_empty(),
            "route example {:?} produced an empty embedding",
            example.text
        );

        let current_dim = i32::try_from(vector.len())
            .map_err(|_| anyhow!("embedding dimension {} exceeds i32", vector.len()))?;
        match vector_dim {
            Some(expected_dim) => ensure!(
                expected_dim == current_dim,
                "route example {:?} produced embedding dimension {}, expected {}",
                example.text,
                current_dim,
                expected_dim
            ),
            None => vector_dim = Some(current_dim),
        }

        example_rows.push(RouteExampleRow {
            text: example.text,
            target: example.target,
            vector,
        });
    }

    let vector_dim = vector_dim.ok_or_else(|| anyhow!("no valid semantic route examples loaded"))?;

    std::fs::create_dir_all(&config.routing_lancedb_path)?;
    let mut route_store = LanceDbState::new(
        &config.routing_lancedb_path,
        &config.routing_lancedb_table_name,
        vector_dim,
    )
    .await?;
    route_store.rebuild_from_examples(&example_rows).await?;

    tracing::info!(
        examples = example_rows.len(),
        vector_dim,
        threshold = config.routing_similarity_threshold,
        top_k = config.routing_top_k,
        lancedb_path = %config.routing_lancedb_path,
        table_name = %config.routing_lancedb_table_name,
        "Routing mode: semantic"
    );

    Ok(Router::with_semantic(
        &config.cloud_upstream_base_url,
        config.local_upstream_base_url.clone(),
        embedding_provider,
        Arc::new(route_store),
        config.routing_similarity_threshold,
        default_target,
        config.routing_top_k,
    ))
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

    validate_local_upstream_base_url(config.local_upstream_base_url.as_deref())?;

    tracing::info!("Starting MaskProxy on port {}", config.port);

    // 3. Build async dependencies up front before handing control to Pingora.
    let rt = tokio::runtime::Runtime::new().expect("Failed to create startup Tokio runtime");

    let redis = rt.block_on(async {
        RedisState::new(&config.redis_url)
            .await
            .expect("Failed to connect to Redis")
    });

    let ner = NER::new(&config.ner_model_path).expect("Failed to load NER model");
    rt.block_on(async { ensure_ner_health(&ner).await })
        .expect("NER health check failed");

    let router = rt
        .block_on(async { build_router(&config).await })
        .expect("Failed to initialise router");

    // Drop the startup runtime before Pingora takes over the process runtime.
    drop(rt);

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

#[cfg(test)]
#[path = "main_tests.rs"]
mod tests;
