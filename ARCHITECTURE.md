# ARCHITECTURE.md — System Map

## High-level stack

| Layer         | Tech                                                              |
|---------------|-------------------------------------------------------------------|
| Reverse proxy | Rust 2021 · Pingora 0.5 · Tokio · reqwest · rustls                |
| ML inference  | `ort` 2.0 (ONNX Runtime) · `tokenizers` 0.20 · DeBERTa/BERT NER   |
| Vector search | LanceDB 0.27 · Arrow 57 (semantic routing, Phase 2)               |
| API service   | Python 3.13 · FastAPI · SQLModel · Pydantic v2                    |
| Dashboard     | Next.js 16 (App Router) · React 19 · TanStack Query · Radix · Tailwind |
| Storage       | PostgreSQL 16 (config) · Redis 8 (session token maps, TTL 3600s)  |
| Orchestration | Docker Compose (5 services + 1 init container for model download) |

## System topology

```
                    ┌──────────────┐
        Browser ──► │  frontend    │ :3000  Next.js dashboard
                    │  (Next.js)   │
                    └──────┬───────┘
                           │  /api/*  (Next.js rewrite)
                           ▼
                    ┌──────────────┐
                    │  backend     │ :8000  FastAPI control plane
                    │  (FastAPI)   │◄──┐
                    └──────┬───────┘   │
                           │           │  Privacy config,
                  ┌────────┴────────┐  │  provider keys, API keys
                  ▼                 ▼  │
            ┌─────────┐       ┌─────────┐
            │   db    │       │  cache  │
            │ Postgres│       │  Redis  │
            └─────────┘       └────┬────┘
                                   │  Token map (PII placeholder ↔ original)
                                   │  Privacy config snapshot
                                   ▼
LLM client ──────────────► ┌──────────────┐ ──────► OpenAI / Anthropic / local LLM
                           │  proxy       │ :8080
                           │  (Pingora)   │
                           └──────────────┘
                            mask → forward → rehydrate
```

**Data flow on a request:**

1. Client sends an LLM API call to the proxy (`:8080`).
2. Proxy reads the API key header, fetches the caller's privacy config from Redis (warmed by backend).
3. Masker parses the JSON body, runs regex + NER over user-visible prompt text, replaces PII with `<<MASK:TYPE_N:MASK>>` placeholders, writes the mapping to Redis under a session-scoped key.
4. Router picks the upstream (cloud vs. local) based on keyword match or semantic similarity.
5. Sanitized request is forwarded; upstream LLM responds.
6. Rehydrator reads the mapping back from Redis and substitutes original values into the response body before returning to the client.

The client never sees masked content. The upstream LLM never sees PII.

## Folder taxonomy

```
MaskProxy/
├── apps/
│   ├── proxy/                    Rust reverse proxy (hot path)
│   │   ├── src/
│   │   │   ├── main.rs           Entry point, config, server bootstrap
│   │   │   ├── proxy.rs          ProxyHttp trait impl — orchestrates pipeline
│   │   │   ├── masker/           PII detection + placeholder generation
│   │   │   │   ├── mod.rs        Regex patterns, entity merging, placeholder format
│   │   │   │   └── ner.rs        ONNX Runtime NER inference
│   │   │   ├── rehydrator/       Response body placeholder → value restoration
│   │   │   ├── router/           Upstream selection (keyword + semantic)
│   │   │   │   ├── mod.rs        Routing decision logic
│   │   │   │   └── embedding.rs  BGE-Small embedding generation
│   │   │   └── state/            Shared state — Redis pool, LanceDB connection
│   │   │       ├── redis.rs      Token map storage, config cache
│   │   │       └── lancedb.rs    Vector store for semantic routing
│   │   └── models/               ONNX model artifacts (downloaded at startup)
│   │
│   ├── backend/                  Python control plane (not on request path)
│   │   └── app/
│   │       ├── main.py           FastAPI app, lifespan, CORS, router mounts
│   │       ├── models.py         SQLModel schemas
│   │       ├── database.py       Postgres connection
│   │       ├── cache.py          Redis async client
│   │       ├── auth.py           JWT + bcrypt
│   │       ├── security.py       API key issuance + SHA256 validation
│   │       ├── security_utils.py Crypto helpers (Fernet for provider keys)
│   │       └── routers/          REST endpoints — one file per resource
│   │
│   └── frontend/                 Next.js dashboard
│       └── src/
│           ├── app/              App Router pages (dashboard, keys, logs, settings, login, register)
│           ├── components/       Domain components + ui/ primitives
│           ├── hooks/            Custom React hooks
│           └── lib/              api.ts (HTTP client), auth.tsx (JWT mgmt)
│
├── packages/ai/                  Reserved for shared AI utilities (currently empty)
└── docker-compose.yml            5-service orchestration
```

## Why this shape

- **Proxy and backend are intentionally separate processes.** The proxy is on the request hot path and must stay lean and fast (Rust, no Python GIL, async I/O). The backend handles infrequent operator concerns (auth, config CRUD) where developer velocity matters more than latency.
- **Redis is the shared substrate.** The backend writes privacy config; the proxy reads it. The proxy writes token maps; the rehydrator reads them. No direct backend↔proxy RPC.
- **The frontend never talks to the proxy.** All UI traffic terminates at the backend. The proxy serves end-user LLM clients only.
- **Models live in a Docker volume** populated by the `model-downloader` init container, so application images stay small and model swaps don't require rebuilds.

## Upcoming surfaces (Phase 2+)

- **Semantic routing** (`router/embedding.rs` + `state/lancedb.rs`): BGE-Small embeddings + LanceDB cosine similarity for prompt-aware upstream selection.
- **SSE streaming rehydration**: placeholders can span chunk boundaries; requires a stateful buffer in the response filter.
- **OTEL tracing**: spans already exist in `tracing` calls; exporter wiring pending.
