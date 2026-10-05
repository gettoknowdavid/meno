//! An in-memory [`AuthRepo`] for tests — §5.6's scriptable double.
//!
//! # What this is for
//!
//! The `services` and `token` tests need a repository whose answers they control and
//! whose state they can read back. `mockall` would give expectations but not state: every
//! test would have to say "when `rotate_session` is called, return `Rotated`" and then
//! separately assert that `revoke_all_sessions` was called, which tests the mock's
//! configuration rather than the service.
//!
//! Here the state is real, so a test can create a session, rotate it, present it again
//! and get the real answer. That is what makes the reuse-detection tests worth writing.
//!
//! # Fidelity
//!
//! The behaviours that a test depends on are implemented to match the Postgres
//! implementation, not to be convenient:
//!
//! - rotation is atomic and single-use — a rotated `jti` answers `Replayed`;
//! - a one-time code is spent by a conditional update, so it works exactly once;
//! - live rows only are visible, so soft-deleting hides an account;
//! - the email uniqueness constraint is a live-row constraint, and a second insert is a
//!   `Conflict` rather than a driver error.
//!
//! The one thing this cannot reproduce is concurrency. A `Mutex` serialises the whole
//! map, so two simultaneous rotations are decided in favour of whichever arrived first —
//! which is the same answer Postgres gives, but for a different reason. The
//! `#[ignore]`d integration tests against a real database are what prove the SQL does it
//! under a genuine race.

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use meno_core::Error as MenoError;
use time::OffsetDateTime;
use uuid::Uuid;

use super::AuthRepo;
use crate::modules::auth::error;
use crate::modules::auth::model::{
    AuthProvider, AuthSession, NewSession, NewUser, Otp, OtpType, RefreshToken, Rotation,
    SessionLookup, User,
};

/// A repository backed by `HashMap`s.
#[derive(Debug, Default)]
pub struct InMemoryAuthRepo {
    state: Mutex<State>,
}

impl InMemoryAuthRepo {
    /// An empty repository.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many refresh tokens have been spent.
    ///
    /// Lets a test assert that a rotation spent the old token rather than merely
    /// succeeding.
    #[must_use]
    pub fn spent_refresh_tokens(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .refresh_spent
    }

    /// What is stored against a live one-time code, if any.
    ///
    /// Exists so a test can assert on the *stored representation* of a code rather than
    /// only on whether spending it worked — "the flow succeeds" is also true when the
    /// code sits in the table in plaintext, which is the property that must not be true.
    /// Returning [`None`] for a spent or unknown code keeps it usable after a spend.
    #[must_use]
    pub fn stored_otp(&self, email: &str, kind: OtpType) -> Option<String> {
        let key = (User::normalized_email(email), kind);
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .codes
            .get(&key)
            .map(|otp| otp.code.clone())
    }
}

#[derive(Debug, Default)]
struct State {
    users: HashMap<Uuid, User>,
    /// Address to user id, for the live rows only.
    email_index: HashMap<String, Uuid>,
    /// Address to user id, for accounts that have a password. Presence here is what
    /// `list_providers` reports as [`AuthProvider::Password`].
    password_index: HashMap<String, Uuid>,
    /// User id to the stored Argon2id PHC string. Absent for a provider-only account,
    /// which is the answer `find_password_hash` must give.
    hashes: HashMap<Uuid, String>,
    /// `(provider, subject)` to user id.
    provider_index: HashMap<(AuthProvider, String), Uuid>,
    /// Every session ever created, live or rotated. `jti` to the row.
    sessions: HashMap<Uuid, AuthSession>,
    /// `jti` to whether the refresh token was spent.
    refresh_spent: usize,
    /// `jti` to `(user id, hash, expiry)`.
    refresh_tokens: HashMap<Uuid, (Uuid, String, OffsetDateTime)>,
    /// Spent refresh `jti`s.
    spent: HashMap<Uuid, Uuid>,
    /// `(address, kind)` to the live code.
    codes: HashMap<(String, OtpType), Otp>,
}

impl State {
    fn locked(repo: &InMemoryAuthRepo) -> std::sync::MutexGuard<'_, State> {
        // `unwrap_or_else(|e| e.into_inner())` rather than `unwrap`: a poisoned lock
        // means some other test panicked while holding it, and the data in here is
        // still consistent enough to read. §9.1 forbids the panic.
        repo.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[async_trait]
impl AuthRepo for InMemoryAuthRepo {
    async fn find_user_by_email(&self, email: &str) -> Result<Option<User>, MenoError> {
        let state = State::locked(self);
        let Some(id) = state.email_index.get(&User::normalized_email(email)) else {
            return Ok(None);
        };
        Ok(state.users.get(id).cloned())
    }

    async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>, MenoError> {
        // `deleted_at IS NULL`, exactly as the Postgres `WHERE` clause says. A double that
        // ignored it would report a soft-deleted account as findable and every
        // deletion test built on it would pass for the wrong reason.
        Ok(State::locked(self)
            .users
            .get(&id)
            .filter(|user| user.deleted_at.is_none())
            .cloned())
    }

    async fn create_user(&self, new: NewUser) -> Result<User, MenoError> {
        let mut state = State::locked(self);
        let email = User::normalized_email(&new.email);

        if state.email_index.contains_key(&email) {
            return Err(error::email_taken());
        }

        let now = OffsetDateTime::now_utc();
        let user = User {
            id: Uuid::new_v4(),
            full_name: new.full_name,
            bio: None,
            email: email.clone(),
            avatar_id: None,
            avatar_url: None,
            verified: false,
            role: "user".to_owned(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };

        state.email_index.insert(email.clone(), user.id);
        if !new.password_hash.is_empty() {
            state.password_index.insert(email, user.id);
            state.hashes.insert(user.id, new.password_hash);
        }
        state.users.insert(user.id, user.clone());

        Ok(user)
    }

    async fn find_password_hash(&self, email: &str) -> Result<Option<String>, MenoError> {
        let state = State::locked(self);
        let email = User::normalized_email(email);
        let Some(id) = state.email_index.get(&email).copied() else {
            return Ok(None);
        };
        // A provider-only account has no password, and answering `Some("")` here would
        // hand the login path an empty string to "verify" against.
        Ok(state.hashes.get(&id).cloned())
    }

    async fn set_password_hash(&self, user_id: Uuid, hash: &str) -> Result<(), MenoError> {
        let mut state = State::locked(self);

        // Sessions first, and unconditionally. The Postgres implementation runs two
        // statements in one transaction with no dependency between them, so revoking
        // them here cannot depend on the user row existing — and an early return on a
        // missing user would let a reset skip the revocation that is the whole point.
        let now = OffsetDateTime::now_utc();
        for session in state.sessions.values_mut() {
            if session.user_id == user_id && session.is_live() {
                session.revoked_at = Some(now);
            }
        }

        if let Some(user) = state.users.get(&user_id).cloned() {
            state.hashes.insert(user_id, hash.to_owned());
            state.password_index.insert(user.email, user_id);
        }

        Ok(())
    }

    async fn set_email_verified(&self, user_id: Uuid) -> Result<bool, MenoError> {
        let mut state = State::locked(self);
        let Some(user) = state.users.get_mut(&user_id) else {
            return Ok(false);
        };
        if user.verified {
            return Ok(false);
        }
        user.verified = true;
        Ok(true)
    }

    async fn list_providers(&self, user_id: Uuid) -> Result<Vec<AuthProvider>, MenoError> {
        let state = State::locked(self);
        let mut providers: Vec<AuthProvider> = state
            .provider_index
            .iter()
            .filter(|(_, id)| **id == user_id)
            .map(|((provider, _), _)| *provider)
            .collect();
        if state.password_index.values().any(|id| *id == user_id) {
            providers.push(AuthProvider::Password);
        }
        providers.sort_by_key(|p| p.as_str());
        providers.dedup();
        Ok(providers)
    }

    async fn soft_delete(&self, user_id: Uuid) -> Result<bool, MenoError> {
        let mut state = State::locked(self);
        let Some(user) = state.users.get_mut(&user_id) else {
            return Ok(false);
        };
        if user.deleted_at.is_some() {
            return Ok(false);
        }
        user.deleted_at = Some(OffsetDateTime::now_utc());
        // The partial unique index releases the address on delete; the double has to
        // agree or a test would conclude that a re-registration is refused. The address
        // is copied out first because `user` holds the borrow and `state`'s other maps
        // need it mutably.
        let email = user.email.clone();
        state.email_index.remove(&email);
        state.password_index.remove(&email);
        state.hashes.remove(&user_id);
        Ok(true)
    }

    async fn create_session(&self, new: NewSession) -> Result<AuthSession, MenoError> {
        let mut state = State::locked(self);
        let now = OffsetDateTime::now_utc();

        let session = AuthSession {
            id: Uuid::new_v4(),
            user_id: new.user_id,
            refresh_jti: new.refresh_jti,
            device_label: new.device.device_label.clone(),
            user_agent: new.device.user_agent.clone(),
            ip: new.device.ip,
            created_at: now,
            last_used_at: now,
            revoked_at: None,
            rotated_from: None,
        };

        state.sessions.insert(session.refresh_jti, session.clone());
        Ok(session)
    }

    async fn rotate_session(
        &self,
        user_id: Uuid,
        current_jti: Uuid,
        replacement: NewSession,
    ) -> Result<Rotation, MenoError> {
        let mut state = State::locked(self);

        match state.sessions.get(&current_jti) {
            // Someone else's session. Not evidence of theft of *this* user's tokens, so
            // it must not fire the revoke-everything path.
            Some(existing) if existing.user_id != user_id => Ok(Rotation::Unknown),
            // Already superseded. This is the reuse signal.
            Some(existing) if existing.rotated_from.is_some() => Ok(Rotation::Replayed),
            Some(_) | None => {
                // Distinguish "never heard of it" from "already rotated" by checking
                // whether any *live* session carries this jti.
                let live = state
                    .sessions
                    .values()
                    .any(|s| s.refresh_jti == current_jti && s.is_live() && s.user_id == user_id);

                if !live {
                    // Superseded rather than deleted. Postgres keeps the row too — the
                    // `rotated_from` chain is the audit trail, and it is the *only* way a
                    // replay can be told from a token we never issued. Deleting here would
                    // silently disable reuse detection for every test built on this double,
                    // which is worse than having no double at all.
                    let was_ours = state
                        .sessions
                        .values()
                        .any(|s| s.refresh_jti == current_jti && s.user_id == user_id);
                    return Ok(if was_ours {
                        Rotation::Replayed
                    } else {
                        Rotation::Unknown
                    });
                }

                if let Some(superseded) = state.sessions.get_mut(&current_jti) {
                    superseded.revoked_at = Some(OffsetDateTime::now_utc());
                }

                let now = OffsetDateTime::now_utc();
                let session = AuthSession {
                    id: Uuid::new_v4(),
                    user_id,
                    refresh_jti: replacement.refresh_jti,
                    device_label: replacement.device.device_label.clone(),
                    user_agent: replacement.device.user_agent.clone(),
                    ip: replacement.device.ip,
                    created_at: now,
                    last_used_at: now,
                    revoked_at: None,
                    rotated_from: Some(current_jti),
                };
                state.sessions.insert(session.refresh_jti, session.clone());
                Ok(Rotation::Rotated(session))
            }
        }
    }

    async fn revoke_session_for_jti(&self, jti: Uuid, user_id: Uuid) -> Result<bool, MenoError> {
        let mut state = State::locked(self);
        let Some(session) = state.sessions.get_mut(&jti) else {
            return Ok(false);
        };
        if session.user_id != user_id {
            return Ok(false);
        }
        if !session.is_live() {
            return Ok(false);
        }

        session.revoked_at = Some(OffsetDateTime::now_utc());
        Ok(true)
    }

    async fn revoke_session(
        &self,
        user_id: Uuid,
        session_id: Uuid,
    ) -> Result<SessionLookup, MenoError> {
        let mut state = State::locked(self);
        let Some(session) = state.sessions.values_mut().find(|s| s.id == session_id) else {
            return Ok(SessionLookup::NotFound);
        };
        if session.user_id != user_id {
            return Ok(SessionLookup::NotFound);
        }
        session.revoked_at = session
            .revoked_at
            .or_else(|| Some(OffsetDateTime::now_utc()));
        Ok(SessionLookup::Found)
    }

    async fn revoke_all_sessions(&self, user_id: Uuid) -> Result<u64, MenoError> {
        let mut state = State::locked(self);
        let now = OffsetDateTime::now_utc();
        let mut ended = 0;

        for session in state.sessions.values_mut() {
            if session.user_id == user_id && session.is_live() {
                session.revoked_at = Some(now);
                ended += 1;
            }
        }

        Ok(ended)
    }

    async fn list_sessions(&self, user_id: Uuid) -> Result<Vec<AuthSession>, MenoError> {
        let state = State::locked(self);
        let mut live: Vec<AuthSession> = state
            .sessions
            .values()
            .filter(|s| s.user_id == user_id && s.is_live())
            .cloned()
            .collect();
        live.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
        Ok(live)
    }

    async fn store_refresh_token(
        &self,
        jti: Uuid,
        user_id: Uuid,
        token_hash: &str,
        expires_at: OffsetDateTime,
    ) -> Result<(), MenoError> {
        let mut state = State::locked(self);
        state
            .refresh_tokens
            .insert(jti, (user_id, token_hash.to_owned(), expires_at));
        Ok(())
    }

    async fn find_refresh_token(
        &self,
        jti: Uuid,
        user_id: Uuid,
    ) -> Result<Option<RefreshToken>, MenoError> {
        let state = State::locked(self);
        let Some((owner, hash, expires_at)) = state.refresh_tokens.get(&jti) else {
            return Ok(None);
        };
        if *owner != user_id || state.spent.contains_key(&jti) {
            return Ok(None);
        }

        Ok(Some(RefreshToken {
            id: jti,
            user_id: *owner,
            token_hash: hash.clone(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            expires_at: *expires_at,
        }))
    }

    async fn consume_refresh_token(&self, jti: Uuid, user_id: Uuid) -> Result<bool, MenoError> {
        let mut state = State::locked(self);
        let owned = state
            .refresh_tokens
            .get(&jti)
            .is_some_and(|(owner, _, _)| *owner == user_id)
            && !state.spent.contains_key(&jti);

        if owned {
            state.spent.insert(jti, user_id);
            state.refresh_spent += 1;
        }
        Ok(owned)
    }

    async fn revoke_refresh_token(&self, jti: Uuid, user_id: Uuid) -> Result<bool, MenoError> {
        let mut state = State::locked(self);
        let owned = state
            .refresh_tokens
            .get(&jti)
            .is_some_and(|(owner, _, _)| *owner == user_id);

        if owned {
            state.spent.insert(jti, user_id);
        }
        Ok(owned)
    }

    async fn save_otp(
        &self,
        email: &str,
        kind: OtpType,
        code: &str,
        expires_at: OffsetDateTime,
    ) -> Result<(), MenoError> {
        let mut state = State::locked(self);
        let email = User::normalized_email(email);
        let now = OffsetDateTime::now_utc();

        state.codes.insert(
            (email.clone(), kind),
            Otp {
                id: Uuid::new_v4(),
                email,
                code: code.to_owned(),
                otp_type: kind.as_str().to_owned(),
                used: false,
                created_at: now,
                expires_at,
            },
        );
        Ok(())
    }

    async fn consume_otp(
        &self,
        email: &str,
        kind: OtpType,
        code: &str,
        now: OffsetDateTime,
    ) -> Result<bool, MenoError> {
        let mut state = State::locked(self);
        let key = (User::normalized_email(email), kind);

        // One conditional update, exactly as the Postgres implementation does it: read
        // then write would let two concurrent requests both spend the same code.
        let Some(stored) = state.codes.get(&key) else {
            return Ok(false);
        };
        if stored.used || stored.code != code || now >= stored.expires_at {
            return Ok(false);
        }

        if let Some(stored) = state.codes.get_mut(&key) {
            stored.used = true;
        }
        Ok(true)
    }

    async fn find_user_by_provider(
        &self,
        provider: AuthProvider,
        provider_user_id: &str,
    ) -> Result<Option<User>, MenoError> {
        let state = State::locked(self);
        let Some(id) = state
            .provider_index
            .get(&(provider, provider_user_id.to_owned()))
        else {
            return Ok(None);
        };
        Ok(state.users.get(id).cloned())
    }

    async fn link_provider(
        &self,
        user_id: Uuid,
        provider: AuthProvider,
        provider_user_id: &str,
    ) -> Result<User, MenoError> {
        let mut state = State::locked(self);
        let Some(user) = state.users.get(&user_id).cloned() else {
            return Err(error::user_not_found());
        };
        state
            .provider_index
            .insert((provider, provider_user_id.to_owned()), user_id);
        Ok(user)
    }

    async fn create_user_from_provider(
        &self,
        new: NewUser,
        provider: AuthProvider,
        provider_user_id: &str,
    ) -> Result<User, MenoError> {
        // A verified provider email is a verified email — the provider has already
        // proved ownership, and making the user click a second link is the friction that
        // pushes people back to passwords.
        let email = User::normalized_email(&new.email);
        if state_taken(&State::locked(self), &email) {
            return Err(error::email_taken());
        }

        let mut state = State::locked(self);
        let now = OffsetDateTime::now_utc();
        let user = User {
            id: Uuid::new_v4(),
            full_name: new.full_name,
            bio: None,
            email: email.clone(),
            avatar_id: None,
            avatar_url: None,
            verified: true,
            role: "user".to_owned(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };

        state.email_index.insert(email, user.id);
        state
            .provider_index
            .insert((provider, provider_user_id.to_owned()), user.id);
        state.users.insert(user.id, user.clone());
        Ok(user)
    }
}

/// Whether a live account holds `email`.
fn state_taken(state: &State, email: &str) -> bool {
    state
        .email_index
        .get(email)
        .is_some_and(|id| state.users.get(id).is_some_and(|u| u.deleted_at.is_none()))
}
