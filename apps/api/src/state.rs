//! Top-level application state.
//!
//! # What this is, and what `master` had
//!
//! `master` had a `MenoState` with thirteen fields — `ProfileState`, `BroadcastState`,
//! `SubscribersState`, `NotificationState`, `ChatState`, `NotesState`, `SettingsState`,
//! `WsService`, `Jobs`, `smtp` — assembled by a `build_meno_router` that constructed
//! each one and merged a route tree from a `routes` module that also does not exist here.
//! None of that code compiles against the current tree, and `lib.rs` never declared this
//! module, so the file was dead: a description of an application rather than part of one.
//!
//! This is the same idea rebuilt around what actually exists. **Auth is the only
//! refactored module**, so [`MenoState`] carries auth plus the infrastructure every
//! module will need. When the next module lands it adds a field here, and nothing else
//! in this file changes — which is the property worth preserving.
//!
//! # Why not the other modules
//!
//! Three reasons, and the third is the one that matters:
//!
//! 1. **They do not exist.** There is no `modules::profile`, no `modules::broadcast`, no
//!    `jobs/`. Naming them would not compile.
//! 2. **A field is a promise.** `pub profile: ProfileState` asserts that the profile
//!    routes work, that the adapter is wired, and that someone can review it. With no
//!    implementation behind it the field is a lie the type system cannot catch.
//! 3. **The alternative already failed once.** `master` shipped all thirteen and the
//!    result was a wiring layer that could not be compiled, tested, or reasoned about
//!    without the modules underneath it. Rebuilding it incrementally is the point.
//!
//! # No handler takes `&MenoState`
//!
//! Each handler receives its own module state through axum's [`State`] extractor. This
//! struct is the assembly point, not a god object handed to business logic — a handler
//! that needed all thirteen fields would be a handler doing too much, and making that
//! awkward to express is deliberate.
//!
//! # Construction is fallible (§4.3, §7.11)
//!
//! [`build`] returns a `Result`. `master`'s equivalent used `.expect("Invalid Google
//! redirect URI")` and three `.unwrap()`s, so a typo in one `.env` variable became a
//! panic with a stack trace at boot. Here every failure names the variable that is
//! wrong.

use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;
use crate::infrastructure::oauth::{OAuthStateStore, RedisOAuthStateStore};
use crate::infrastructure::push::PushSender;
use crate::infrastructure::redis::Redis;
use crate::infrastructure::storage::ObjectStore;
use crate::modules::auth::mailer::AuthMailer;
use crate::modules::auth::state::AuthState;
use crate::modules::broadcast::state::BroadcastState;

/// Everything the running application holds.
///
/// Cheap to clone — every field is an `Arc`, a pool or a handle — so axum's `State`
/// extractor can hold one per request without meaningful cost.
#[derive(Clone)]
pub struct MenoState {
    /// Validated configuration (§4.6). Shared rather than cloned per module.
    pub config: Arc<Config>,
    /// The Postgres pool, for migrations, health checks and future modules.
    pub db: PgPool,
    /// Redis, for the auth token blocklist and the OAuth state store.
    pub redis: Redis,
    /// Authentication (§4.7).
    pub auth: AuthState,
    /// Broadcasts — create, schedule, go live, join, participants (§2's tree).
    ///
    /// The second module to land, and the one that shows why a module carries its own
    /// state rather than reaching into the application: this field is a projection of a
    /// repository and a media adapter, and nothing outside `state` names either.
    pub broadcast: BroadcastState,
    /// The access-token guard the router mounts on protected routes (§4.7 items 5-6).
    ///
    /// Built here rather than in `routes.rs` because it is fallible — a blank
    /// `JWT_SECRET` must stop boot (§7.11) — and because this file is the one place
    /// allowed to name concrete adapters. It is the middleware's own view (verifier +
    /// blocklist); no handler takes it, which is the point of keeping it beside — but
    /// separate from — [`Self::auth`].
    pub auth_guard: crate::middleware::auth::AuthState,
    /// Push notifications. Optional (§4.6); a no-op sender when disabled.
    ///
    /// Held here rather than by a module because no module uses it yet. It is the first
    /// thing `notifications/` will need, and wiring it at that point is a one-line
    /// change to this struct.
    pub push: Arc<dyn PushSender>,
    /// Object storage. Optional (§4.6); a no-op store when disabled.
    pub storage: Arc<dyn ObjectStore>,
    /// The Prometheus handle `GET /metrics` renders (§7.5, §4.8).
    ///
    /// Cloned from the process-wide recorder; whether the endpoint *answers* is
    /// decided by [`Self::config`]'s `metrics_token`, not by this field — the
    /// recorder keeps counting either way, so enabling the endpoint later shows
    /// history from the moment it started counting.
    pub metrics: crate::infrastructure::metrics::Metrics,
    /// The local socket registry behind `GET /ws` (plan §4.7's realtime surface).
    ///
    /// "Local" is the point: which sockets live on *this* replica, and nothing about
    /// the others. Cross-replica delivery is [`Self::ws_bridge`]'s job, and keeping
    /// the two apart is what lets the registry be I/O-free — and therefore tested
    /// without a Redis.
    pub ws: crate::infrastructure::ws::WsService,
    /// Redis pub/sub fan-out for events that must reach other replicas (§3.1).
    ///
    /// Built eagerly rather than on first publish: a bridge that cannot reach Redis
    /// must stop boot with a message (§7.11), not fail one delivery at a time once
    /// traffic is live. Its subscriber loop is spawned by `bootstrap`, because only
    /// that knows whether this process holds sockets.
    pub ws_bridge: crate::infrastructure::ws::pubsub::WsPubSubBridge,
}

/// Lets axum hand a nested sub-router its own state.
///
/// This is the one place the application state is allowed to know about a module's
/// state, and it is a *projection*: the router can extract `AuthState` from
/// `MenoState` without a handler ever seeing `MenoState`. Without it, mounting the
/// auth routes would mean either widening all fourteen handlers to take the whole
/// application — the god object §9.3 rules out — or unwrapping the field inside
/// [`build_routes`](crate::routes::build_routes), which would tie the route table to
/// this struct's layout.
///
/// The impl is infallible and total, so it cannot refuse a mount.
impl axum::extract::FromRef<MenoState> for AuthState {
    fn from_ref(state: &MenoState) -> Self {
        state.auth.clone()
    }
}

/// The same projection for the broadcast module (plan §4.1).
///
/// The second implementation of the same idea, and the point of naming it: every
/// module that lands adds one `FromRef` and one field, and the handlers keep taking
/// their own state. The alternative — a sub-router taking `MenoState` — would hand every
/// handler the whole application, which §9.3 calls a god object and §2's tree is built
/// to avoid.
impl axum::extract::FromRef<MenoState> for BroadcastState {
    fn from_ref(state: &MenoState) -> Self {
        state.broadcast.clone()
    }
}

impl std::fmt::Debug for MenoState {
    /// Names the fields and nothing else.
    ///
    /// Every one of these holds a credential somewhere — the config has JWT secrets,
    /// `Redis` and the config's URL fields have passwords — so this writes the
    /// struct's shape rather than deriving it. §4.8's structured logs are only useful
    /// if a `{:?}` cannot leak one.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenoState")
            .field("config", &"[redacted]")
            .field("db", &"[pool]")
            .field("redis", &"[redacted]")
            .field("auth", &self.auth)
            .field("auth_guard", &self.auth_guard)
            .field("broadcast", &self.broadcast)
            .field("push", &"[sender]")
            .field("storage", &"[store]")
            .field("metrics", &self.metrics)
            .field("ws", &"[sockets]")
            .field("ws_bridge", &"[bridge]")
            .finish()
    }
}

/// The collaborators [`build`] cannot assemble for itself.
///
/// Kept as a parameter rather than constructed inside so a test — or a future
/// in-process job runner — can supply an [`AuthMailer`] of its choosing. The mailer is
/// §4.6's optional integration, and the decision about which one to use belongs to
/// whoever is assembling the application.
#[derive(Clone)]
pub struct Assembly {
    /// Validated configuration.
    pub config: Arc<Config>,
    /// The Postgres pool.
    pub db: PgPool,
    /// The Redis handle.
    pub redis: Redis,
    /// Transactional mail for verification and reset codes.
    pub mailer: Arc<dyn AuthMailer>,
    /// The push sender, already built from configuration.
    pub push: Arc<dyn PushSender>,
    /// The object store, already built from configuration.
    pub storage: Arc<dyn ObjectStore>,
}

/// Wire the application.
///
/// # Errors
///
/// [`AuthState::new`], which returns the first of these that is wrong:
///
/// - a blank, placeholder or shared JWT secret pair, or a non-positive lifetime
///   (§4.7 item 4 — a shared pair means a refresh token would validate as an access
///   token);
/// - Google sign-in enabled but the settings do not build a client;
/// - the Argon2id dummy hash that equalises login timing could not be computed;
/// - the WebSocket pub/sub bridge cannot reach Redis — an eagerly-built adapter that
///   is enabled but unusable, refused at boot rather than per message.
///
/// All of these are startup failures by design: booting anyway turns each into a
/// one-request-at-a-time outage discovered by a user, rather than a boot that stops
/// with a message naming the variable at fault.
pub async fn build(parts: Assembly) -> Result<MenoState, meno_core::Error> {
    // Redis, not the in-memory store: an OAuth callback is a separate request that may
    // land on another replica, and a state stored in one process's memory is invisible
    // to the others — which would refuse every such callback as a CSRF replay.
    let oauth_states: Arc<dyn OAuthStateStore> =
        Arc::new(RedisOAuthStateStore::new(parts.redis.clone()));

    let auth = AuthState::new(crate::modules::auth::state::Wiring {
        pool: parts.db.clone(),
        redis: parts.redis.clone(),
        config: Arc::clone(&parts.config),
        oauth_states,
        mailer: parts.mailer.clone(),
    })?;

    // The router's guard (§4.7 items 5-6): one HMAC check against the access secret
    // and one blocklist round trip per protected request. `JwtVerifier::new` refuses a
    // blank secret so a typo'd `JWT_SECRET` fails here, at boot, instead of rejecting
    // every token at runtime (§7.11).
    let auth_guard = crate::middleware::auth::AuthState::new(
        Arc::new(crate::middleware::auth::JwtVerifier::new(
            parts.config.jwt_secret.expose(),
        )?),
        Arc::new(crate::middleware::auth::RedisTokenBlocklist::new(
            parts.redis.clone(),
        )),
    );

    // The realtime pair: the local socket registry, and the pub/sub bridge that makes
    // a message published here reach sockets on other replicas (§3.1). The bridge is
    // built — and verifies both of its clients — right now rather than on first
    // publish, so an unreachable Redis is a boot failure with a message instead of a
    // silent one-replica delivery failure discovered when a message goes missing.
    let ws = crate::infrastructure::ws::WsService::new(parts.redis.clone());
    let ws_bridge = crate::infrastructure::ws::pubsub::WsPubSubBridge::build_from_config(
        &crate::infrastructure::redis::RedisConfig::from_url(
            parts.config.redis_url.expose().to_owned(),
        ),
        ws.clone(),
        parts.redis.clone(),
    )
    .await
    .map_err(|error| meno_core::Error::Internal {
        context: "build_ws_pubsub_bridge",
        detail: error.to_string(),
    })?;

    // Built before the struct literal, because the literal moves `parts.db` and
    // `parts.config` and this needs both. Cloning a pool and an `Arc` is free; a
    // borrow-after-move is not.
    let broadcast = BroadcastState::new(crate::modules::broadcast::state::Wiring {
        pool: parts.db.clone(),
        livekit: parts.config.livekit.clone(),
    });

    Ok(MenoState {
        config: parts.config,
        db: parts.db,
        redis: parts.redis,
        auth,
        // Broadcasts (§2's tree). The media server is optional — §4.6 — so `Wiring`
        // chose an adapter from configuration rather than this file demanding one, and a
        // deployment without LiveKit still gets drafts, schedules and the catalogue.
        broadcast,
        auth_guard,
        push: parts.push,
        storage: parts.storage,
        metrics: crate::infrastructure::metrics::Metrics::global(),
        ws,
        ws_bridge,
    })
}
