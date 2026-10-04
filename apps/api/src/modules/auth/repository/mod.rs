//! What auth needs from Postgres, and nothing else.
//!
//! # Trait objects, deliberately
//!
//! §4.1 asks for exactly this: *"Replace with explicit trait objects for consistency
//! with the auth module"*, with `new(deps) -> Self` and *"no Option, no expect, no
//! panic"*.
//!
//! The previous revision already used `Arc<dyn AuthRepo>` — but alongside a *generic*
//! `TokenService<R, C>` with two type parameters threaded through it, and a builder with
//! `expect()` calls. The generics bought nothing here: every production call site used
//! `PgAuthRepo` and `RedisAuthCache`, so the parameters were two ways of spelling a
//! monomorphisation. A `dyn` costs one pointer per call and buys a compile error when a
//! dependency is missing.
//!
//! # The one method that carries a security decision
//!
//! [`AuthRepo::rotate_session`] returns [`Rotation`] rather than `bool`. See
//! [`Rotation`] for why the three-way split is load-bearing.
//!
//! # Errors
//!
//! Every method returns [`meno_core::Error`]. A `sqlx::Error` never crosses this
//! boundary: it is converted to [`MenoError::Internal`] with the operation named, so
//! driver text reaches the log and not the client (§4.2, §5.5).

mod pg;

#[cfg(test)]
mod memory;

#[cfg(test)]
pub use memory::InMemoryAuthRepo;
pub use pg::PgAuthRepo;

use async_trait::async_trait;
use meno_core::Error as MenoError;
use uuid::Uuid;

use super::model::{
    AuthProvider, AuthSession, NewSession, NewUser, OtpType, RefreshToken, Rotation, SessionLookup,
    User,
};

/// Postgres-backed storage for the auth module.
///
/// # Why `async_trait`
///
/// `dyn AuthRepo` needs an object-safe async method. Rust's native `async fn` in traits
/// is not object safe, and `async_trait` boxes one future per call. At this module's
/// call volume the allocation is not worth an unstable feature or a macro per method.
#[async_trait]
pub trait AuthRepo: Send + Sync + std::fmt::Debug {
    // ── users ──────────────────────────────────────────────────────────────

    /// A live account by email address.
    ///
    /// `email` is expected already normalised by
    /// [`User::normalized_email`](super::model::User::normalized_email).
    async fn find_user_by_email(&self, email: &str) -> Result<Option<User>, MenoError>;

    /// A live account by id.
    async fn find_user_by_id(&self, id: Uuid) -> Result<Option<User>, MenoError>;

    /// Create an account and its email identity atomically.
    ///
    /// Atomic because the alternative is an account row with no identity row, which can
    /// then never sign in and can never be deleted cleanly — the two are separated by
    /// no foreign key in that direction.
    async fn create_user(&self, new: NewUser) -> Result<User, MenoError>;

    /// The password hash for an account, or `None` if it has none.
    ///
    /// Separate from [`Self::find_user_by_email`] so the login path does not read the
    /// whole `users` row to answer one question, and so a future non-password provider
    /// cannot accidentally be given a `NULL` hash to verify against.
    async fn find_password_hash(&self, email: &str) -> Result<Option<String>, MenoError>;

    /// Set an account's password hash, rotating every live session.
    ///
    /// The session revocation is part of the same statement's transaction and not the
    /// caller's job: a password reset that leaves the old device signed in is a
    /// vulnerability, and making it a separate call makes it a bug waiting to happen.
    async fn set_password_hash(&self, user_id: Uuid, hash: &str) -> Result<(), MenoError>;

    /// Mark an account's email as verified.
    async fn set_email_verified(&self, user_id: Uuid) -> Result<bool, MenoError>;

    /// Which providers an account can sign in with.
    async fn list_providers(&self, user_id: Uuid) -> Result<Vec<AuthProvider>, MenoError>;

    /// Soft-delete an account: set `deleted_at`, leaving the row in place.
    ///
    /// Soft rather than hard because broadcasts, chat messages and notes reference the
    /// user, and the partial unique index on `users.email` deliberately releases the
    /// address on delete. Every read path filters on `deleted_at IS NULL`, which is what
    /// makes releasing the address safe.
    async fn soft_delete(&self, user_id: Uuid) -> Result<bool, MenoError>;

    // ── sessions (plan §4.7 items 1-3) ────────────────────────────────────

    /// Create a session bound to a freshly issued refresh token.
    async fn create_session(&self, new: NewSession) -> Result<AuthSession, MenoError>;

    /// Atomically replace `current_jti`'s session with one carrying `replacement`.
    ///
    /// The single `UPDATE` followed by an `INSERT`, or the reverse, is the security
    /// boundary: two concurrent refreshes with the same token must not both succeed. A
    /// caller that checks liveness and then rotates is two round trips, and the second
    /// one wins.
    async fn rotate_session(
        &self,
        user_id: Uuid,
        current_jti: Uuid,
        replacement: NewSession,
    ) -> Result<Rotation, MenoError>;

    /// Revoke the live session carrying `jti`, if it belongs to `user_id`.
    ///
    /// Keyed on the refresh `jti` rather than the session id because that is all logout
    /// has: §4.7's endpoint revokes by session id, but `POST /auth/logout` is presented
    /// a token and has to find the device behind it. Without this, logging out leaves
    /// the session row live and the device still listed under "your devices".
    ///
    /// Returns whether a row changed, so a second logout is a no-op rather than an error.
    async fn revoke_session_for_jti(&self, jti: Uuid, user_id: Uuid) -> Result<bool, MenoError>;

    /// Revoke one session, if it belongs to `user_id`.
    ///
    /// Returns [`SessionLookup::NotFound`] for both "no such session" and "not yours" so
    /// the endpoint cannot be used to enumerate session ids.
    async fn revoke_session(
        &self,
        user_id: Uuid,
        session_id: Uuid,
    ) -> Result<SessionLookup, MenoError>;

    /// Revoke every live session for a user. Returns how many were ended.
    ///
    /// This is what reuse detection fires, and what "log out all devices" calls.
    async fn revoke_all_sessions(&self, user_id: Uuid) -> Result<u64, MenoError>;

    /// Every live session for a user, newest first.
    async fn list_sessions(&self, user_id: Uuid) -> Result<Vec<AuthSession>, MenoError>;

    // ── refresh tokens ─────────────────────────────────────────────────────

    /// Record that a refresh token was issued.
    async fn store_refresh_token(
        &self,
        jti: Uuid,
        user_id: Uuid,
        token_hash: &str,
        expires_at: time::OffsetDateTime,
    ) -> Result<(), MenoError>;

    /// Find an issued-but-not-spent refresh token by its `jti`.
    async fn find_refresh_token(
        &self,
        jti: Uuid,
        user_id: Uuid,
    ) -> Result<Option<RefreshToken>, MenoError>;

    /// Mark a refresh token spent, returning whether it was still live.
    async fn consume_refresh_token(&self, jti: Uuid, user_id: Uuid) -> Result<bool, MenoError>;

    /// Mark a refresh token dead without consuming it.
    ///
    /// Distinct from [`Self::consume_refresh_token`] because the two have different
    /// meanings. "Consumed" is the *rotation* path: the token was swapped for a new
    /// session, and presenting it again is reuse. "Revoked" is the *logout* path: the
    /// token is simply finished with, and presenting it again is a client retry, which
    /// §4.7's logout has to tolerate.
    ///
    /// `user_id` is in the predicate so a caller cannot revoke another account's token
    /// by guessing a `jti`. Returns whether a row changed, which is `false` for a token
    /// that was already gone — the answer logout treats as success.
    async fn revoke_refresh_token(&self, jti: Uuid, user_id: Uuid) -> Result<bool, MenoError>;

    // ── one-time codes ────────────────────────────────────────────────────

    /// Issue a code, replacing any live one of the same kind for the same address.
    async fn save_otp(
        &self,
        email: &str,
        kind: OtpType,
        code: &str,
        expires_at: time::OffsetDateTime,
    ) -> Result<(), MenoError>;

    /// Spend a code if it is live, unexpired and of the right kind.
    ///
    /// Returns `false` for every failure — wrong code, already used, wrong kind,
    /// expired — so the caller cannot tell them apart and turn the endpoint into an
    /// oracle. The `used` flip is conditional in the `UPDATE` for the same reason: a
    /// read-then-write would let two concurrent requests both succeed.
    async fn consume_otp(
        &self,
        email: &str,
        kind: OtpType,
        code: &str,
        now: time::OffsetDateTime,
    ) -> Result<bool, MenoError>;

    // ── provider linking (plan §4.7 item 7) ───────────────────────────────

    /// Find an account by a provider's subject id.
    async fn find_user_by_provider(
        &self,
        provider: AuthProvider,
        provider_user_id: &str,
    ) -> Result<Option<User>, MenoError>;

    /// Link a provider identity to an existing account.
    async fn link_provider(
        &self,
        user_id: Uuid,
        provider: AuthProvider,
        provider_user_id: &str,
    ) -> Result<User, MenoError>;

    /// Create an account from a verified provider identity.
    async fn create_user_from_provider(
        &self,
        new: NewUser,
        provider: AuthProvider,
        provider_user_id: &str,
    ) -> Result<User, MenoError>;
}

/// Everything needed to build an [`AuthRepo`], in one place (plan §4.1).
///
/// A struct rather than a positional argument list, because a six-argument `new` is
/// where two same-typed dependencies get swapped at a call site and nothing complains.
#[derive(Debug, Clone)]
pub struct RepoDeps {
    /// The pool to read and write through.
    pub pool: sqlx::PgPool,
}

#[cfg(test)]
mod tests {
    //! The trait's own contract, checked against [`InMemoryAuthRepo`].
    //!
    //! These are the properties a test double has to honour for the service tests to
    //! mean anything. A double that quietly diverges from Postgres is worse than no
    //! double, because the service tests then pass and production fails.

    use super::super::model::DeviceContext;
    use super::*;
    use time::{Duration, OffsetDateTime};
    use uuid::Uuid;

    fn user_id() -> Uuid {
        Uuid::new_v4()
    }

    fn new_session(user: Uuid) -> NewSession {
        NewSession {
            user_id: user,
            refresh_jti: Uuid::new_v4(),
            token_hash: "hash".to_owned(),
            expires_at: OffsetDateTime::now_utc() + Duration::days(30),
            device: DeviceContext::default(),
        }
    }

    /// A repo with a session already issued, and its `jti`.
    async fn repo_with_session() -> (InMemoryAuthRepo, Uuid, Uuid) {
        let repo = InMemoryAuthRepo::new();
        let user = user_id();
        let session = new_session(user);
        let jti = session.refresh_jti;
        repo.create_session(session)
            .await
            .expect("creating a session");
        (repo, user, jti)
    }

    #[tokio::test]
    async fn a_freshly_created_session_can_be_rotated_exactly_once() {
        // The core of §4.7 item 2. A second rotation of the same jti is `Replayed`,
        // which is what distinguishes theft from a typo.
        let (repo, user, jti) = repo_with_session().await;
        let replacement = new_session(user);

        let first = repo
            .rotate_session(user, jti, replacement)
            .await
            .expect("the first rotation succeeds");
        assert!(matches!(first, Rotation::Rotated(_)));

        let second = repo
            .rotate_session(user, jti, new_session(user))
            .await
            .expect("the second rotation is answered, not an error");
        assert_eq!(
            second,
            Rotation::Replayed,
            "a rotated jti presented again is reuse, not an unknown token"
        );
    }

    #[tokio::test]
    async fn a_jti_from_another_user_is_unknown_rather_than_replayed() {
        // The distinction §4.7 item 2 rests on: someone else's token is not evidence of
        // theft *of yours*, and revoking every session on that signal would let one
        // attacker sign out every user they can reach.
        let (repo, owner, jti) = repo_with_session().await;
        let stranger = user_id();

        assert_eq!(
            repo.rotate_session(stranger, jti, new_session(stranger))
                .await
                .expect("answered"),
            Rotation::Unknown
        );
        assert_ne!(Rotation::Unknown, Rotation::Replayed);
        assert!(owner != stranger);
    }

    #[tokio::test]
    async fn rotation_links_the_new_session_to_the_one_it_superseded() {
        // `rotated_from` is the audit trail for "which device chain is this", and
        // without it reuse detection has no history to point at.
        let (repo, user, jti) = repo_with_session().await;
        let replacement = new_session(user);

        let Rotation::Rotated(session) = repo
            .rotate_session(user, jti, replacement.clone())
            .await
            .expect("rotation")
        else {
            panic!("expected a rotation");
        };

        assert_eq!(session.user_id, user);
        assert_eq!(session.refresh_jti, replacement.refresh_jti);
        assert!(session.rotated_from.is_some(), "the chain link is missing");
    }

    #[tokio::test]
    async fn a_rotated_session_is_no_longer_live() {
        let (repo, user, jti) = repo_with_session().await;
        let before = repo.list_sessions(user).await.expect("listing");
        assert_eq!(before.len(), 1);

        repo.rotate_session(user, jti, new_session(user))
            .await
            .expect("rotation");

        let after = repo.list_sessions(user).await.expect("listing");
        assert_eq!(after.len(), 1, "rotation replaces, it does not accumulate");
    }

    #[tokio::test]
    async fn revoking_all_sessions_ends_only_that_users_sessions() {
        // Reuse detection fires this. If it touched other accounts, one stolen token
        // would be a denial-of-service against the whole deployment, so both users live
        // in one repo and the bystander's session must survive untouched.
        let repo = InMemoryAuthRepo::new();
        let victim = user_id();
        let bystander = user_id();

        repo.create_session(new_session(victim))
            .await
            .expect("victim session");
        repo.create_session(new_session(bystander))
            .await
            .expect("bystander session");

        let ended = repo
            .revoke_all_sessions(victim)
            .await
            .expect("revoking all");

        assert_eq!(ended, 1, "exactly the victim's session was ended");
        assert!(
            repo.list_sessions(victim)
                .await
                .expect("listing")
                .is_empty()
        );
        assert_eq!(
            repo.list_sessions(bystander).await.expect("listing").len(),
            1,
            "the bystander's session must survive"
        );
    }

    #[tokio::test]
    async fn revoking_a_session_only_works_for_its_owner() {
        // §4.7 item 3's endpoint must not accept a session id belonging to someone else.
        let (repo, owner, _jti) = repo_with_session().await;
        let live = repo.list_sessions(owner).await.expect("listing");
        let session_id = live[0].id;
        let stranger = user_id();

        assert_eq!(
            repo.revoke_session(stranger, session_id)
                .await
                .expect("revoking"),
            SessionLookup::NotFound
        );
        assert_eq!(
            repo.revoke_session(owner, session_id)
                .await
                .expect("revoking"),
            SessionLookup::Found
        );
        assert!(
            repo.list_sessions(owner).await.expect("listing").is_empty(),
            "the owner's revoke actually revoked"
        );
    }

    #[tokio::test]
    async fn an_unknown_session_id_is_not_found() {
        let repo = InMemoryAuthRepo::new();
        let user = user_id();
        assert_eq!(
            repo.revoke_session(user, Uuid::new_v4())
                .await
                .expect("revoking"),
            SessionLookup::NotFound
        );
    }

    #[tokio::test]
    async fn a_one_time_code_can_be_spent_only_once() {
        // The `used` flip is conditional in the real repository; the double has to
        // behave the same way or the service tests would not be testing anything.
        let repo = InMemoryAuthRepo::new();
        let now = OffsetDateTime::now_utc();

        repo.save_otp(
            "ada@example.com",
            OtpType::VerifyEmail,
            "123456",
            now + Duration::minutes(10),
        )
        .await
        .expect("saving a code");

        assert!(
            repo.consume_otp("ada@example.com", OtpType::VerifyEmail, "123456", now)
                .await
                .expect("consuming")
        );
        assert!(
            !repo
                .consume_otp("ada@example.com", OtpType::VerifyEmail, "123456", now)
                .await
                .expect("consuming"),
            "a spent code must not work twice"
        );
    }

    #[tokio::test]
    async fn a_code_of_the_wrong_kind_is_refused() {
        // A reset code must not verify an email. Sharing one table for both kinds is
        // only safe if the kind is part of the lookup.
        let repo = InMemoryAuthRepo::new();
        let now = OffsetDateTime::now_utc();
        repo.save_otp(
            "ada@example.com",
            OtpType::ResetPassword,
            "654321",
            now + Duration::minutes(10),
        )
        .await
        .expect("saving");

        assert!(
            !repo
                .consume_otp("ada@example.com", OtpType::VerifyEmail, "654321", now)
                .await
                .expect("consuming")
        );
    }

    #[tokio::test]
    async fn an_expired_code_is_refused() {
        let repo = InMemoryAuthRepo::new();
        let issued = OffsetDateTime::now_utc();
        repo.save_otp(
            "ada@example.com",
            OtpType::VerifyEmail,
            "111111",
            issued + Duration::minutes(10),
        )
        .await
        .expect("saving");

        assert!(
            !repo
                .consume_otp(
                    "ada@example.com",
                    OtpType::VerifyEmail,
                    "111111",
                    issued + Duration::minutes(11)
                )
                .await
                .expect("consuming"),
            "a code past its expiry must be refused"
        );
    }

    #[tokio::test]
    async fn issuing_a_second_code_replaces_the_first() {
        // Otherwise the first code stays usable and "resend" does not mean what the
        // button says it means.
        let repo = InMemoryAuthRepo::new();
        let now = OffsetDateTime::now_utc();
        repo.save_otp(
            "ada@example.com",
            OtpType::VerifyEmail,
            "111111",
            now + Duration::minutes(10),
        )
        .await
        .expect("first");
        repo.save_otp(
            "ada@example.com",
            OtpType::VerifyEmail,
            "222222",
            now + Duration::minutes(10),
        )
        .await
        .expect("second");

        assert!(
            !repo
                .consume_otp("ada@example.com", OtpType::VerifyEmail, "111111", now)
                .await
                .expect("consuming")
        );
        assert!(
            repo.consume_otp("ada@example.com", OtpType::VerifyEmail, "222222", now)
                .await
                .expect("consuming")
        );
    }

    #[tokio::test]
    async fn setting_a_password_hash_revokes_sessions_in_the_same_call() {
        let (repo, user, _jti) = repo_with_session().await;

        repo.set_password_hash(user, "$argon2id$new")
            .await
            .expect("setting");
        assert!(
            repo.list_sessions(user).await.expect("listing").is_empty(),
            "a password reset must end every live session"
        );
    }

    #[tokio::test]
    async fn a_password_hash_is_only_returned_for_the_address_that_has_one() {
        let repo = InMemoryAuthRepo::new();
        let user = repo
            .create_user(NewUser {
                full_name: "Ada Lovelace".to_owned(),
                email: "ada@example.com".to_owned(),
                password_hash: "$argon2id$hash".to_owned(),
            })
            .await
            .expect("creating");

        assert_eq!(
            repo.find_password_hash("ada@example.com")
                .await
                .expect("looking up")
                .as_deref(),
            Some("$argon2id$hash")
        );
        assert!(
            repo.find_password_hash("nobody@example.com")
                .await
                .expect("looking up")
                .is_none()
        );
        assert!(!user.verified);
    }

    #[tokio::test]
    async fn setting_an_email_verified_twice_reports_false_the_second_time() {
        // The second call is a 409 at the service layer, and it can only be a 409 if
        // the repository says the state did not change.
        let repo = InMemoryAuthRepo::new();
        let user = repo
            .create_user(NewUser {
                full_name: "Ada Lovelace".to_owned(),
                email: "ada@example.com".to_owned(),
                password_hash: "$argon2id$hash".to_owned(),
            })
            .await
            .expect("creating");

        assert!(repo.set_email_verified(user.id).await.expect("verifying"));
        assert!(
            !repo
                .set_email_verified(user.id)
                .await
                .expect("verifying again")
        );
        assert!(
            repo.find_user_by_id(user.id)
                .await
                .expect("reading")
                .expect("row")
                .verified
        );
    }

    #[tokio::test]
    async fn provider_linking_is_reported_by_provider_and_subject() {
        let repo = InMemoryAuthRepo::new();
        let user = repo
            .create_user(NewUser {
                full_name: "Ada Lovelace".to_owned(),
                email: "ada@example.com".to_owned(),
                password_hash: "$argon2id$hash".to_owned(),
            })
            .await
            .expect("creating");

        assert!(
            repo.find_user_by_provider(AuthProvider::Google, "google-subject")
                .await
                .expect("looking up")
                .is_none()
        );

        repo.link_provider(user.id, AuthProvider::Google, "google-subject")
            .await
            .expect("linking");

        let found = repo
            .find_user_by_provider(AuthProvider::Google, "google-subject")
            .await
            .expect("looking up")
            .expect("the link is findable");
        assert_eq!(found.id, user.id);

        assert!(
            repo.list_providers(user.id).await.expect("listing").len() >= 2,
            "a linked provider joins the password one"
        );
    }

    #[tokio::test]
    async fn a_provider_only_account_has_no_password() {
        // §4.7 item 7's account creation path. An account created from a Google sign-in
        // must never be handed a `NULL` hash to verify a password against.
        let repo = InMemoryAuthRepo::new();
        let user = repo
            .create_user_from_provider(
                NewUser {
                    full_name: "Grace Hopper".to_owned(),
                    email: "grace@example.com".to_owned(),
                    password_hash: String::new(),
                },
                AuthProvider::Google,
                "google-subject-2",
            )
            .await
            .expect("creating");

        assert!(user.verified, "a verified provider email is verified");
        assert_eq!(
            repo.find_password_hash("grace@example.com")
                .await
                .expect("looking up"),
            None
        );
        assert_eq!(
            repo.list_providers(user.id).await.expect("listing"),
            vec![AuthProvider::Google]
        );
    }

    #[tokio::test]
    async fn a_refresh_token_is_recorded_and_spent_once() {
        let repo = InMemoryAuthRepo::new();
        let user = user_id();
        let jti = Uuid::new_v4();
        let expires_at = OffsetDateTime::now_utc() + Duration::days(30);

        repo.store_refresh_token(jti, user, "hash", expires_at)
            .await
            .expect("storing");

        assert!(
            repo.consume_refresh_token(jti, user)
                .await
                .expect("consuming")
        );
        assert!(
            !repo
                .consume_refresh_token(jti, user)
                .await
                .expect("consuming"),
            "a refresh token is single-use"
        );
    }

    #[tokio::test]
    async fn a_refresh_token_belonging_to_another_user_cannot_be_spent() {
        let repo = InMemoryAuthRepo::new();
        let owner = user_id();
        let thief = user_id();
        let jti = Uuid::new_v4();

        repo.store_refresh_token(
            jti,
            owner,
            "hash",
            OffsetDateTime::now_utc() + Duration::days(30),
        )
        .await
        .expect("storing");

        assert!(
            !repo
                .consume_refresh_token(jti, thief)
                .await
                .expect("consuming"),
            "the user id is part of the lookup, not just the jti"
        );
    }

    #[tokio::test]
    async fn a_soft_deleted_account_is_not_findable() {
        // The `users` table has a partial unique index on email for live rows, which
        // only means anything if lookups honour `deleted_at`.
        let repo = InMemoryAuthRepo::new();
        let user = repo
            .create_user(NewUser {
                full_name: "Ada Lovelace".to_owned(),
                email: "ada@example.com".to_owned(),
                password_hash: "$argon2id$hash".to_owned(),
            })
            .await
            .expect("creating");

        assert!(
            repo.find_user_by_email("ada@example.com")
                .await
                .expect("looking up")
                .is_some()
        );

        repo.soft_delete(user.id).await.expect("deleting");

        assert!(
            repo.find_user_by_email("ada@example.com")
                .await
                .expect("looking up")
                .is_none()
        );
        assert!(
            repo.find_user_by_id(user.id)
                .await
                .expect("looking up")
                .is_none()
        );
    }

    #[tokio::test]
    async fn two_accounts_cannot_share_an_email() {
        // The unique index is on live rows, so the second insert has to fail — and it
        // has to fail as a `Conflict`, not as a 500.
        let repo = InMemoryAuthRepo::new();
        let template = NewUser {
            full_name: "Ada Lovelace".to_owned(),
            email: "ada@example.com".to_owned(),
            password_hash: "$argon2id$hash".to_owned(),
        };
        repo.create_user(template.clone()).await.expect("first");

        let second = repo
            .create_user(template)
            .await
            .expect_err("a duplicate email is refused");
        assert_eq!(second.code(), super::super::error::email_taken().code());
    }
}
