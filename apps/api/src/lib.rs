//! The Meno HTTP API and background worker.
//!
//! Both binaries — `src/main.rs` and `src/worker.rs` — are thin: they load configuration,
//! install telemetry, connect the infrastructure, and get out of the way. Everything testable
//! lives in the library, which is why it is a lib *and* two bins rather than two independent
//! crates.
//!
//! # Layout
//!
//! | Module | Holds |
//! | --- | --- |
//! | [`config`] | typed, validated configuration (§4.6, §7.8) |
//! | [`infrastructure`] | adapters — Redis, database, storage, OAuth, push, LiveKit, sockets |
//! | [`middleware`] | cross-cutting request concerns, and the single §4.2 error renderer |
//! | [`modules`] | the domain features; [`modules::auth`] is the one that has landed |
//! | [`types`] | wire types shared across domains; the success envelope |
//!
//! **Still to come**, per §2's tree: `bootstrap`, `routes`, `state` and `jobs`. Only what
//! exists today is declared here, so the crate compiles and its tests run — but note that
//! `bootstrap.rs` is what `main.rs` and `worker.rs` will share once `routes` lands, which
//! is why neither binary grows that wiring in the meantime.
//!
//! # The layer rule
//!
//! §4.1 keeps the per-domain layout, and the direction of every dependency in this crate is
//! the same: `middleware` and `types` depend on `infrastructure` and `crates/core`, never the
//! reverse. `infrastructure` knows about drivers (`sqlx`, `fred`, `reqwest`); nothing above it
//! does. A `fred::Error` reaching a handler is a §5.5 leak, and each adapter's `error.rs` is
//! where it is stopped.
//!
//! # Error handling
//!
//! There is one taxonomy — [`meno_core::Error`] — and one renderer for request-serving code:
//! `middleware::from_error` and `middleware::error_response`, both crate-private. Each builds a
//! [`meno_core::ErrorBody`], so a client sees one envelope for a body rejection, a throttle, a
//! validation failure and a domain error alike.

pub mod bootstrap;
pub mod config;
pub mod infrastructure;
pub mod middleware;
pub mod modules;
pub mod routes;
pub mod state;
pub mod types;
