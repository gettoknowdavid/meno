//! The Meno HTTP API and background worker.
//!
//! Both binaries — `src/main.rs` and `src/worker.rs` — are thin: they load
//! configuration, install telemetry, connect the infrastructure, and get out of the
//! way. Everything testable lives in the library, which is why it is a lib *and* two
//! bins rather than two independent crates.
//!
//! **Module list is still partial.** The guide declares `bootstrap`, `jobs`,
//! `middleware`, `modules`, `routes` and `state`; those arrive as the refactor lands.
//! Only what exists today is declared here, so the crate compiles and its tests run.

pub mod config;
pub mod infrastructure;
pub mod types;
