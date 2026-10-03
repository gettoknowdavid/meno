//! Pure domain primitives shared across Meno services.
//!
//! **Hard constraint: no I/O.** No `sqlx`, no `axum`, no `fred`, no `tokio`, no `std::fs`,
//! no `std::net`. If a type here needs to touch the outside world it belongs in
//! `apps/api/src/infrastructure/`.
//!
//! The payoff: the cursor system has five distinct wire shapes and is the most
//! intricate logic in the codebase, and on `master` it was untestable without standing
//! up Postgres, Redis and an Axum router. Here it is a pure `#[test]`.

// TODO(§2.3, §4): uncomment as each module lands. `pagination` is copied from
// master in Step 2.3; `error`, `ids` and `time` are written in Step 4.
// pub mod error;
// pub mod ids;
// pub mod pagination;
// pub mod time;
//
// pub use error::{to_body, Error, ErrorBody, ErrorCode};
// pub use pagination::{Cursor, CursorPage, CursorParams, Order};