# RULES.md — Engineering Philosophy

Non-negotiable rules. Read once per session. When in doubt, choose the boring option.

## Digital minimalism

1. **No new dependencies without justification.** Before adding a crate / pip package / npm module, check if the standard library or an existing dependency can do it. Justification = "the alternative is >50 lines of tricky code I'd have to maintain."
2. **No frameworks for problems a function solves.** A regex utility does not need its own crate. A formatting helper does not need a UI library.
3. **Boring code beats clever code.** Imperative loops are fine. Early returns are fine. The reader's confusion budget is precious.
4. **No speculative abstraction.** Don't add generics, traits, or interfaces for a "future" use case that doesn't exist in a current ticket. Three duplicated lines beats a premature abstraction.
5. **Delete fearlessly.** Unused code is a liability. If you find a dead function and you're already in the file, remove it.

## Code hygiene

### Rust (`apps/proxy/`)
- Edition 2021. `cargo fmt` before commit. `cargo clippy` warnings are bugs.
- Use `tracing` (not `println!` or `log`) for diagnostics. `info!` for lifecycle, `debug!` for per-request detail, `warn!`/`error!` only for genuinely abnormal conditions.
- Errors propagate with `anyhow::Result` at boundaries; module-internal errors may use `thiserror`.
- No `unwrap()` or `expect()` on values derived from request data. Panics in the proxy take down all in-flight requests.
- Tests live in `mod.rs` siblings (`*_tests.rs`) and run as part of `cargo test`. New masker/rehydrator behavior requires a test.

### Python (`apps/backend/`)
- Python 3.13. Type hints on all public functions. `from __future__ import annotations` at the top of new files.
- FastAPI routes return Pydantic models, not dicts.
- SQLModel sessions are dependency-injected via FastAPI's `Depends`; never construct sessions inline.
- No print statements. Use `logging.getLogger(__name__)`.
- Tests in `tests/` mirror `app/` layout. New routes require at least one happy-path test.

### TypeScript (`apps/frontend/`)
- Strict mode on; no `any`. If you reach for `any`, the type is wrong somewhere — fix the source.
- Components: PascalCase filenames for exports (`ApiKeyManager.tsx`), kebab-case for non-component files (`use-config.ts`). Existing files use kebab-case across the board — match what's already there.
- TanStack Query for server state. No `useEffect` + `fetch`.
- Tailwind classes only — no inline `style={{}}` except for dynamic values that can't be expressed in classes.
- API types live in `src/lib/api.ts` and are the single source of truth for request/response shapes.

## Domain constraints

These are product-level invariants. Violations are bugs even if tests pass.

1. **PII never leaves the proxy unmasked.** The upstream LLM request body must not contain any value that the masker identified as PII. If you change the masker, write a test that asserts this on a realistic payload.
2. **The client never sees placeholders.** The rehydrator must replace every placeholder it issued. Leftover `<<MASK:...:MASK>>` strings in a response are a P0 bug.
3. **Token maps expire.** Redis keys for session token maps have a 3600s TTL. Do not introduce code paths that store PII without a TTL.
4. **Provider keys are never logged.** Not in tracing, not in error messages, not in response bodies. Fernet-encrypted at rest; decrypted only at use.
5. **Constant-time comparison for secrets.** API key validation, JWT signature checks, anything secret-shaped — use the language's constant-time helper (`hmac.compare_digest`, `subtle::ConstantTimeEq`).
6. **CORS is restrictive on purpose.** Do not broaden the allowed origins beyond `localhost:3000` without explicit operator action.
7. **No request-body content in logs by default.** Tracing may include lengths, types, and counts. Full bodies only at `DEBUG` level and only behind an explicit opt-in env var.
8. **Streaming responses are not yet rehydration-safe.** Until Phase 3 ships, do not advertise streaming as supported. A placeholder spanning two SSE chunks will leak.

## Cross-cutting

- **`.env` is never committed.** `.env.example` documents the keys.
- **Migrations are forward-only.** No `DROP COLUMN` without a deprecation window.
- **The proxy is the contract.** If the backend changes a config field name, the proxy must be updated in the same commit — there is no compatibility layer.
- **One PR, one concern.** A masker feature and a UI tweak go in separate PRs even if they're "related."
