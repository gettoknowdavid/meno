//! Subscribers: who follows whom, and the two lists that read it.
//!
//! # What this module is
//!
//! One join table — `user_subscribers` — and the three things a client does with it:
//! follow somebody, stop following them, and read a page of a user's followers or of the
//! people they follow. Every item in either list carries `isFollowing`, which is *the
//! viewer's* relationship to that row's user; a list of followers is therefore a
//! personalised answer, one `EXISTS` per row rather than a public projection.
//!
//! # Parts
//!
//! | File | Answers |
//! | --- | --- |
//! | [`dto`] | what the wire carries (camelCase, RFC 3339 — §9.5), and which of the two collections a request is |
//! | [`error`] | which taxonomy entry a refusal is (§4.2) |
//! | [`repository`] | what is stored — Postgres ([`repository::pg`]) and the §10 double ([`repository::memory`]) |
//! | [`service`] | the rules, in order |
//! | [`handlers`] | how a request becomes a response — nothing else (§9.3) |
//! | [`state`] | which adapter the trait object is bound to (§5.6) |
//!
//! There is no `model.rs`. `profile`'s `ProfileSearchResult` set the precedent for a
//! *list row* that is also its own wire type, and this module's rows are the same kind of
//! thing: a user's public fields plus the viewer-relative flag. Nothing on the row is
//! private — `email` is never selected — so a second struct would be a conversion that
//! copies three strings to hide nothing.
//!
//! # What changed from `master`, and why
//!
//! `master`'s subscribers module is the one this replaces (`apps/api/src/modules/subscribers/`
//! and its route file, which is now `apps/api/src/routes/subscribers.rs`). The endpoints are
//! the same; the shape underneath is not.
//!
//! 1. **The `is_following` annotation is one query, not two.** `master` selected a literal
//!    `false AS is_following`, then ran a second statement (`batch_is_subscribed`) over the
//!    page's ids and mutated every row in Rust. The `EXISTS` now sits in the page query, so
//!    a list is one round trip and the per-row fix-up loop is gone. `profile` solved the
//!    same problem the same way.
//! 2. **Two page statements became one, and four service methods became one.**
//!    `find_subscribers` and `find_subscriptions` were the same SQL with two column names
//!    swapped; [`dto::FollowDirection`] now selects that pair. `get_my_subscribers` and
//!    `get_user_subscribers` differed only in whether the owner was the caller, so the
//!    service has one [`service::SubscribersService::page`] and the handler passes both
//!    ids — including the case where they are equal.
//! 3. **A write reports what it changed, so the common path is one statement.** Both
//!    `create` and `remove` answer *whether a row was inserted or removed*; the service
//!    asks [`repository::SubscribersRepo::user_exists`] only when nothing changed, which is
//!    the only case where the 404 and the idempotent success need telling apart. `master`
//!    looked the user up first, on every call, through a second port.
//! 4. **`IdentityReader` is gone.** A narrow read-only trait is the right instinct (§5.4)
//!    and the plan holds it up as the example — but the only thing subscribers read from
//!    it was *whether a user exists*, which is one `EXISTS` against a table this module
//!    already knows. Asking the repository removes the cross-module edge entirely, and
//!    with it the second pool-holder in `state`'s wiring.
//! 5. **The errors are the shared taxonomy.** `master`'s seven-variant `SubscribersError`
//!    and its hand-written `IntoResponse` are replaced by three named constructors over
//!    [`meno_core::Error`]. `SubscriptionNotFound` was never returned by anything and is
//!    not replaced.
//! 6. **The wire is camelCase with RFC 3339 dates.** `master` emitted `full_name`,
//!    `avatar_url`, `is_following` and `subscribed_at`, so a Dart client needed a second
//!    naming rule for this one feature. `SubscriberItem` and the unused `SubscribeResponse`
//!    (never constructed — the handler answered `no_content`) are gone.
//! 7. **No notification is sent, and the counters are not written here.** `master` called
//!    `NotificationService::notify(USER_SUBSCRIBED)` after the insert and swallowed the
//!    failure. There is no notifications module in this tree (the migration `0012` table
//!    exists; nothing writes it), so the call site is documented rather than faked — when
//!    that module lands, `subscribe` is the one place it attaches. `users.followers` and
//!    `users.following` are the `user_subscribers` trigger's job (migration `0009`), which
//!    is why nothing here writes them.
//!
//! # The layer rule
//!
//! The module reaches down to `infrastructure` (the cursor helpers), `middleware` (the
//! caller), `types` (the envelope) and `meno_core`, and sideways nowhere. `user_subscribers`
//! and `users` are the only tables it reads, and it reads them through its own SQL.

pub mod dto;
pub mod error;
pub mod handlers;
pub mod repository;
pub mod service;
pub mod state;
