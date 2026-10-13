//! Broadcast chat: a message history, reactions, and the socket events that carry them.
//!
//! # What this module is
//!
//! The conversation inside a live broadcast. A participant sends a message or a
//! reaction over the socket they are already holding; everybody in the room hears
//! about it on the same socket; and the history is readable over one HTTP endpoint so a
//! client that just joined does not have to replay the room to see what was said.
//!
//! # Parts
//!
//! | File | Answers |
//! | --- | --- |
//! | [`model`] | what a message and a reaction *are*, and the edit window that governs them |
//! | [`dto`] | what the wire carries (camelCase, RFC 3339 — §9.5) |
//! | [`error`] | which taxonomy entry a refusal is (§4.2) |
//! | [`repository`] | what is stored — Postgres ([`repository::pg`]) and the §10 double ([`repository::memory`]) |
//! | [`publisher`] | how a stored event reaches a room's sockets (§3.1) |
//! | [`service`] | the rules, in order |
//! | [`handlers`] | the one HTTP read; the writes arrive on the socket ([`crate::routes::ws`]) |
//! | [`state`] | which adapters those trait objects are bound to (§5.6) |
//!
//! # What changed from `master`, and why
//!
//! `master`'s chat module is the one this replaces
//! (`apps/api/src/modules/chat/`, and `apps/api/src/routes/chat.rs`). Four changes are
//! structural rather than cosmetic, and each is a bug the old shape made possible:
//!
//! 1. **The write guards are inside the statements.** `master` asked
//!    `is_active_broadcast`, then `is_broadcast_participant`, then inserted. Between
//!    the second answer and the insert a broadcast could end or a participant be
//!    removed, and the message landed anyway — a row in a room nobody is in.
//!    [`repository::ChatRepo::insert_message`] is one `INSERT … SELECT … WHERE EXISTS`
//!    that writes nothing unless both conditions hold *at the moment of the write*.
//!    The guards are still asked, but only to explain a refusal.
//! 2. **The Redis page cache is gone.** It cached a page under a keyset built from the
//!    request and invalidated it by `SCAN`-ing the whole keyspace after every write —
//!    a full scan per message, for a cache whose asynchronous fill could race its own
//!    invalidation and leave a stale page for the whole TTL. The read underneath it is
//!    one keyset query on the index migration 0010 already provides; the catalogue in
//!    [`crate::modules::broadcast`] made the same call for the same reason.
//! 3. **The module returns its own types, not responses.** `master`'s repository built
//!    `ChatMessageResponse` directly, so the storage layer decided the wire shape. Here
//!    the repository returns [`model::ChatMessage`] and the service builds the DTO once.
//! 4. **A message is edited in the broadcast it was written in.** `master` never
//!    compared the message's `broadcast_id` to the one in the request, so a member of
//!    one room could edit their own message *in another room's name* — applied, and then
//!    announced to a room that had never seen it. [`service::ChatService::edit_message`]
//!    refuses that with the same 404 as a message that does not exist.
//!
//! # The layer rule still holds
//!
//! The module does not import [`crate::modules::broadcast`]: it reads `broadcasts` and
//! `broadcast_participants` through its own SQL, and names its own
//! [`model::SenderSummary`] rather than a broadcast-side user projection. Reaching
//! sideways into another module would make this one's contract depend on that one's
//! internals; a stored procedure would be the same coupling with worse tooling.

pub mod dto;
pub mod error;
pub mod handlers;
pub mod model;
pub mod publisher;
pub mod repository;
pub mod service;
pub mod state;
