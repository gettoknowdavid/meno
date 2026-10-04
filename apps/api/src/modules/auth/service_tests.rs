//! Contract tests for [`AuthService`], [`CredentialService`] and [`handlers`].
//!
//! # Why a sibling file
//!
//! §9.3 caps a file at ~400 lines, and `services.rs` is already near its limit without a
//! test module. Splitting the tests out rather than appending them keeps the production
//! file readable — a reader looking for "what does login do" should not have to scroll
//! past forty tests to find out.
//!
//! # What is asserted here, and why it is asserted *here*
//!
//! Every property below is one that can only be observed by driving the service through
//! its real collaborators. A mock would let each pass while the wiring was wrong, which
//! is the failure §5.6's trait-object arrangement exists to prevent.
//!
//! The four that matter most:
//!
//! - **No enumeration.** Login, resend and forgot-password answer identically for an
//!   unknown address, and login still does an Argon2id verification when no account
//!   matched. The timing half is asserted as a *ratio*, not a constant, because CI
//!   machines are noisy.
//! - **Device-bound sessions.** A refresh token belongs to the session it was issued
//!   to, "log out everywhere" ends them, and revoking one leaves the others alone.
//! - **Reuse detection.** A replayed refresh token comes back as a refusal *and* ends
//!   every session for that user — while a *forged* one ends nothing.
//! - **§7.12.** An unverified Google address never links to an existing account.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::{Request as HttpRequest, StatusCode, header};
use axum::routing::{get, post};
use http_body_util::BodyExt;
use tower::ServiceExt;

use super::cache::{AuthCache, InMemoryAuthCache};
use super::credentials::{CredentialDeps, CredentialService};
use super::dto::{
    ForgotPasswordRequest, GoogleMobileAuthRequest, LoginRequest, LogoutRequest,
    RefreshTokenRequest, RegisterRequest, ResendOtpRequest, SessionResponse, Validatable,
};
use super::error;
use super::google::{ProviderExchange, StubExchange};
use super::mailer::{AuthEmail, AuthMailer, RecordingAuthMailer};
use super::model::{AuthProvider, OtpType, UserRole};
use super::repository::{AuthRepo, InMemoryAuthRepo};
use super::services::{AuthDeps, AuthService};
use super::state::AuthState;
use super::token::{TokenConfig, TokenService};
use crate::config::Secret;
use crate::middleware::auth::AuthUser;
use meno_core::ErrorCode;

/// The password every fixture account is given.
///
/// A real password that also satisfies the composition policy — an all-lowercase
/// passphrase would be refused at `RegisterRequest` validation before the login tests
/// ever reached Argon2id, which is the path a timing assertion has to measure to mean
/// anything.
const PASSWORD: &str = "Correct horse battery staple";

/// Everything the module needs, with the doubles left concrete so a test can read them.
struct Harness {
    state: AuthState,
    service: AuthService,
    credentials: CredentialService,
    tokens: TokenService,
    repo: Arc<InMemoryAuthRepo>,
    mailer: Arc<RecordingAuthMailer>,
    google: Arc<StubExchange>,
}

fn token_config() -> TokenConfig {
    TokenConfig::validate(
        Secret::new("access-secret-for-tests"),
        Secret::new("refresh-secret-for-tests"),
        900,
        2_592_000,
    )
    .expect("a valid configuration")
}

fn harness() -> Harness {
    harness_with(StubExchange::exchanging(
        "google-subject",
        "ada@example.com",
    ))
}

fn harness_with(google: StubExchange) -> Harness {
    let repo = Arc::new(InMemoryAuthRepo::new());
    let cache = Arc::new(InMemoryAuthCache::new());
    // Concrete, so the assertions below can read what was sent; the `dyn` view handed
    // to the services is a clone of the same allocation, not a second double.
    let mailer = Arc::new(RecordingAuthMailer::new());
    let mailer_dyn: Arc<dyn AuthMailer> = Arc::clone(&mailer) as Arc<dyn AuthMailer>;
    let repo_trait: Arc<dyn AuthRepo> = Arc::clone(&repo) as Arc<dyn AuthRepo>;

    let tokens = TokenService::new(
        token_config(),
        Arc::clone(&repo_trait),
        Arc::clone(&cache) as Arc<dyn AuthCache>,
    )
    .expect("a valid token service");

    let service = AuthService::new(AuthDeps {
        repo: Arc::clone(&repo_trait),
        tokens: tokens.clone(),
        mailer: Arc::clone(&mailer_dyn),
    })
    .expect("a valid service");

    let credentials = CredentialService::new(CredentialDeps {
        repo: Arc::clone(&repo_trait),
        tokens: tokens.clone(),
        mailer: Arc::clone(&mailer_dyn),
    });

    let google = Arc::new(google);
    let state = AuthState::from_parts(
        repo_trait,
        cache as Arc<dyn AuthCache>,
        tokens.clone(),
        Arc::clone(&google) as Arc<dyn super::google::GoogleExchange>,
        mailer_dyn,
    )
    .expect("a valid state");

    Harness {
        state,
        service,
        credentials,
        tokens,
        repo,
        mailer,
        google,
    }
}

fn register_request() -> RegisterRequest {
    RegisterRequest {
        full_name: "Ada Lovelace".to_owned(),
        // Mixed case on purpose: §9.5 normalises at the boundary, so this must not
        // become a second account.
        email: "Ada@Example.com".to_owned(),
        password: PASSWORD.to_owned(),
    }
}

/// A registered, signed-in account, and the refresh token for its session.
async fn registered(h: &Harness) -> (super::model::User, String) {
    let response = h
        .service
        .register(&register_request())
        .await
        .expect("registering");

    // Registration returns a session, so a login test has something real to verify
    // against: `create_user` stores the hash the DTO's password was turned into.
    let user = h
        .repo
        .find_user_by_email("ada@example.com")
        .await
        .expect("looking up")
        .expect("the account exists");

    (user, response.refresh_token)
}

/// Sign a second device in, so a test has more than one session to reason about.
async fn second_device(h: &Harness, user: &super::model::User) {
    h.tokens
        .issue_pair(user, vec![AuthProvider::Password], Default::default())
        .await
        .expect("issuing for a second device");
}

// ── registration ───────────────────────────────────────────────────────────

#[tokio::test]
async fn registering_creates_the_account_mails_a_code_and_returns_a_session() {
    let h = harness();

    let response = h
        .service
        .register(&register_request())
        .await
        .expect("registering");

    assert!(!response.access_token.is_empty());
    assert_eq!(response.expires_in, 900, "expires_in is the configured TTL");
    // §9.5: the boundary normalises, so `Ada@Example.com` and `ada@example.com` are one
    // account rather than two.
    assert_eq!(response.user.email, "ada@example.com");
    assert!(!response.user.verified, "a new address is not yet proven");
    assert_eq!(response.user.providers, vec![AuthProvider::Password]);

    assert_eq!(h.mailer.count(), 1, "a verification code is sent");
    let mail = h.mailer.last().expect("a message");
    assert_eq!(mail.kind, AuthEmail::VerifyEmail);
    assert_eq!(mail.to, "ada@example.com");
    assert_eq!(mail.code.len(), 6, "six digits");
    assert!(mail.code.bytes().all(|byte| byte.is_ascii_digit()));
}

#[tokio::test]
async fn a_second_registration_for_the_same_address_is_refused() {
    let h = harness();
    h.service
        .register(&register_request())
        .await
        .expect("registering");

    let error = h
        .service
        .register(&register_request())
        .await
        .expect_err("the address is taken");

    assert_eq!(error.code(), ErrorCode::EmailTaken);
    assert_eq!(h.mailer.count(), 1, "no code for the refused attempt");
}

#[tokio::test]
async fn a_mail_failure_does_not_fail_the_registration() {
    // Best-effort mail: the account exists either way, and telling the client "we could
    // not send your code" would be free feedback about whether an address is free.
    let repo = Arc::new(InMemoryAuthRepo::new());
    let repo_trait: Arc<dyn AuthRepo> = Arc::clone(&repo) as Arc<dyn AuthRepo>;
    let tokens = TokenService::new(
        token_config(),
        Arc::clone(&repo_trait),
        Arc::new(InMemoryAuthCache::new()) as Arc<dyn AuthCache>,
    )
    .expect("a valid token service");

    let service = AuthService::new(AuthDeps {
        repo: repo_trait,
        tokens,
        mailer: Arc::new(RecordingAuthMailer::failing("smtp is down")),
    })
    .expect("a valid service");

    service
        .register(&register_request())
        .await
        .expect("registration succeeds despite the mail failure");

    assert!(
        repo.find_user_by_email("ada@example.com")
            .await
            .expect("looking up")
            .is_some(),
        "the account exists, so the client can retry 'resend'"
    );
}

// ── no enumeration ─────────────────────────────────────────────────────────

#[tokio::test]
async fn login_answers_identically_for_an_unknown_address() {
    let h = harness();
    h.service
        .register(&register_request())
        .await
        .expect("registering");

    let unknown = h
        .service
        .login(&LoginRequest {
            email: "nobody@example.com".to_owned(),
            password: "whatever they typed".to_owned(),
        })
        .await
        .expect_err("there is no such account");

    let wrong = h
        .service
        .login(&LoginRequest {
            email: "ada@example.com".to_owned(),
            password: "whatever they typed".to_owned(),
        })
        .await
        .expect_err("the password is wrong");

    assert_eq!(unknown.code(), wrong.code(), "same code");
    assert_eq!(
        unknown.to_string(),
        wrong.to_string(),
        "and the same message — a different one is an oracle"
    );
}

#[tokio::test]
async fn an_unknown_address_still_costs_a_verification() {
    // §7.9's timing half. The assertion is a ratio rather than a constant: CI machines
    // are noisy, but a login that short-circuits is three orders of magnitude faster,
    // which no amount of noise hides.
    let h = harness();
    h.service
        .register(&register_request())
        .await
        .expect("registering");

    let started = Instant::now();
    assert!(
        h.service
            .login(&LoginRequest {
                email: "nobody@example.com".to_owned(),
                password: "a guess".to_owned(),
            })
            .await
            .is_err()
    );
    let unknown_elapsed = started.elapsed();

    let started = Instant::now();
    assert!(
        h.service
            .login(&LoginRequest {
                email: "ada@example.com".to_owned(),
                password: "a guess".to_owned(),
            })
            .await
            .is_err()
    );
    let wrong_elapsed = started.elapsed();

    let ratio = unknown_elapsed.as_secs_f64() / wrong_elapsed.as_secs_f64().max(1e-9);
    assert!(
        (0.1..10.0).contains(&ratio),
        "an unknown address must cost what a wrong password costs: \
         unknown={unknown_elapsed:?} wrong={wrong_elapsed:?} ratio={ratio}"
    );
}

#[tokio::test]
async fn resend_and_forgot_password_answer_the_same_way_for_an_unknown_address() {
    let h = harness();

    let resend = h
        .credentials
        .resend_otp(&ResendOtpRequest {
            email: "nobody@example.com".to_owned(),
            otp_type: OtpType::VerifyEmail,
        })
        .await;
    let forgot = h
        .credentials
        .forgot_password(&ForgotPasswordRequest {
            email: "nobody@example.com".to_owned(),
        })
        .await;

    assert!(resend.is_ok() && forgot.is_ok(), "both succeed for nobody");
    assert_eq!(
        h.mailer.count(),
        0,
        "and neither sends anything, so the two are indistinguishable"
    );
}

#[tokio::test]
async fn a_password_reset_request_ends_every_session() {
    // The requester may not be the owner; leaving the old devices signed in would defeat
    // the point of resetting.
    let h = harness();
    let (user, _) = registered(&h).await;
    second_device(&h, &user).await;

    h.credentials
        .forgot_password(&ForgotPasswordRequest {
            email: "ada@example.com".to_owned(),
        })
        .await
        .expect("requesting a reset");

    assert!(
        h.repo
            .list_sessions(user.id)
            .await
            .expect("listing")
            .is_empty(),
        "a reset request ends every session"
    );
    assert_eq!(
        h.mailer.last().expect("sent").kind,
        AuthEmail::ResetPassword
    );
}

// ── login ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_correct_password_signs_in() {
    let h = harness();
    let (user, _) = registered(&h).await;

    let response = h
        .service
        .login(&LoginRequest {
            // Deliberately un-normalised: the boundary normalises, so the lookup finds
            // the same account.
            email: "ADA@example.com".to_owned(),
            password: PASSWORD.to_owned(),
        })
        .await
        .expect("signing in");

    assert_eq!(response.user.id, user.id);
    assert!(!response.access_token.is_empty());
}

#[tokio::test]
async fn the_session_list_names_a_device_and_counts_rotations() {
    let h = harness();
    let (user, first_refresh) = registered(&h).await;
    second_device(&h, &user).await;

    let before = h.service.list_sessions(user.id).await.expect("listing");
    assert_eq!(before.len(), 2);
    assert!(
        before.iter().all(|s| !s.device_label.is_empty()),
        "an unnamed session still needs something a person recognises"
    );

    // A refresh rotates the chain, so the new session records that it is not the first.
    h.service
        .refresh(&RefreshTokenRequest {
            refresh_token: first_refresh,
        })
        .await
        .expect("refreshing");

    let after = h.service.list_sessions(user.id).await.expect("listing");
    assert_eq!(after.len(), 2, "one in, one out");
    assert!(
        after.iter().any(|s| s.rotations > 0),
        "the rotated session says so"
    );
}

#[tokio::test]
async fn a_refresh_reports_the_configured_lifetime() {
    // §9.4 in miniature: the client is told the real TTL so it can refresh on its own,
    // rather than guessing.
    let h = harness();
    let (_, refresh) = registered(&h).await;

    let started = Instant::now();
    let response = h
        .service
        .refresh(&RefreshTokenRequest {
            refresh_token: refresh,
        })
        .await
        .expect("refreshing");

    assert_eq!(response.expires_in, 900);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "and refreshing does nothing unexpectedly slow"
    );
}

#[tokio::test]
async fn logging_out_twice_succeeds() {
    // A client that retries on a timeout must not be left permanently signed in.
    let h = harness();
    let (_, refresh) = registered(&h).await;

    let request = || LogoutRequest {
        refresh_token: refresh.clone(),
        access_token: None,
    };

    h.service.logout(&request()).await.expect("first logout");
    h.service.logout(&request()).await.expect("second logout");

    assert!(
        h.service
            .refresh(&RefreshTokenRequest {
                refresh_token: refresh
            })
            .await
            .is_err(),
        "and the token is genuinely dead"
    );
}

#[tokio::test]
async fn logging_out_with_a_token_that_was_never_issued_succeeds() {
    // Same reason: "your session is over" is the answer whether or not there was one.
    let h = harness();

    h.service
        .logout(&LogoutRequest {
            refresh_token: "not.a.jwt".to_owned(),
            access_token: None,
        })
        .await
        .expect("an unusable token is already logged out");
}

#[tokio::test]
async fn a_logged_out_device_leaves_the_device_list() {
    // §4.7 item 3 is about revoking a *device*. If logout only ended the token row, the
    // device would still be listed under "your devices" and still revokable.
    let h = harness();
    let (user, refresh) = registered(&h).await;
    second_device(&h, &user).await;

    h.service
        .logout(&LogoutRequest {
            refresh_token: refresh,
            access_token: None,
        })
        .await
        .expect("logging out");

    let remaining = h.service.list_sessions(user.id).await.expect("listing");
    assert_eq!(remaining.len(), 1, "exactly one device was ended");
}

#[tokio::test]
async fn revoking_a_session_leaves_the_others_alone() {
    let h = harness();
    let (user, _) = registered(&h).await;
    second_device(&h, &user).await;

    let sessions: Vec<SessionResponse> = h.service.list_sessions(user.id).await.expect("listing");
    assert_eq!(sessions.len(), 2);

    h.service
        .revoke_session(user.id, sessions[0].id)
        .await
        .expect("revoking");

    let remaining = h.service.list_sessions(user.id).await.expect("listing");
    assert_eq!(remaining.len(), 1);
}

#[tokio::test]
async fn revoking_someone_elses_session_reports_it_as_not_found() {
    // §4.7 item 3, and the enumeration question: "not yours" and "does not exist" must
    // be indistinguishable.
    let h = harness();
    let (user, _) = registered(&h).await;
    let sessions = h.service.list_sessions(user.id).await.expect("listing");
    let stranger = uuid::Uuid::new_v4();

    let theirs = h
        .service
        .revoke_session(stranger, sessions[0].id)
        .await
        .expect_err("not theirs");
    let invented = h
        .service
        .revoke_session(stranger, uuid::Uuid::new_v4())
        .await
        .expect_err("no such session");

    assert_eq!(theirs.code(), ErrorCode::NotFound);
    assert_eq!(theirs.code(), invented.code());
    assert_eq!(theirs.to_string(), invented.to_string());
}

// ── refresh and reuse detection (plan §4.7 item 2) ─────────────────────────

#[tokio::test]
async fn a_refresh_rotates_the_pair_and_retires_the_old_one() {
    let h = harness();
    let (user, refresh) = registered(&h).await;

    let response = h
        .service
        .refresh(&RefreshTokenRequest {
            refresh_token: refresh.clone(),
        })
        .await
        .expect("refreshing");

    assert_ne!(response.refresh_token, refresh);
    assert_eq!(
        h.repo.list_sessions(user.id).await.expect("listing").len(),
        1,
        "rotation replaces rather than adds"
    );
}

#[tokio::test]
async fn replaying_a_refresh_token_revokes_every_session() {
    // The headline of §4.7 item 2. A stolen token raced against the real client must
    // leave both parties signed out, which is the only outcome that reliably ends it.
    let h = harness();
    let (user, refresh) = registered(&h).await;

    h.service
        .refresh(&RefreshTokenRequest {
            refresh_token: refresh.clone(),
        })
        .await
        .expect("the legitimate refresh");

    // A second device signs in while the theft is unresolved.
    second_device(&h, &user).await;
    assert_eq!(
        h.repo.list_sessions(user.id).await.expect("listing").len(),
        2
    );

    let error = h
        .service
        .refresh(&RefreshTokenRequest {
            refresh_token: refresh,
        })
        .await
        .expect_err("the replay is refused");

    assert_eq!(
        error.code(),
        ErrorCode::InvalidToken,
        "deliberately the same code as any other unusable token — a distinct one would \
         tell a thief their guess was right"
    );
    assert!(
        h.repo
            .list_sessions(user.id)
            .await
            .expect("listing")
            .is_empty(),
        "every session for that user is gone"
    );
}

#[tokio::test]
async fn an_unknown_refresh_token_revokes_nothing() {
    // The other half of the distinction: "I have never heard of this token" must not fire
    // the revoke-everything path, or anyone who can sign a JWT could sign everyone out.
    let h = harness();
    let (user, _) = registered(&h).await;

    let error = h
        .service
        .refresh(&RefreshTokenRequest {
            refresh_token: "not.a.jwt".to_owned(),
        })
        .await
        .expect_err("refused");

    assert_eq!(error.code(), ErrorCode::InvalidToken);
    assert!(
        !h.repo
            .list_sessions(user.id)
            .await
            .expect("listing")
            .is_empty(),
        "a forged token must not end a real session"
    );
}

#[tokio::test]
async fn logging_out_everywhere_ends_every_device() {
    let h = harness();
    let (user, _) = registered(&h).await;
    second_device(&h, &user).await;

    h.service
        .logout_everywhere(user.id)
        .await
        .expect("logging out");

    assert!(
        h.repo
            .list_sessions(user.id)
            .await
            .expect("listing")
            .is_empty()
    );
}

// ── Google (plan §4.7 item 7, §7.12) ───────────────────────────────────────

#[tokio::test]
async fn a_google_sign_in_creates_the_account_and_records_the_provider() {
    let h = harness_with(StubExchange::named(
        "google-subject",
        "newcomer@example.com",
        Some("Grace Hopper"),
    ));

    let response = h
        .service
        .google_mobile_auth(
            &GoogleMobileAuthRequest {
                id_token: "id-token".to_owned(),
                device_label: Some("Ada's phone".to_owned()),
            },
            h.google.as_ref(),
        )
        .await
        .expect("signing in with Google");

    assert_eq!(response.user.email, "newcomer@example.com");
    assert_eq!(response.user.full_name, "Grace Hopper");
    assert!(
        response.user.verified,
        "§4.7 item 7: the provider proved the address, so the account is verified"
    );
    assert!(
        response.user.providers.contains(&AuthProvider::Google),
        "and the account is recognisable as a Google one later"
    );

    let sessions = h
        .service
        .list_sessions(response.user.id)
        .await
        .expect("listing");
    assert_eq!(
        sessions[0].device_label, "Ada's phone",
        "§4.7 item 1: the label the client sent names the session"
    );
}

#[tokio::test]
async fn a_google_sign_in_for_a_known_address_links_to_that_account() {
    let h = harness_with(StubExchange::exchanging(
        "google-subject",
        "ada@example.com",
    ));
    let (user, _) = registered(&h).await;
    assert_eq!(
        h.repo.list_providers(user.id).await.expect("listing"),
        vec![AuthProvider::Password],
        "before"
    );

    h.service
        .google_mobile_auth(
            &GoogleMobileAuthRequest {
                id_token: "id-token".to_owned(),
                device_label: None,
            },
            h.google.as_ref(),
        )
        .await
        .expect("linking");

    let providers = h.repo.list_providers(user.id).await.expect("listing");
    assert!(
        providers.contains(&AuthProvider::Google) && providers.contains(&AuthProvider::Password),
        "both ways in, and no second account: {providers:?}"
    );
}

#[tokio::test]
async fn an_unverified_provider_address_never_links_to_an_existing_account() {
    // §7.12, asserted through the service rather than only the seam: an attacker holding
    // a Google account carrying somebody else's address must not inherit that account.
    let repo = Arc::new(InMemoryAuthRepo::new());
    let repo_trait: Arc<dyn AuthRepo> = Arc::clone(&repo) as Arc<dyn AuthRepo>;
    let tokens = TokenService::new(
        token_config(),
        Arc::clone(&repo_trait),
        Arc::new(InMemoryAuthCache::new()) as Arc<dyn AuthCache>,
    )
    .expect("a valid token service");

    let service = AuthService::new(AuthDeps {
        repo: Arc::clone(&repo_trait),
        tokens,
        mailer: Arc::new(RecordingAuthMailer::new()),
    })
    .expect("a valid service");

    service
        .register(&register_request())
        .await
        .expect("registering the victim");
    let victim = repo
        .find_user_by_email("ada@example.com")
        .await
        .expect("looking up")
        .expect("exists");

    let exchange = ProviderExchange::new(
        Arc::new(
            crate::infrastructure::oauth::InMemoryIdentityProvider::returning(
                crate::infrastructure::oauth::GoogleIdentity::unverified(
                    "attacker",
                    "ada@example.com",
                ),
            ),
        ),
        Arc::new(crate::infrastructure::oauth::InMemoryStateStore::new()),
    );

    let error = service
        .google_mobile_auth(
            &GoogleMobileAuthRequest {
                id_token: "id-token".to_owned(),
                device_label: None,
            },
            &exchange,
        )
        .await
        .expect_err("the link is refused");

    assert_eq!(error.code(), ErrorCode::EmailNotVerified);
    assert_eq!(
        repo.list_providers(victim.id).await.expect("listing"),
        vec![AuthProvider::Password],
        "and the victim's account gained nothing"
    );
}

#[tokio::test]
async fn a_provider_fault_is_retryable_and_leaks_nothing() {
    let google = StubExchange::failing_id_token(meno_core::Error::Upstream {
        service: "google-oauth",
        detail: "connection reset to https://accounts.google.com/o/oauth2/v2/auth".to_owned(),
    });
    let h = harness_with(StubExchange::exchanging("subject", "someone@example.com"));

    let error = h
        .service
        .google_mobile_auth(
            &GoogleMobileAuthRequest {
                id_token: "id-token".to_owned(),
                device_label: None,
            },
            &google,
        )
        .await
        .expect_err("the provider is down");

    assert_eq!(error.code(), ErrorCode::UpstreamUnavailable);
    let body = meno_core::to_body(&error);
    assert_eq!(body.http_status, 503, "worth retrying, not worth panicking");
    assert!(
        !body.message.contains("accounts.google.com"),
        "the provider endpoint must not reach the client: {}",
        body.message
    );
}

// ── §4.8 observability ────────────────────────────────────────────────────

#[test]
fn a_logged_address_keeps_only_the_domain() {
    // The local part is what identifies a person and what a breach list is keyed on.
    assert_eq!(
        super::services::redact("ada.lovelace@example.com"),
        "***@example.com"
    );
    assert_eq!(
        super::services::redact("ada@example.com"),
        "***@example.com"
    );
    assert_eq!(super::services::redact("not-an-address"), "***");
    assert_eq!(super::services::redact(""), "***");
}

#[test]
fn no_debug_output_carries_a_secret_or_a_code() {
    let h = harness();

    let service = format!("{:?}", h.service);
    assert!(!service.contains("access-secret-for-tests"), "{service}");
    assert!(!service.contains("refresh-secret-for-tests"), "{service}");

    let credentials = format!("{:?}", h.credentials);
    assert!(
        !credentials.contains("refresh-secret-for-tests"),
        "{credentials}"
    );

    let state = format!("{:?}", h.state);
    assert!(!state.contains("-secret-"), "{state}");

    let google = format!("{:?}", h.google);
    assert!(!google.contains("ada@example.com"), "{google}");
}

// ── §9.3: handlers are thin, and that is observable ────────────────────────

/// The router `routes.rs` will mount.
///
/// Built here so the handlers are exercised as axum sees them — `State`, the body
/// extractor, `Extension` and the error renderer all in play. A handler tested by calling
/// it directly would not prove any of that.
fn router(state: AuthState) -> Router {
    Router::new()
        .route("/auth/login", post(super::handlers::login))
        .route("/auth/logout", post(super::handlers::logout))
        .route("/auth/sessions", get(super::handlers::list_sessions))
        .route("/auth/resend-otp", post(super::handlers::resend_otp))
        .with_state(state)
}

fn json_post(uri: &str, body: &str) -> HttpRequest<Body> {
    HttpRequest::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_owned()))
        .expect("a valid request")
}

async fn send(router: Router, request: HttpRequest<Body>) -> (StatusCode, serde_json::Value) {
    let response = router.oneshot(request).await.expect("a response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body buffers")
        .to_bytes();

    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

#[tokio::test]
async fn a_successful_login_renders_the_success_envelope() {
    let h = harness();
    registered(&h).await;

    let (status, body) = send(
        router(h.state),
        json_post(
            "/auth/login",
            &format!(
                r#"{{"email":"ada@example.com","password":{}}}"#,
                serde_json::json!(PASSWORD)
            ),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"], "OK");
    assert_eq!(body["status"], true);
    assert!(body["data"]["access_token"].is_string(), "{body}");
    assert!(body["data"]["expires_in"].is_i64(), "{body}");
}

#[tokio::test]
async fn a_bad_password_renders_the_taxonomy_envelope() {
    let h = harness();
    registered(&h).await;

    let (status, body) = send(
        router(h.state),
        json_post(
            "/auth/login",
            r#"{"email":"ada@example.com","password":"wrong"}"#,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "INVALID_CREDENTIALS");
    assert_eq!(body["status"], false);
}

#[tokio::test]
async fn a_validation_failure_lists_every_field_that_failed() {
    let (status, body) = send(
        router(harness().state),
        json_post("/auth/login", r#"{"email":"not-an-address","password":""}"#),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "422 for a §4.2 validation"
    );
    assert_eq!(body["code"], "VALIDATION_FAILED");
    // Both fields, not just the first — one round trip should be enough to fix a form.
    assert!(body["data"]["email"].is_array(), "{body}");
    assert!(body["data"]["password"].is_array(), "{body}");
}

#[tokio::test]
async fn a_route_without_the_auth_layer_reports_a_wiring_bug_not_a_401() {
    // The session endpoints read `AuthUser` from extensions. With no layer there is
    // nothing to read, and reporting that as "log in" would hide the misordering.
    let request = HttpRequest::builder()
        .uri("/auth/sessions")
        .body(Body::empty())
        .expect("a valid request");

    let response = router(harness().state)
        .oneshot(request)
        .await
        .expect("a response");

    assert_eq!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "the extractor reports a missing AuthUser as a server fault"
    );
}

#[tokio::test]
async fn a_session_list_carries_only_the_callers_own_devices() {
    let h = harness();
    let (user, _) = registered(&h).await;

    let mut request = HttpRequest::builder()
        .uri("/auth/sessions")
        .body(Body::empty())
        .expect("a valid request");
    request.extensions_mut().insert(AuthUser {
        id: user.id,
        jti: uuid::Uuid::new_v4(),
        full_name: user.full_name.clone(),
        email: user.email.clone(),
        verified: false,
        providers: Vec::new(),
        role: UserRole::User,
    });

    let (status, body) = send(router(h.state), request).await;
    assert_eq!(status, StatusCode::OK);

    let devices = body["data"].as_array().expect("a list");
    assert!(!devices.is_empty(), "the caller's own sessions are listed");
    assert!(
        devices
            .iter()
            .all(|d| d["id"].is_string() && d["device_label"].is_string()),
        "{body}"
    );
}

#[tokio::test]
async fn a_resend_is_indistinguishable_for_a_known_and_an_unknown_address() {
    // Both are 200 with the same body. That is the whole contract.
    let h = harness();
    registered(&h).await;

    let known = send(
        router(h.state.clone()),
        json_post(
            "/auth/resend-otp",
            r#"{"email":"ada@example.com","otp_type":"verify_email"}"#,
        ),
    )
    .await;
    let unknown = send(
        router(h.state),
        json_post(
            "/auth/resend-otp",
            r#"{"email":"nobody@example.com","otp_type":"verify_email"}"#,
        ),
    )
    .await;

    assert_eq!(known.0, unknown.0);
    assert_eq!(known.0, StatusCode::OK);
    assert_eq!(known.1, unknown.1, "and the same body");
}

#[tokio::test]
async fn a_logout_renders_no_payload() {
    // 200 with an envelope rather than 204: the client matches on `code`, and a 204 has
    // no body to put it in.
    let h = harness();
    let (_, refresh) = registered(&h).await;

    let (status, body) = send(
        router(h.state),
        json_post(
            "/auth/logout",
            &serde_json::json!({ "refresh_token": refresh }).to_string(),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["code"], "OK");
    assert!(
        body.get("data").is_none(),
        "a logout has nothing to return: {body}"
    );
}

// ── the boundary does the validating (§9.5) ────────────────────────────────

#[test]
fn the_registration_password_policy_is_enforced_at_the_boundary() {
    // The service does not re-check these; if it did, a second caller would have to
    // remember to.
    let weak = RegisterRequest {
        full_name: "Ada".to_owned(),
        email: "ada@example.com".to_owned(),
        password: "short".to_owned(),
    };
    assert_eq!(
        weak.validate().expect_err("refused").code(),
        ErrorCode::ValidationFailed
    );

    let bad_email = RegisterRequest {
        full_name: "Ada".to_owned(),
        email: "nope".to_owned(),
        password: PASSWORD.to_owned(),
    };
    assert_eq!(
        bad_email.validate().expect_err("refused").code(),
        ErrorCode::ValidationFailed
    );
}

#[test]
fn a_login_does_not_apply_the_registration_password_policy() {
    // Applying it would be free feedback for someone probing a password set elsewhere,
    // and would lock out accounts created before the policy changed.
    let request = LoginRequest {
        email: "ada@example.com".to_owned(),
        password: "short".to_owned(),
    };

    assert!(
        request.validate().is_ok(),
        "a weak password is the *server's* problem to reject, not the client's to police"
    );
}

#[test]
fn an_invalid_field_names_the_field_the_client_has_to_fix() {
    let failure = error::invalid_field("email", "That is not an address");

    // `ValidationFailed`, not `BadRequest`: a well-formed request carrying an
    // unacceptable value is 422, and §4.2 gives that its own wire code so a client can
    // tell "you sent nonsense" from "you sent something we disallow".
    assert_eq!(failure.code(), ErrorCode::ValidationFailed);

    // Asserted on the rendered body, not on `Display`: `Display` is the fixed string
    // "validation failed" precisely so that a log line or an error-chain print cannot
    // leak what was wrong with the request. The field name is client-facing only, and
    // this is the shape the client actually receives.
    let body = meno_core::to_body(&failure);
    assert_eq!(body.http_status, 422);
    assert_eq!(body.code, ErrorCode::ValidationFailed.as_str());
    let fields = body.data.expect("a validation failure carries its fields");
    assert_eq!(
        fields["email"],
        vec!["That is not an address".to_owned()],
        "{fields:?}"
    );
}
