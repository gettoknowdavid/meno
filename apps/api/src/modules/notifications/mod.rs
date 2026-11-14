//! Notifications: a per-user inbox, the templates it is rendered from, and the
//! fan-out that fills it.
//!
//! # What this module is
//!
//! Every event the product wants a user to hear about — someone followed you, a
//! creator you follow went live, you were added as a co-host — becomes a row in
//! `notifications` for that user. The row points at a *template*
//! (`notification_templates`), which carries the sentence with placeholders; the
//! row carries the actor and the broadcast the sentence talks about. Reading the
//! inbox renders the placeholders, so a template can be reworded without rewriting
//! history.
//!
//! # Parts
//!
//! | File | Answers |
//! | --- | --- |
//! | [`model`] | what a template and a notification row *are*, and the type codes |
//! | [`dto`] | what the wire carries (camelCase, RFC 3339 — §9.5) |
//! | [`error`] | which taxonomy entry a refusal is (§4.2) |
//! | [`repository`] | what is stored — Postgres ([`repository::pg`]) and the §10 double ([`repository::memory`]) |
//! | [`publisher`] | how an in-app event reaches the recipient's sockets (§3.1) |
//! | [`counter`] | the Redis unread counter, and its in-memory double |
//! | [`service`] | the rules, in order |
//! | [`handlers`] | how a request becomes a response — nothing else (§9.3) |
//! | [`state`] | which adapters those trait objects are bound to (§5.6) |
//!
//! # What changed from `master`, and why
//!
//! `master`'s notifications module is the one this replaces
//! (`apps/api/src/modules/notifications/`). Four changes are structural rather than
//! cosmetic:
//!
//! 1. **The repository is a trait object, not a concrete pool owner.** `master`'s
//!    service took a `sqlx::PgPool` and built its own repository, so nothing in the
//!    module could be exercised without Postgres. [`state`] now chooses the adapter
//!    (§5.6) and the suites assemble the service from doubles instead.
//! 2. **Rendering happens once, on read, with every placeholder.** `master`
//!    substituted `{actor}` when listing and left `{title}` (the broadcast's title)
//!    in the sentence, because the title was never stored. The page query now joins
//!    `broadcasts` for it, so `"{actor} is live: {title}"` renders on the list path
//!    and in the push payload from the same variable map.
//! 3. **Push is awaited, not detached, on the single-recipient path.** `master`
//!    spawned the send, which made its outcome unobservable. Push is off the request
//!    path by design (§4.9) but it is also a fact this module owes a test; the
//!    fan-out path is still detached, because a thousand-device multicast must not
//!    hold a request open.
//! 4. **The errors are the shared taxonomy.** `master`'s `NotificationError` enum,
//!    its `IntoResponse`, and its `Redis`/`anyhow` variants are gone; a refusal is a
//!    named constructor over [`meno_core::Error`], and a missing template is a
//!    logged `500` rather than a bespoke payload.
//!
//! # The layer rule
//!
//! The module reaches down to `infrastructure` (push, Redis, the pub/sub bridge),
//! `middleware`, `types` and `meno_core`, and sideways nowhere. `notifications`,
//! `notification_templates`, `broadcasts`, `users` and `settings` are read through
//! this module's own SQL; it imports no other module's types.

pub mod counter;
pub mod dto;
pub mod error;
pub mod handlers;
pub mod model;
pub mod publisher;
pub mod repository;
pub mod service;
pub mod state;
