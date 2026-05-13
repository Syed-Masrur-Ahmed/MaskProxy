# CLAUDE.md — Execution Protocol

This file governs how Claude operates inside the `MaskProxy/MaskProxy/` monorepo. The parent `../CLAUDE.md` describes *what* the project is; this file describes *how* to work in it.

## Bootstrap sequence (start of every session)

1. Read `ARCHITECTURE.md` for the system map.
2. Read `COMPONENTS.md` only for the domain you're touching (auth, masker, router, etc.).
3. Read `RULES.md` once per session — these are non-negotiable.
4. Run `/prime-context` if available; otherwise skim `git log --oneline -20`.

Do **not** load all four docs blindly on every turn. Use the index here as a router.

## Context economy

- **No full-workspace scans.** Never `cat` or `Read` an entire service end-to-end unless explicitly asked. Use `grep`, the Explore agent, or `claude-mem:smart-explore` for structural lookups.
- **Read only files relevant to the current atomic task.** A masker bug does not require reading frontend components. A UI tweak does not require reading `proxy.rs`.
- **Prefer the dedicated tool over Bash.** `Read` for files, `Edit` for edits, `grep` via Bash only when no dedicated tool fits.
- **Subagents for breadth.** If a question spans more than 3 files across services, delegate to Explore rather than reading inline.

## Commit rule

Before any `git commit` that changes architectural surface area (new service, new module under `apps/proxy/src/`, new router under `apps/backend/app/routers/`, new top-level frontend route), run `/sync-architecture` — or, if unavailable, manually update `ARCHITECTURE.md` and `COMPONENTS.md` in the same commit. Documentation drift is treated as a bug.

Routine bug fixes, refactors confined to one file, and dependency bumps do **not** trigger this rule.

## Service boundaries

- **`apps/proxy/`** — Rust, Pingora. Owns request interception, masking, rehydration, routing. Hot path; performance-sensitive.
- **`apps/backend/`** — Python, FastAPI. Owns user accounts, privacy config, provider keys, API key issuance. Not on the request hot path.
- **`apps/frontend/`** — Next.js. Owns the operator dashboard. Talks to the backend only — never to the proxy directly.

Cross-service changes (e.g., a new privacy config field) require touching all three. State that explicitly in the plan before editing.

## Testing expectations

- Rust changes under `masker/`, `rehydrator/`, `router/`: run `cargo test` for the relevant module before declaring done.
- Backend route changes: run `pytest` for the affected router.
- Frontend changes: at minimum run `npm run lint` and `npm run build` (build catches type errors).
- UI-visible changes: open the browser and verify the feature before reporting success. Type-checking is not feature-checking.

## What lives where

| Doc                | Purpose                                                    |
|--------------------|------------------------------------------------------------|
| `CLAUDE.md`        | This file. How to operate.                                 |
| `ARCHITECTURE.md`  | System topology, data flow, folder taxonomy.               |
| `COMPONENTS.md`    | Per-domain file registry (logic / UI / data boundaries).   |
| `RULES.md`         | Engineering philosophy and project-specific constraints.   |
| `README.md`        | Human-facing setup and usage.                              |
| `../CLAUDE.md`     | Project description (tech stack, request lifecycle, env).  |
