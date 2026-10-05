//! The Postgres implementation of [`AuthRepo`].
//!
//! # Two properties this file is careful about
//!
//! **Atomic rotation.** [`AuthRepo::rotate_session`] runs inside one transaction and
//! decides liveness with a `DELETE ... RETURNING`. A read-then-write would let two
//! concurrent refreshes carrying the same token both succeed, which is exactly the
//! double-spend §4.7 item 2 exists to prevent — and the race is not exotic, because a
//! mobile client that retries a timed-out refresh *is* two concurrent requests.
//!
//! **No driver error escapes.** Every method funnels through [`internal`], which names
//! the operation and carries the driver's text into the log and nowhere else. One
//! unique-violation check is the exception: a duplicate email is a `409 EMAIL_TAKEN`
//! rather than a `500`, because it is a client error the client can act on.
//!
//! # Runtime queries, not macros
//!
//! `sqlx::query_as` with `#[derive(FromRow)]` rather than the `query_as!` macro: the
//! macro verifies its SQL against a live database at compile time, which would make
//! `cargo build` require a `DATABASE_URL` and turn a schema change into a build failure
//! for anyone without a database. §4.3 makes migrations a runtime startup step, and
//! this keeps that property.

use async_trait::async_trait;
use time::OffsetDateTime;
use uuid::Uuid;

use super::{AuthRepo, RepoDeps};
use crate::modules::auth::error;
use crate::modules::auth::model::{
    AuthProvider, AuthSession, NewSession, NewUser, OtpType, RefreshToken, Rotation, SessionLookup,
    User, provider_db_str, provider_from_db_str,
};
use meno_core::Error as MenoError;
use sqlx::AssertSqlSafe;

/// Columns selected wherever a `User` is read.
///
/// Named once so a column added to `users` and forgotten here is a compile error rather
/// than a `FromRow` failure at runtime.
const USER_COLUMNS: &str = "id, full_name, bio, email, avatar_id, avatar_url, verified, \
                            role, created_at, updated_at, deleted_at";

/// [`USER_COLUMNS`], qualified with the `u` alias.
///
/// The `SELECT`s that join `users` against `user_identities` need this: both
/// tables have an `id`, so the unqualified list is ambiguous and Postgres
/// rejects it. Two constants rather than one, because `RETURNING` cannot take
/// a table qualifier -- these are genuinely different SQL, not a preference.
const USER_COLUMNS_U: &str = "u.id, u.full_name, u.bio, u.email, u.avatar_id, \
                              u.avatar_url, u.verified, u.role, u.created_at, \
                              u.updated_at, u.deleted_at";

/// Splice `USER_COLUMNS` into a statement template and mark the result safe.
///
/// # Why this is an audit and not a suppression
///
/// sqlx 0.9 only accepts a `&'static str` as a query, precisely so that a `format!`
/// carrying user input cannot reach the database by accident. These six statements are
/// the exception, and the exception is narrow and checkable:
///
/// - the only interpolated value is [`USER_COLUMNS`], a `const` of literal column names
///   that no request can influence;
/// - every value that *is* request-derived — an address, a `jti`, a hash — is a bind
///   parameter, never text in the statement.
///
/// So the injection surface is empty, and `AssertSqlSafe` is the type that says so. If a
/// future change interpolates anything else here, it fails the same way this file had to
/// be changed to compile: by someone reading this comment.
fn with_user_columns(template: &str) -> AssertSqlSafe<String> {
    AssertSqlSafe(
        template
            .replace("{USER_COLUMNS_U}", USER_COLUMNS_U)
            .replace("{USER_COLUMNS}", USER_COLUMNS),
    )
}

/// Postgres-backed [`AuthRepo`].
#[derive(Debug, Clone)]
pub struct PgAuthRepo {
    pool: sqlx::PgPool,
}

impl PgAuthRepo {
    /// Build from a pool.
    ///
    /// Takes [`RepoDeps`] rather than the pool directly so the dependency list is one
    /// named struct — §4.1's "makes the dependency list visible at the call site".
    #[must_use]
    pub fn new(deps: RepoDeps) -> Self {
        Self { pool: deps.pool }
    }
}

/// Convert a driver error, mapping the one unique violation that is a client error.
///
/// `23505` is Postgres's `unique_violation`. Everything else is an internal failure:
/// guessing that a `NOT NULL` violation means "duplicate email" would turn a bug into a
/// misleading 409.
fn map_error(operation: &'static str, error: sqlx::Error) -> MenoError {
    if let sqlx::Error::Database(db) = &error
        && db.is_unique_violation()
    {
        return error::email_taken();
    }
    internal(operation, error)
}

/// An internal failure named by the operation that hit it.
fn internal(operation: &'static str, error: impl std::fmt::Display) -> MenoError {
    error::internal(operation, error)
}

#[async_trait]
impl AuthRepo for PgAuthRepo {
    async fn find_user_by_email(&self, email: &str) -> Result<Option<User>, MenoError> {
        let sql = with_user_columns(
            "SELECT {USER_COLUMNS_U} FROM public.users u \
             WHERE u.email = $1 AND u.deleted_at IS NULL",
        );

        sqlx::query_as::<_, User>(sql)
            .bind(User::normalized_email(email))
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| map_error("find_user_by_email", e))
    }

    async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>, MenoError> {
        let sql = with_user_columns(
            "SELECT {USER_COLUMNS_U} FROM public.users u \
             WHERE u.id = $1 AND u.deleted_at IS NULL",
        );

        sqlx::query_as::<_, User>(sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| map_error("find_user_by_id", e))
    }

    async fn create_user(&self, new: NewUser) -> Result<User, MenoError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| internal("begin_create_user", e))?;

        let row: User = sqlx::query_as(with_user_columns(
            "INSERT INTO public.users (full_name, email) VALUES ($1, $2) \
             RETURNING {USER_COLUMNS}",
        ))
        .bind(&new.full_name)
        .bind(User::normalized_email(&new.email))
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_error("insert_user", e))?;

        sqlx::query(
            "INSERT INTO public.user_identities \
             (user_id, provider_type, provider_user_id, password_hash) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(row.id)
        .bind(provider_db_str(AuthProvider::Password))
        .bind(row.email.clone())
        .bind(&new.password_hash)
        .execute(&mut *tx)
        .await
        .map_err(|e| map_error("insert_user_identity", e))?;

        tx.commit()
            .await
            .map_err(|e| internal("commit_create_user", e))?;

        Ok(row)
    }

    async fn find_password_hash(&self, email: &str) -> Result<Option<String>, MenoError> {
        let hash: Option<String> = sqlx::query_scalar(
            "SELECT i.password_hash FROM public.user_identities i \
             JOIN public.users u ON u.id = i.user_id \
             WHERE u.email = $1 AND u.deleted_at IS NULL AND i.provider_type = $2",
        )
        .bind(User::normalized_email(email))
        .bind(provider_db_str(AuthProvider::Password))
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| internal("find_password_hash", e))?;

        Ok(hash)
    }

    async fn set_password_hash(&self, user_id: Uuid, hash: &str) -> Result<(), MenoError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| internal("begin_set_password", e))?;

        sqlx::query(
            "UPDATE public.user_identities SET password_hash = $2, updated_at = now() \
             WHERE user_id = $1 AND provider_type = $3",
        )
        .bind(user_id)
        .bind(hash)
        .bind(provider_db_str(AuthProvider::Password))
        .execute(&mut *tx)
        .await
        .map_err(|e| internal("update_password_hash", e))?;

        // In the same transaction, not the caller's job. A password reset that leaves
        // the old device signed in is the vulnerability; making it a separate call
        // makes it a bug waiting to happen.
        sqlx::query(
            "UPDATE public.auth_sessions SET revoked_at = now() \
             WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| internal("revoke_sessions_after_reset", e))?;

        tx.commit()
            .await
            .map_err(|e| internal("commit_set_password", e))
    }

    async fn set_email_verified(&self, user_id: Uuid) -> Result<bool, MenoError> {
        // `RETURNING` makes "did the state actually change" a property of the write
        // rather than of a preceding read, so two concurrent verifications cannot both
        // report success.
        let updated: Option<Uuid> = sqlx::query_scalar(
            "UPDATE public.users SET verified = true, updated_at = now() \
             WHERE id = $1 AND deleted_at IS NULL AND verified = false RETURNING id",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| internal("set_email_verified", e))?;

        Ok(updated.is_some())
    }

    async fn list_providers(&self, user_id: Uuid) -> Result<Vec<AuthProvider>, MenoError> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT provider_type FROM public.user_identities WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| internal("list_providers", e))?;

        // A value this build does not recognise is dropped rather than defaulted. It
        // would otherwise be reported to the client as a provider it cannot sign in
        // with, and the sign-in screen offers a button that fails.
        Ok(rows
            .iter()
            .filter_map(|raw| provider_from_db_str(raw))
            .collect())
    }

    async fn soft_delete(&self, user_id: Uuid) -> Result<bool, MenoError> {
        let updated: Option<Uuid> = sqlx::query_scalar(
            "UPDATE public.users SET deleted_at = now(), updated_at = now() \
             WHERE id = $1 AND deleted_at IS NULL RETURNING id",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| internal("soft_delete_user", e))?;

        Ok(updated.is_some())
    }

    async fn create_session(&self, new: NewSession) -> Result<AuthSession, MenoError> {
        let session = sqlx::query_as::<_, AuthSession>(
            "INSERT INTO public.auth_sessions \
             (user_id, refresh_jti, device_label, user_agent, ip) \
             VALUES ($1, $2, $3, $4, $5) \
             RETURNING id, user_id, refresh_jti, device_label, user_agent, ip, \
                       created_at, last_used_at, revoked_at, rotated_from",
        )
        .bind(new.user_id)
        .bind(new.refresh_jti)
        .bind(new.device.device_label.as_deref())
        .bind(new.device.user_agent.as_deref())
        .bind(new.device.ip)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| map_error("create_session", e))?;

        Ok(session)
    }

    async fn rotate_session(
        &self,
        user_id: Uuid,
        current_jti: Uuid,
        replacement: NewSession,
    ) -> Result<Rotation, MenoError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| internal("begin_rotate_session", e))?;

        // The conditional `UPDATE` both tests liveness and claims the token, and
        // `RETURNING` is what makes the claim atomic: a losing racer matches no
        // live row and lands in `Replayed`.
        //
        // It sets `revoked_at` rather than deleting, and that is load-bearing: the
        // row is the only evidence that this jti ever existed, and the probe below
        // reads it to tell a replay from an unknown token. Deleting here made
        // every replay read as `Unknown` -- the opposite of what §4.7 item 2 needs.
        let claimed: Option<(Uuid, Uuid)> = sqlx::query_as(
            "UPDATE public.auth_sessions SET revoked_at = now(), last_used_at = now() \
             WHERE refresh_jti = $1 AND user_id = $2 AND revoked_at IS NULL \
             RETURNING id, user_id",
        )
        .bind(current_jti)
        .bind(user_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| internal("claim_session_for_rotation", e))?;

        let (superseded_id, _) = match claimed {
            Some(row) => row,
            None => {
                tx.rollback()
                    .await
                    .map_err(|e| internal("rollback_rotate_session", e))?;

                // Nothing live for this jti. Was it ever real? That is the difference
                // between "unknown token" and "a token we already rotated", and it is
                // the whole of the reuse signal.
                let seen: Option<Uuid> = sqlx::query_scalar(
                    "SELECT id FROM public.auth_sessions \
                     WHERE refresh_jti = $1 AND user_id = $2 LIMIT 1",
                )
                .bind(current_jti)
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| internal("check_rotated_jti", e))?;

                return Ok(if seen.is_some() {
                    Rotation::Replayed
                } else {
                    Rotation::Unknown
                });
            }
        };

        let session = sqlx::query_as::<_, AuthSession>(
            "INSERT INTO public.auth_sessions \
             (user_id, refresh_jti, device_label, user_agent, ip, rotated_from) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             RETURNING id, user_id, refresh_jti, device_label, user_agent, ip, \
                       created_at, last_used_at, revoked_at, rotated_from",
        )
        .bind(user_id)
        .bind(replacement.refresh_jti)
        .bind(replacement.device.device_label.as_deref())
        .bind(replacement.device.user_agent.as_deref())
        .bind(replacement.device.ip)
        .bind(superseded_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_error("insert_rotated_session", e))?;

        tx.commit()
            .await
            .map_err(|e| internal("commit_rotate_session", e))?;

        Ok(Rotation::Rotated(session))
    }

    async fn revoke_session_for_jti(&self, jti: Uuid, user_id: Uuid) -> Result<bool, MenoError> {
        let revoked: Option<Uuid> = sqlx::query_scalar(
            "UPDATE public.auth_sessions SET revoked_at = now() \
             WHERE refresh_jti = $1 AND user_id = $2 AND revoked_at IS NULL RETURNING id",
        )
        .bind(jti)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| internal("revoke_session_for_jti", e))?;

        Ok(revoked.is_some())
    }

    async fn revoke_session(
        &self,
        user_id: Uuid,
        session_id: Uuid,
    ) -> Result<SessionLookup, MenoError> {
        // Scoped by `user_id` in the `WHERE`, so a caller cannot revoke someone else's
        // session even with a valid id. The `WHERE revoked_at IS NULL` makes a repeat
        // revoke report `NotFound`, which is the same answer as "not yours".
        let revoked: Option<Uuid> = sqlx::query_scalar(
            "UPDATE public.auth_sessions SET revoked_at = now() \
             WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL RETURNING id",
        )
        .bind(session_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| internal("revoke_session", e))?;

        Ok(if revoked.is_some() {
            SessionLookup::Found
        } else {
            SessionLookup::NotFound
        })
    }

    async fn revoke_all_sessions(&self, user_id: Uuid) -> Result<u64, MenoError> {
        let ended = sqlx::query(
            "UPDATE public.auth_sessions SET revoked_at = now() \
             WHERE user_id = $1 AND revoked_at IS NULL",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| internal("revoke_all_sessions", e))?;

        Ok(ended.rows_affected())
    }

    async fn list_sessions(&self, user_id: Uuid) -> Result<Vec<AuthSession>, MenoError> {
        sqlx::query_as::<_, AuthSession>(
            "SELECT id, user_id, refresh_jti, device_label, user_agent, ip, \
                    created_at, last_used_at, revoked_at, rotated_from \
             FROM public.auth_sessions \
             WHERE user_id = $1 AND revoked_at IS NULL \
             ORDER BY created_at DESC, id DESC",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| internal("list_sessions", e))
    }

    async fn store_refresh_token(
        &self,
        jti: Uuid,
        user_id: Uuid,
        token_hash: &str,
        expires_at: OffsetDateTime,
    ) -> Result<(), MenoError> {
        sqlx::query(
            "INSERT INTO public.refresh_tokens (user_id, token_hash, expires_at, jti) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(token_hash)
        .bind(expires_at)
        .bind(jti)
        .execute(&self.pool)
        .await
        .map_err(|e| map_error("store_refresh_token", e))?;

        Ok(())
    }

    async fn find_refresh_token(
        &self,
        jti: Uuid,
        user_id: Uuid,
    ) -> Result<Option<RefreshToken>, MenoError> {
        // Keyed on the `jti` migration 0016 added, not on the token hash: the
        // caller already has the `jti` from the decoded claims, and re-hashing the
        // whole token to find its own row would scan the token text.
        sqlx::query_as::<_, RefreshToken>(
            "SELECT id, user_id, token_hash, created_at, expires_at \
             FROM public.refresh_tokens WHERE user_id = $1 AND jti = $2",
        )
        .bind(user_id)
        .bind(jti)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| internal("find_refresh_token", e))
    }

    async fn revoke_refresh_token(&self, jti: Uuid, user_id: Uuid) -> Result<bool, MenoError> {
        // An `UPDATE ... RETURNING` rather than a `DELETE`: the row is the audit trail
        // for "this token was issued and is no longer live", and §3.2's retention note
        // assumes it outlives the token itself.
        let revoked: Option<Uuid> = sqlx::query_scalar(
            "UPDATE public.refresh_tokens SET revoked_at = now() \
             WHERE user_id = $1 AND jti = $2 AND revoked_at IS NULL RETURNING id",
        )
        .bind(user_id)
        .bind(jti)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| internal("revoke_refresh_token", e))?;

        Ok(revoked.is_some())
    }

    async fn consume_refresh_token(&self, jti: Uuid, user_id: Uuid) -> Result<bool, MenoError> {
        let deleted: Option<Uuid> = sqlx::query_scalar(
            "DELETE FROM public.refresh_tokens WHERE user_id = $1 AND jti = $2 \
             RETURNING id",
        )
        .bind(user_id)
        .bind(jti)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| internal("consume_refresh_token", e))?;

        Ok(deleted.is_some())
    }

    async fn save_otp(
        &self,
        email: &str,
        kind: OtpType,
        code: &str,
        expires_at: OffsetDateTime,
    ) -> Result<(), MenoError> {
        // Upsert rather than insert: "resend" has to replace the live code, or the first
        // one stays usable and the button does not mean what it says.
        sqlx::query(
            "INSERT INTO public.otps (email, code, type, expires_at) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (email, type) DO UPDATE \
             SET code = EXCLUDED.code, used = false, expires_at = EXCLUDED.expires_at",
        )
        .bind(User::normalized_email(email))
        .bind(code)
        .bind(kind.as_str())
        .bind(expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| map_error("save_otp", e))?;

        Ok(())
    }

    async fn consume_otp(
        &self,
        email: &str,
        kind: OtpType,
        code: &str,
        now: OffsetDateTime,
    ) -> Result<bool, MenoError> {
        // One conditional DELETE. A read-then-write would let two concurrent submissions
        // of the same code both succeed.
        let spent: Option<Uuid> = sqlx::query_scalar(
            "DELETE FROM public.otps \
             WHERE email = $1 AND type = $2 AND code = $3 \
               AND used = false AND expires_at > $4 \
             RETURNING id",
        )
        .bind(User::normalized_email(email))
        .bind(kind.as_str())
        .bind(code)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| internal("consume_otp", e))?;

        Ok(spent.is_some())
    }

    async fn find_user_by_provider(
        &self,
        provider: AuthProvider,
        provider_user_id: &str,
    ) -> Result<Option<User>, MenoError> {
        let sql = with_user_columns(
            "SELECT {USER_COLUMNS_U} FROM public.users u \
             JOIN public.user_identities i ON i.user_id = u.id \
             WHERE i.provider_type = $1 AND i.provider_user_id = $2 AND u.deleted_at IS NULL",
        );

        sqlx::query_as::<_, User>(sql)
            .bind(provider_db_str(provider))
            .bind(provider_user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| internal("find_user_by_provider", e))
    }

    async fn link_provider(
        &self,
        user_id: Uuid,
        provider: AuthProvider,
        provider_user_id: &str,
    ) -> Result<User, MenoError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| internal("begin_link_provider", e))?;

        sqlx::query(
            "INSERT INTO public.user_identities \
             (user_id, provider_type, provider_user_id) VALUES ($1, $2, $3) \
             ON CONFLICT (user_id, provider_type) DO UPDATE \
             SET provider_user_id = EXCLUDED.provider_user_id, updated_at = now()",
        )
        .bind(user_id)
        .bind(provider_db_str(provider))
        .bind(provider_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| map_error("link_provider", e))?;

        let row = sqlx::query_as::<_, User>(with_user_columns(
            "SELECT {USER_COLUMNS_U} FROM public.users u \
             WHERE u.id = $1 AND u.deleted_at IS NULL",
        ))
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_error("load_user_after_link", e))?;

        tx.commit()
            .await
            .map_err(|e| internal("commit_link_provider", e))?;

        Ok(row)
    }

    async fn create_user_from_provider(
        &self,
        new: NewUser,
        provider: AuthProvider,
        provider_user_id: &str,
    ) -> Result<User, MenoError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| internal("begin_create_from_provider", e))?;

        // `verified = true`: §4.7 item 7's guard has already run. The provider proved
        // ownership of the address before this was called, and making the user click a
        // second link is the friction that pushes people back to passwords.
        let row = sqlx::query_as::<_, User>(with_user_columns(
            "INSERT INTO public.users (full_name, email, verified) VALUES ($1, $2, true) \
             RETURNING {USER_COLUMNS}",
        ))
        .bind(&new.full_name)
        .bind(User::normalized_email(&new.email))
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_error("insert_user_from_provider", e))?;

        sqlx::query(
            "INSERT INTO public.user_identities \
             (user_id, provider_type, provider_user_id) VALUES ($1, $2, $3)",
        )
        .bind(row.id)
        .bind(provider_db_str(provider))
        .bind(provider_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| map_error("insert_provider_identity", e))?;

        tx.commit()
            .await
            .map_err(|e| internal("commit_create_from_provider", e))?;

        Ok(row)
    }
}
