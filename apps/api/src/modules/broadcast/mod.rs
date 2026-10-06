//! Broadcasts: creating, scheduling, going live, joining, and the room behind them.
//!
//! # The shape of the module
//!
//! The same five layers as [`crate::modules::auth`], so a rule has exactly one home:
//!
//! | File | Question | Change it when |
//! | --- | --- | --- |
//! | [`dto`] | is this shape acceptable? | a field or a rule changes |
//! | [`error`] | which taxonomy entry is this refusal? | a client needs to branch on it |
//! | [`model`] | what do the rows say? | the schema changes |
//! | [`repository`] | what exists? | storage does |
//! | [`media`] | what does the media server allow? | the vendor does |
//! | [`service`] | in what order? | a *flow* changes |
//! | [`state`] | which adapter is real here? | wiring changes |
//! | [`handlers`] | how does a request become a response? | the wire contract changes |
//!
//! # The invariants this module holds
//!
//! Each is asserted somewhere in the tests, and each has one enforcement point:
//!
//! - **Only the creator starts, ends, edits or deletes a broadcast.** Enforced in
//!   [`service::BroadcastService`] through one helper, so there is one place to read
//!   rather than four comparisons to audit.
//! - **A live broadcast is immutable.** The same layer, so an endpoint cannot opt out by
//!   forgetting to check.
//! - **§7.6 — `total_participants` moves with the rows.** One `join` call writes the row
//!   *and* the counter, in one statement; see [`repository::pg`].
//! - **The creator cannot join, and a host cannot leave.** Joining would downgrade the
//!   host grant; leaving would leave a live broadcast nobody can end.
//! - **§4.5 — every date on the wire is RFC 3339.** Stated in [`dto`] and enforced for
//!   this whole file by `apps/api/tests/wire_dates.rs`.
//!
//! # What this port does *not* include
//!
//! The module arrived from `master` as ten files that referenced a tree this workspace
//! does not have (`crate::shared::*`, a `jobs` module, Redis cache helpers, a
//! `UserSummary` DTO). Porting all of it faithfully would have meant porting those too,
//! so the scope is the part that is *complete and testable*, and the rest is named here
//! rather than left as a half-wired file that compiles and does nothing — the failure
//! mode §2's `pub mod` note warns about.
//!
//! **Deferred, with the reason:**
//!
//! - **Redis list caching.** The feed is one indexed keyset query today, which §9.4's
//!   budget covers. The cache belongs with the *scheduled-start* job below, because
//!   both are about invalidation.
//! - **Scheduled start and recording finalisation (plan §6).** These are jobs, and
//!   §6.4 puts them in the dedicated worker binary. Wiring them into an HTTP handler
//!   would make a request responsible for ending another request's broadcast.
//! - **Cohost *invitations* (`cohost_invitations`, migration 0008).** Add/remove works;
//!   the invite-and-accept handshake is a separate flow with its own table, and adding
//!   it half-way would leave a cohost list with two ways to change it.
//! - **Bookmarks and subscriptions.** `is_bookmarked` is reported as `false` because the
//!   module does not read `broadcast_bookmarks` yet; it is a wire field the client
//!   already renders, and filling it wrongly would be worse than leaving it.
//! - **WebSocket fan-out of broadcast events.** [`crate::infrastructure::ws`] can carry
//!   them; deciding which events matter to which sockets is a protocol decision, not a
//!   wiring one.
//!
//! Every one of these is a *missing feature*, not a missing test: nothing in this module
//! pretends to do them.

pub mod dto;
pub mod error;
pub mod handlers;
pub mod media;
pub mod model;
pub mod repository;
pub mod service;
pub mod state;

// The module's contract tests live in `apps/api/tests/broadcast_service.rs` and
// `apps/api/tests/broadcast_router.rs`, outside the crate, where every path starts at
// `meno_api::` and the fixtures are shared through `tests/support/mod.rs`.
