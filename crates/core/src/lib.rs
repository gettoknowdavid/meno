//! Pure domain primitives shared across Meno services.
//!
//! **Hard constraint: no I/O.** No `sqlx`, no `axum`, no `fred`, no `tokio`, no `std::fs`,
//! no `std::net`. If a type here needs to touch the outside world it belongs in
//! `apps/api/src/infrastructure/`.
//!
//! The payoff: the cursor system has five distinct wire shapes and is the most
//! intricate logic in the codebase, and on `master` it was untestable without standing
//! up Postgres, Redis and an Axum router. Here it is a pure `#[test]`.
//!
//! Every module in this crate is dependency-free *of the world*, and the whole crate
//! compiles in seconds with no infrastructure running. That is the property the
//! verification step exists to protect: if `cargo test -p meno-core` ever needs a
//! database, something leaked.

pub mod error;
pub mod ids;
pub mod pagination;
pub mod time;

pub use error::{Error, ErrorBody, ErrorCode, Meta, to_body};
pub use pagination::{Cursor, CursorPage, CursorParams, Order};
