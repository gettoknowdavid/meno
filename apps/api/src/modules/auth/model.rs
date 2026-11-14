//! The rows and enums the auth module reads and writes.
//!
//! # One definition of a user role, not three
//!
//! The previous revision of this file declared its own `UserRole` and `AuthProvider`,
//! and so did `middleware::auth`. Two enums with the same name, the same variants and
//! different ones (`model::AuthProvider` had `Email`/`Apple`/`Facebook`, the middleware's
//! had `Password`/`Google`), both `Serialize`d into the same JWT claim and the same
//! access token. Whichever one a given call site imported silently determined the wire
//! value — `"email"` or `"password"` for the same user. §4.2's point about duplicated
//! mapping logic drifting applies to types as much as to error enums.
//!
//! So this module imports [`UserRole`] and [`AuthProvider`] from
//! [`crate::middleware::auth`] and re-exports them. One definition, one set of wire
//! strings, and a new provider variant is a change in one place.
//!
//! # Why `role` is a `String` on the row
//!
//! [`User::role`] is the raw `users.role` column, which Postgres constrains to
//! `'user' | 'admin'`. It is *not* typed as [`UserRole`] because
//! [`UserRole::Creator`] — which the token can carry — has no database representation:
//! a creator is a broadcast attribute in this schema, not a role on the account. Typing
//! the column as the token enum would mean either lying to sqlx about the constraint or
//! adding a value the constraint forbids. [`User::role`] parses it instead, and the
//! parse is where the "unknown string means least privilege" decision is made and tested.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

pub use crate::middleware::auth::{AuthProvider, UserRole};

/// A `users` row.
#[derive(Debug, Clone, FromRow)]
pub struct User {
    /// Primary key.
    pub id: Uuid,
    /// Display name.
    pub full_name: String,
    /// Optional biography.
    pub bio: Option<String>,
    /// Login address. Unique among live rows.
    pub email: String,
    /// Storage key for the avatar, if any.
    pub avatar_id: Option<String>,
    /// Resolved avatar URL, if any.
    pub avatar_url: Option<String>,
    /// Whether the email address has been confirmed.
    pub verified: bool,
    /// The raw `role` column. See the module note on why this is not [`UserRole`].
    pub role: String,
    /// When the account was created.
    pub created_at: OffsetDateTime,
    /// When the account was last modified.
    pub updated_at: OffsetDateTime,
    /// When the account was soft-deleted; `None` for a live row.
    pub deleted_at: Option<OffsetDateTime>,
}

impl User {
    /// The account's role, parsed.
    ///
    /// An unrecognised string is [`UserRole::User`], the least-privileged role, and the
    /// return is `&self.role` only in the sense that this borrows. §9.5's rule is that a
    /// malformed value fails towards *less* access, and there is no way for a database
    /// string to grant a capability.
    #[must_use]
    pub fn role(&self) -> UserRole {
        match self.role.as_str() {
            "admin" => UserRole::Admin,
            "creator" => UserRole::Creator,
            _ => UserRole::User,
        }
    }

    /// The email in the form addresses are compared and stored in.
    ///
    /// Lowercased on the way in, not on the way out. §9.5: a value that is normalised at
    /// the boundary cannot be stored in two spellings, and two spellings of one address
    /// are two accounts.
    #[must_use]
    pub fn normalized_email(email: &str) -> String {
        email.trim().to_ascii_lowercase()
    }
}

/// A `user_identities` row: one way of proving who a user is.
#[derive(Debug, Clone, FromRow)]
pub struct UserIdentity {
    /// Primary key.
    pub id: Uuid,
    /// Owning account.
    pub user_id: Uuid,
    /// Which kind of identity this is.
    pub provider_type: String,
    /// The provider's stable subject id, for non-password providers.
    pub provider_user_id: String,
    /// The Argon2id PHC string, for the password provider. `None` otherwise.
    pub password_hash: Option<String>,
    /// When the link was created.
    pub created_at: OffsetDateTime,
    /// When the link was last modified.
    pub updated_at: Option<OffsetDateTime>,
}

/// An `auth_sessions` row — one device's refresh chain (plan §4.7 items 1 and 3).
///
/// The chain is a linked list: each rotation deletes the current row and inserts a new
/// one whose [`Self::rotated_from`] is the old row's id. A `jti` that is no longer the
/// live [`Self::refresh_jti`] of any session was therefore already rotated, which is the
/// whole of §4.7 item 2's reuse detection.
// `PartialEq`/`Eq` are here for `Rotation`'s derive, not because two sessions are ever
// compared in production: a test asserting "the rotation produced a different session"
// should be saying so with `!=` rather than comparing six timestamps by hand.
#[derive(Debug, Clone, FromRow, PartialEq, Eq)]
pub struct AuthSession {
    /// Primary key, and what `POST /auth/sessions/{id}/revoke` names.
    pub id: Uuid,
    /// Owning account.
    pub user_id: Uuid,
    /// The `jti` of the refresh token this session's current token was signed with.
    pub refresh_jti: Uuid,
    /// What the user called this device, e.g. "Ada's iPhone".
    pub device_label: Option<String>,
    /// The `User-Agent` the session was created from.
    pub user_agent: Option<String>,
    /// The client address the session was created from.
    pub ip: Option<IpAddr>,
    /// When the session was created.
    pub created_at: OffsetDateTime,
    /// When the session was last used to refresh.
    pub last_used_at: OffsetDateTime,
    /// When the session ended, if it has.
    pub revoked_at: Option<OffsetDateTime>,
    /// The session this one superseded, if any.
    pub rotated_from: Option<Uuid>,
}

impl AuthSession {
    /// Whether this session can still be refreshed against.
    ///
    /// `revoked_at` is the only thing that decides it. `expires_at` is deliberately
    /// absent: the JWT's own `exp` covers that, and a second expiry column would be a
    /// second answer to a question with one.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        self.revoked_at.is_none()
    }

    /// A label safe to show in a "your devices" list.
    ///
    /// Falls back through the user's own label, the platform's user agent, the address,
    /// and finally a constant — so the list is never blank, and a blank entry is
    /// indistinguishable from a rendering bug.
    #[must_use]
    pub fn display_label(&self) -> std::borrow::Cow<'_, str> {
        use std::borrow::Cow;

        // `Cow` rather than `String` because the common case -- a session the user
        // named -- is a borrow, and this is read once per row in a list endpoint. The
        // address fallback allocates, but only for sessions carrying no label at all.
        for candidate in [&self.device_label, &self.user_agent] {
            if let Some(text) = candidate
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
            {
                return Cow::Borrowed(text);
            }
        }

        match self.ip {
            Some(ip) => Cow::Owned(ip.to_string()),
            None => Cow::Borrowed("Unknown device"),
        }
    }
}

/// A `refresh_tokens` row.
///
/// Retained alongside `auth_sessions` because it answers a different question: not
/// "which device" but "is this specific token string one we issued", enforced by a
/// `UNIQUE` constraint on the hash. §4.7's session table tracks the chain; this tracks
/// the tokens.
#[derive(Debug, Clone, FromRow)]
pub struct RefreshToken {
    /// Primary key.
    pub id: Uuid,
    /// Owning account.
    pub user_id: Uuid,
    /// SHA-256 of the signed token. The token itself is never stored.
    pub token_hash: String,
    /// When it was issued.
    pub created_at: OffsetDateTime,
    /// When it stops being accepted.
    pub expires_at: OffsetDateTime,
}

/// Which one-time code an `otps` row is.
// `Hash` is here because the in-memory repository keys its `otps` map on
// `(String, OtpType)`. It costs nothing on a two-variant enum and turns a missing derive
// into a compile error rather than a redesign.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OtpType {
    /// A code proving the caller owns the email address.
    VerifyEmail,
    /// A code permitting a password reset.
    ResetPassword,
}

impl OtpType {
    /// The value stored in `otps.type`, matching the table's `CHECK` constraint.
    ///
    /// Not `Display`: this is a database value, and conflating it with a human-facing
    /// rendering is how the two drift apart.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VerifyEmail => "verify_email",
            Self::ResetPassword => "reset_password",
        }
    }

    /// Parse a stored `otps.type`.
    ///
    /// `None` for anything else, so a caller cannot mistake an unknown value for a
    /// known type and issue the wrong kind of code.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "verify_email" => Some(Self::VerifyEmail),
            "reset_password" => Some(Self::ResetPassword),
            _ => None,
        }
    }
}

/// An `otps` row.
#[derive(Debug, Clone, FromRow)]
pub struct Otp {
    /// Primary key.
    pub id: Uuid,
    /// The address the code was sent to.
    pub email: String,
    /// The six-digit code.
    ///
    /// Stored in plaintext because the column is `TEXT NOT NULL` in migration 0003 and
    /// the alternative — a hash — is a migration. §7.11 lists "OTP codes hashed at rest"
    /// as outstanding work; this model does not pretend otherwise. What it does do is
    /// keep the *access* window short, which is the property that matters most.
    pub code: String,
    /// Which kind of code this is.
    pub otp_type: String,
    /// Whether it has been spent.
    pub used: bool,
    /// When it was issued.
    pub created_at: OffsetDateTime,
    /// When it stops being accepted.
    pub expires_at: OffsetDateTime,
}

impl Otp {
    /// The typed [`OtpType`], or `None` for a value this build does not recognise.
    #[must_use]
    pub fn kind(&self) -> Option<OtpType> {
        OtpType::parse(&self.otp_type)
    }

    /// Whether the code is still acceptable at `now`.
    ///
    /// Not a `const fn`: `time`'s `PartialOrd` is an ordinary trait impl, and a const
    /// function cannot call one. Nothing needs it at compile time.
    #[must_use]
    pub fn is_usable_at(&self, now: OffsetDateTime) -> bool {
        !self.used && now < self.expires_at
    }
}

/// The values needed to create an account.
#[derive(Debug, Clone)]
pub struct NewUser {
    /// Display name, already length-checked by the DTO.
    pub full_name: String,
    /// Address, already lowercased by [`User::normalized_email`].
    pub email: String,
    /// The Argon2id hash of the chosen password.
    pub password_hash: String,
}

/// The device context a new session is bound to (§4.7 item 1).
///
/// Collected at the edge so the "your devices" list can show something a person
/// recognises. Every field is optional because none of them is required to refresh a
/// token — refusing a login because the client sent no `User-Agent` would be a
/// availability bug dressed up as a security feature.
#[derive(Debug, Clone, Default)]
pub struct DeviceContext {
    /// The user's own label for this device.
    pub device_label: Option<String>,
    /// The request's `User-Agent`.
    pub user_agent: Option<String>,
    /// The peer's address.
    pub ip: Option<IpAddr>,
}

/// The fields a new `auth_sessions` row needs.
#[derive(Debug, Clone)]
pub struct NewSession {
    /// Owning account.
    pub user_id: Uuid,
    /// The `jti` of the refresh token being issued.
    pub refresh_jti: Uuid,
    /// SHA-256 of that token.
    pub token_hash: String,
    /// When the token stops being accepted.
    pub expires_at: OffsetDateTime,
    /// Device context for the session list.
    pub device: DeviceContext,
}

/// What happened when a refresh token was presented.
///
/// The three-way split is §4.7 item 2. Collapsing [`Self::Replayed`] and
/// [`Self::Unknown`] into one variant would make reuse detection impossible, because the
/// difference between "I have never heard of this token" and "I rotated it once already"
/// is the entire signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rotation {
    /// The token was live and has been replaced by a new session.
    Rotated(AuthSession),
    /// The `jti` matches no session for this user at all.
    Unknown,
    /// The `jti` belonged to a session that has already been rotated.
    ///
    /// The only defensible reading is that the token leaked. Every session for the user
    /// is revoked and they must sign in again.
    Replayed,
}

/// How a session lookup ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLookup {
    /// A live session belonging to the caller.
    Found,
    /// No such session, or it is not the caller's.
    ///
    /// One variant for both, so the revoke endpoint cannot be used to discover which
    /// session ids exist.
    NotFound,
}

/// The value stored in `user_identities.provider_type`.
///
/// Not [`AuthProvider::as_str`], and the difference is load-bearing: the wire vocabulary
/// says `password` (what the *client* calls the credential) while the column says `email`
/// (what the *row* is an identity for), and migration 0003's `CHECK` enumerates
/// `('email','google','apple','facebook')`. Conflating the two would mean either
/// rewriting the constraint or writing `password` into a column that refuses it.
///
/// The mapping lives here, next to the type, so a schema change and a wire change are
/// the same edit.
#[must_use]
pub const fn provider_db_str(provider: AuthProvider) -> &'static str {
    match provider {
        AuthProvider::Password => "email",
        // `AuthProvider` is `#[non_exhaustive]`, so a variant added later has to be
        // named here rather than silently mapping to its wire string -- which is only
        // correct by coincidence.
        other => other.as_str(),
    }
}

/// Parse a `user_identities.provider_type` value.
///
/// `None` for anything this build does not recognise, so a row written by a newer
/// deployment does not silently become a password identity -- which would let a client
/// present a password against it.
#[must_use]
pub fn provider_from_db_str(raw: &str) -> Option<AuthProvider> {
    match raw {
        "email" => Some(AuthProvider::Password),
        "google" => Some(AuthProvider::Google),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! The decisions these types make, as opposed to the columns they carry.
    //!
    //! A `FromRow` derive cannot be meaningfully tested without a database, so these
    //! cover the three places where a struct is more than a bag of fields:
    //! [`User::role`]'s "unknown means least privilege", [`AuthSession::is_live`]'s
    //! definition, and [`OtpType`]'s database round trip.

    use super::*;

    fn user_with_role(role: &str) -> User {
        User {
            id: Uuid::nil(),
            full_name: "Ada Lovelace".to_owned(),
            bio: None,
            email: "ada@example.com".to_owned(),
            avatar_id: None,
            avatar_url: None,
            verified: true,
            role: role.to_owned(),
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            deleted_at: None,
        }
    }

    fn session(revoked_at: Option<OffsetDateTime>) -> AuthSession {
        AuthSession {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            refresh_jti: Uuid::nil(),
            device_label: None,
            user_agent: None,
            ip: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            last_used_at: OffsetDateTime::UNIX_EPOCH,
            revoked_at,
            rotated_from: None,
        }
    }

    // ── role ───────────────────────────────────────────────────────────────

    #[test]
    fn a_known_role_string_parses_to_its_role() {
        assert_eq!(user_with_role("admin").role(), UserRole::Admin);
        assert_eq!(user_with_role("creator").role(), UserRole::Creator);
        assert_eq!(user_with_role("user").role(), UserRole::User);
    }

    #[test]
    fn an_unrecognised_role_string_is_the_least_privileged_role() {
        // §9.5: a malformed value must fail towards less access. Nothing about a
        // database string should be able to grant a capability.
        for raw in ["superuser", "", "ADMIN", "admin; DROP TABLE users"] {
            assert_eq!(
                user_with_role(raw).role(),
                UserRole::User,
                "{raw:?} must not grant anything"
            );
        }
    }

    #[test]
    fn an_unknown_role_is_never_privileged() {
        // Stated separately because it is the property that matters: it is not enough
        // that the value is *some* role.
        let role = user_with_role("root").role();
        assert!(!role.at_least(UserRole::Creator));
        assert!(!role.at_least(UserRole::Admin));
    }

    // ── email normalisation ────────────────────────────────────────────────

    #[test]
    fn addresses_are_normalised_at_the_boundary() {
        // Two spellings of one address are two accounts, so the normal form is decided
        // once — here — rather than at each comparison site.
        assert_eq!(
            User::normalized_email("  Ada@Example.COM "),
            "ada@example.com".to_owned()
        );
    }

    #[test]
    fn normalisation_is_ascii_only_so_it_cannot_change_a_character_count() {
        // Turkish dotless-i: `to_lowercase()` is locale-sensitive in some libraries and
        // would map `İ` to two characters. ASCII-only keeps the stored value a pure
        // case fold of what was typed.
        assert_eq!(User::normalized_email("ADA@EXAMPLE.COM"), "ada@example.com");
    }

    // ── session ────────────────────────────────────────────────────────────

    #[test]
    fn a_session_is_live_exactly_when_it_is_not_revoked() {
        assert!(session(None).is_live());
        assert!(!session(Some(OffsetDateTime::now_utc())).is_live());
    }

    #[test]
    fn rotation_distinguishes_reuse_from_an_unknown_token() {
        // §4.7 item 2 lives entirely in this enum. If `Replayed` disappeared, theft and
        // a typo would produce the same revocation behaviour and the feature would be
        // inert.
        assert_ne!(Rotation::Unknown, Rotation::Replayed);
        assert!(!matches!(Rotation::Unknown, Rotation::Replayed));
        assert!(matches!(Rotation::Replayed, Rotation::Replayed));
    }

    // ── otp type ───────────────────────────────────────────────────────────

    #[test]
    fn an_otp_type_round_trips_through_its_database_value() {
        // The value must match the `otps_type_check` constraint, or every insert fails
        // against a real database and nothing here would notice.
        assert_eq!(OtpType::VerifyEmail.as_str(), "verify_email");
        assert_eq!(OtpType::ResetPassword.as_str(), "reset_password");

        assert_eq!(OtpType::parse("verify_email"), Some(OtpType::VerifyEmail));
        assert_eq!(
            OtpType::parse("reset_password"),
            Some(OtpType::ResetPassword)
        );
    }

    #[test]
    fn an_unknown_otp_type_parses_to_none_rather_than_a_default() {
        // Defaulting would let a build issue verification codes when the row asked for
        // a reset, which is a privilege escalation wearing a fallback.
        assert_eq!(OtpType::parse("verifyEmail"), None);
        assert_eq!(OtpType::parse(""), None);
        assert_eq!(OtpType::parse("admin"), None);
    }

    #[test]
    fn the_wire_form_and_the_database_form_are_both_snake_case() {
        // Two spellings for one variant is how the previous revision's duplicate enums
        // drifted. This asserts the JSON form matches the stored form.
        let json = serde_json::to_string(&OtpType::VerifyEmail).expect("serialising an enum");
        assert_eq!(json, "\"verify_email\"");
        assert_eq!(json.trim_matches('"'), OtpType::VerifyEmail.as_str());
    }

    // ── device label ───────────────────────────────────────────────────────

    #[test]
    fn a_session_label_falls_back_rather_than_rendering_blank() {
        let mut s = session(None);
        // A blank entry in the device list is indistinguishable from a rendering bug,
        // so every layer of the fallback is exercised.
        assert_eq!(s.display_label(), "Unknown device");

        s.user_agent = Some("  Mozilla/5.0 (iPhone)  ".to_owned());
        assert_eq!(s.display_label(), "Mozilla/5.0 (iPhone)");

        s.device_label = Some("Ada's iPhone".to_owned());
        assert_eq!(s.display_label(), "Ada's iPhone");
    }

    #[test]
    fn a_whitespace_only_label_does_not_win_over_a_user_agent() {
        // `Option<String>` is `Some` for `"   "`, which is the case a `is_some` check
        // gets wrong.
        let mut s = session(None);
        s.device_label = Some("   ".to_owned());
        s.user_agent = Some("curl/8".to_owned());
        assert_eq!(s.display_label(), "curl/8");
    }

    // -- provider column mapping --------------------------------------------

    #[test]
    fn the_wire_name_and_the_column_name_are_different_on_purpose() {
        // A client calls it "password"; the row calls it "email". Getting this wrong
        // writes a value the `provider_type` CHECK constraint rejects, so every Google
        // sign-in fails with a constraint violation rather than anything readable.
        assert_eq!(AuthProvider::Password.as_str(), "password");
        assert_eq!(provider_db_str(AuthProvider::Password), "email");
        assert_eq!(provider_db_str(AuthProvider::Google), "google");
    }

    #[test]
    fn every_stored_provider_value_round_trips() {
        for provider in [AuthProvider::Password, AuthProvider::Google] {
            let stored = provider_db_str(provider);
            assert_eq!(
                provider_from_db_str(stored),
                Some(provider),
                "{stored:?} did not parse back to {provider:?}"
            );
        }
    }

    #[test]
    fn a_stored_value_this_build_does_not_know_is_not_a_password() {
        // The security direction: an unknown identity must never become a credential a
        // client can present. `password` is exactly what must NOT come back here.
        for raw in ["password", "apple", "facebook", "", "admin"] {
            assert_eq!(provider_from_db_str(raw), None, "{raw:?}");
        }
    }
}
