//! The Meno HTTP API and background worker.
//!
//! **TEMPORARY module list.** Step 3.7 declares the full set — `bootstrap`, `config`,
//! `jobs`, `middleware`, `modules`, `routes`, `state` — once Step 2.3 has copied the
//! source tree in. Only the two modules that exist today are declared here, so the
//! crate compiles and its tests run.

pub mod infrastructure;
pub mod types;
