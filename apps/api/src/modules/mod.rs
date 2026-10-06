//! The domain features (plan §2's tree, §5.1).
//!
//! One directory per domain, and inside each the same five-part split, because §5.1 is a
//! claim about *responsibility* rather than about file names: a module that keeps its
//! rules, its storage and its orchestration in one file has not separated anything, it
//! has only moved the code.
//!
//! | Part | Answers |
//! | --- | --- |
//! | `dto` + `validators` | what shape is acceptable (§9.5 — validate at the boundary) |
//! | `error` | which taxonomy entry a refusal is (§4.2 — one taxonomy) |
//! | `repository` + `cache` | what exists |
//! | `service` | in what order things happen |
//! | `handlers` | how a request becomes a response — nothing else (§9.3) |
//!
//! # What has landed
//!
//! [`auth`] and [`broadcast`]. The rest of §2's tree — `profile`, `subscribers`,
//! `notifications`, `chat`, `notes`, `settings` — is declared as it is written rather
//! than stubbed, because a `pub mod` with an empty body is a module that compiles and
//! does nothing.
//!
//! [`broadcast`] landed second and shows the pattern at its clearest: a module that
//! needs an external dependency it may not have (LiveKit, §4.6) takes that as a
//! `state::Wiring` parameter and chooses an adapter, so `LIVEKIT_ENABLED=false` is a
//! deployment rather than a compile error.
//!
//! # The layer rule
//!
//! A module may reach down to `infrastructure`, `middleware`, `types` and `meno_core`.
//! Nothing above may reach into one, and `infrastructure` never imports a module: the
//! dependency arrow points one way, which is what lets the OAuth adapter in
//! `infrastructure/oauth` be swapped for a double without the auth module knowing.
//!
//! Each module is also bounded by its own `state.rs`, which is where its trait objects
//! are chosen (§5.6). That file is the *only* place concrete adapter types appear, so a
//! service or handler cannot accidentally depend on Postgres rather than on `AuthRepo`.

pub mod auth;
pub mod broadcast;
