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
    /// Authentication. The only refactored module so far (§4.7).
    pub auth: AuthState,
    /// Push notifications. Optional (§4.6); a no-op sender when disabled.
    ///
    /// Held here rather than by a module because no module uses it yet. It is the first
    /// thing `notifications/` will need, and wiring it at that point is a one-line
    /// change to this struct.
    pub push: Arc<dyn PushSender>,
    /// Object storage. Optional (§4.6); a no-op store when disabled.
    pub storage: Arc<dyn ObjectStore>,
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
            .field("push", &"[sender]")
            .field("storage", &"[store]")
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
/// - the Argon2id dummy hash that equalises login timing could not be computed.
///
/// All three are startup failures by design: booting anyway turns each into a
/// one-request-at-a-time outage discovered by a user, rather than a boot that stops
/// with a message naming the variable at fault.
pub fn build(parts: Assembly) -> Result<MenoState, meno_core::Error> {
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

    Ok(MenoState {
        config: parts.config,
        db: parts.db,
        redis: parts.redis,
        auth,
        push: parts.push,
        storage: parts.storage,
    })
}
