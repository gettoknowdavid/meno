# `crates/db` — Meno database schema

This directory contains **migrations only**. It is deliberately not a Cargo package:
it holds no Rust code, so there is nothing for Cargo to compile or link, and an empty
crate would only add a phantom entry to `Cargo.lock` and a phantom dependency to
`apps/api`.

Its only artefact is `migrations/`, consumed by path:

- At compile time, `apps/api` embeds them with
  `sqlx::migrate!("../../crates/db/migrations")`. That path resolves relative to the
  **calling** crate, so it needs no crate here to anchor it.
- At the command line, `sqlx migrate run --source crates/db/migrations` locates them.
  The CLI resolves a directory, never a manifest.

On `master` these lived in `packages/db/` — a directory with no manifest that nothing
referenced, which is precisely why no migration runner was ever written. What was
missing was a *consumer*, not a package.

## Why there is no `Cargo.toml` here

The intent behind the original package boundary was sound: schema must stay separable
from application logic. A manifest does not enforce that — a Cargo package can hold
Rust code perfectly well. The rules below, and review, are what enforce it. Keeping an
empty `src/lib.rs` would have implied an enforcement that does not exist.

## Rules

1. No Rust. No `sqlx`, no `axum`, no I/O, no types. Anything that queries or models
   data belongs in `apps/api`.
2. Migrations are append-only. Never edit a shipped migration; add a new one.
   `_sqlx_migrations` records a checksum per version and will refuse to start
   otherwise.
3. `sqlx::migrate!()` resolves its path relative to the crate that calls it — so a
   rename of this directory is a breaking change in `apps/api/src/`.