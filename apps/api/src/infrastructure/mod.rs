//! Everything that talks to the outside world.
//!
//! Each submodule is an **adapter**: it owns a third-party client and translates that
//! client's vocabulary into this codebase's (§5.6). The layering rule is one-directional
//! — domain code calls *traits* declared at the use site, and these adapters implement
//! them. Nothing in `modules/` may name `fred`, `sqlx` or `livekit_api` directly; a
//! `#[from]` on a driver error in a domain enum is the leak §5.5 is about.
//!
//! Driver types are legitimate *here* and only here. `infrastructure::database::db_err`
//! is where a `sqlx::Error` becomes a `meno_core::Error`, and
//! `infrastructure::redis` is where a `fred::error::Error` is logged and erased.

pub mod constants;
pub mod livekit;
pub mod redis;
pub mod signals;
pub mod telemetry;
pub mod ws;
