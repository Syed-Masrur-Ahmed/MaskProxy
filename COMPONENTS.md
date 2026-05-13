# COMPONENTS.md — Interaction Registry

A per-domain map of which files own logic, UI, and data. Use this to scope reads — do not load files outside the domain you're editing.

---

## 1. Privacy Middleware (masking + rehydration)

The product's core. Owns the transformation pipeline that sits between client and upstream LLM.

| Concern              | File                                            |
|----------------------|-------------------------------------------------|
| Pipeline orchestration | `apps/proxy/src/proxy.rs`                     |
| Regex PII detection  | `apps/proxy/src/masker/mod.rs` (EMAIL, PHONE, SSN, CREDIT_CARD) |
| NER PII detection    | `apps/proxy/src/masker/ner.rs` (PERSON_NAME, LOCATION, ORGANIZATION) |
| Per-tenant config    | `apps/proxy/src/masker/mod.rs` (`PrivacyConfig`, `entity_enabled`) |
| Config fetch         | `apps/proxy/src/proxy.rs` (`load_privacy_config`) |
| Placeholder format   | `apps/proxy/src/masker/mod.rs` (`<<MASK:TYPE_N:MASK>>`) |
| Token map storage    | `apps/proxy/src/state/redis.rs`                 |
| Response rewriting   | `apps/proxy/src/rehydrator/mod.rs`              |
| Tests                | `apps/proxy/src/masker/`, `rehydrator/tests.rs`, `proxy_tests.rs` |

**Interface:** Pingora's `ProxyHttp` trait. `upstream_request_filter` masks; `response_filter` rehydrates. Communication between the two phases is via a session-scoped Redis key.

**Entity merging rule:** longest span wins; on tie, regex beats NER.

**Per-request configuration:** after resolving the caller's `user_id` from the API key, the proxy fetches `privacy_config:<user_id>` from Redis and gates kinds via `entity_enabled` before placeholder substitution. NER confidence threshold is taken from the same config. `mask_finance` controls both `SSN` and `CREDIT_CARD`; `EMAIL` and `PHONE` are always on.

---

## 2. Routing

Decides whether a request goes to a cloud LLM or a local one.

| Concern              | File                                            |
|----------------------|-------------------------------------------------|
| Routing decision     | `apps/proxy/src/router/mod.rs`                  |
| Embedding generation | `apps/proxy/src/router/embedding.rs`            |
| Vector store         | `apps/proxy/src/state/lancedb.rs`               |
| `upstream_peer` hook | `apps/proxy/src/proxy.rs`                       |
| Tests                | `apps/proxy/src/router/tests.rs`, `state/lancedb_tests.rs` |

**Current:** keyword match (`ROUTING_LOCAL_KEYWORDS` env var) with a default fallback (`ROUTING_DEFAULT_TARGET`).
**Planned (Phase 2):** semantic similarity against a LanceDB-indexed corpus.

---

## 3. Authentication & Identity

Operator login for the dashboard. Not invoked on the LLM request path.

| Concern              | File                                            |
|----------------------|-------------------------------------------------|
| User schema          | `apps/backend/app/models.py` (`User`)           |
| Password hashing     | `apps/backend/app/auth.py` (bcrypt)             |
| JWT issuance         | `apps/backend/app/auth.py` (python-jose)        |
| Login / register     | `apps/backend/app/routers/auth.py`              |
| User CRUD            | `apps/backend/app/routers/users.py`             |
| Frontend token store | `apps/frontend/src/lib/auth.tsx`                |
| Login page           | `apps/frontend/src/app/login/page.tsx`          |
| Register page        | `apps/frontend/src/app/register/page.tsx`       |

**Interface:** JWT bearer in `Authorization` header. Token in `localStorage` (frontend).

---

## 4. API Key Management

Customer-facing `mp_*` keys that LLM clients send to the proxy.

| Concern              | File                                            |
|----------------------|-------------------------------------------------|
| Key generation       | `apps/backend/app/security.py`                  |
| Crypto helpers       | `apps/backend/app/security_utils.py`            |
| Key schema           | `apps/backend/app/models.py` (`APIKey`)         |
| Endpoints            | `apps/backend/app/routers/api_keys.py`          |
| UI                   | `apps/frontend/src/components/api-key-manager.tsx` |
| Page                 | `apps/frontend/src/app/keys/page.tsx`           |

**Storage:** SHA256 hash only; plaintext shown once at creation. Constant-time comparison on validation.

---

## 5. Provider Key Vault

Upstream LLM provider API keys (OpenAI, Anthropic, etc.) the proxy uses to call out.

| Concern              | File                                            |
|----------------------|-------------------------------------------------|
| Encryption (Fernet)  | `apps/backend/app/security_utils.py`            |
| Schema               | `apps/backend/app/models.py` (`ProviderKey`)    |
| Endpoints            | `apps/backend/app/routers/provider_keys.py`     |

**Storage:** Fernet-encrypted at rest with `MASTER_ENCRYPTION_KEY`. Decrypted only when the proxy fetches a key for outbound calls.

---

## 6. Privacy Configuration

Per-tenant rules for which PII categories to mask.

| Concern              | File                                            |
|----------------------|-------------------------------------------------|
| Schema               | `apps/backend/app/models.py` (`PrivacyConfig`, `PrivacyConfigUpdate`) |
| Idempotent column adds | `apps/backend/app/database.py` (`ALTER TABLE ... ADD COLUMN IF NOT EXISTS`) |
| REST endpoints       | `apps/backend/app/routers/config.py`            |
| Redis cache (writer) | `apps/backend/app/cache.py`                     |
| Proxy reader         | `apps/proxy/src/proxy.rs::load_privacy_config`  |
| Proxy consumer       | `apps/proxy/src/masker/mod.rs` (`PrivacyConfig`, `entity_enabled`) |
| API type             | `apps/frontend/src/lib/api.ts` (`PrivacyConfig`) |
| UI                   | `apps/frontend/src/app/settings/pii-settings.tsx` |
| Page                 | `apps/frontend/src/app/settings/page.tsx` (tab: PII Masking) |

**Fields:** `mask_names`, `mask_locations`, `mask_organizations`, `mask_finance` (SSN + credit cards), `threshold` (NER confidence cutoff, 0.0–1.0). All booleans default to `true`; threshold defaults to `0.75`.

**Interface:** Write-through cache. Backend writes Postgres → Redis (`privacy_config:<user_id>`). Proxy reads Redis only, fallback to defaults on miss or parse error.

**Schema migration:** `database.py` runs an idempotent `ALTER TABLE ... ADD COLUMN IF NOT EXISTS` on startup for fields introduced after `SQLModel.metadata.create_all` first built the table. Used in lieu of Alembic; safe to re-run.

---

## 7. Live Request Logs

Operator visibility into proxied requests and what was masked.

| Concern              | File                                            |
|----------------------|-------------------------------------------------|
| Log endpoints        | `apps/backend/app/routers/logs.py`              |
| Live log component   | `apps/frontend/src/components/live-request-logs.tsx` |
| Table view           | `apps/frontend/src/components/logs-table.tsx`   |
| Masked prompt render | `apps/frontend/src/components/masked-prompt.tsx`|
| Logs page            | `apps/frontend/src/app/logs/page.tsx`           |

---

## 8. Dashboard Shell

Cross-cutting UI scaffolding.

| Concern              | File                                            |
|----------------------|-------------------------------------------------|
| Root layout          | `apps/frontend/src/app/layout.tsx`              |
| App shell            | `apps/frontend/src/components/app-shell.tsx`    |
| Sidebar              | `apps/frontend/src/components/sidebar.tsx`      |
| Stats bar            | `apps/frontend/src/components/stats-bar.tsx`    |
| Radix primitives     | `apps/frontend/src/components/ui/`              |
| HTTP client          | `apps/frontend/src/lib/api.ts`                  |

---

## Interface patterns

| Boundary                     | Protocol / Format                              |
|------------------------------|------------------------------------------------|
| Client ↔ Proxy               | HTTP/JSON (LLM provider-compatible payloads)   |
| Proxy ↔ Upstream LLM         | HTTP/JSON via `reqwest`                        |
| Frontend ↔ Backend           | REST/JSON via Next.js `/api/*` rewrite         |
| Backend ↔ Postgres           | SQLModel (SQLAlchemy under the hood)           |
| Backend ↔ Redis              | `redis-py` async                               |
| Proxy ↔ Redis                | `redis` crate, connection-manager pool         |
| Auth (operator)              | JWT bearer in `Authorization` header           |
| Auth (LLM client)            | `mp_*` API key, SHA256-validated server-side   |
| Token map key                | Redis hash, session-scoped UUID, TTL 3600s     |
| PII placeholder              | `<<MASK:TYPE_N:MASK>>` (e.g. `<<MASK:EMAIL_1:MASK>>`) |
