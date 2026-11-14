//! The shared outbound HTTP client (plan §5.6, §9.4).
//!
//! # Why one client
//!
//! Before this module each adapter built its own `reqwest::Client`: FCM built one, the
//! Google adapter built one, and the OAuth code exchange built **one per request** —
//! `oauth2::reqwest::Client::new()` inside `exchange_code`, which means a fresh
//! connection pool, a fresh TLS handshake and no keep-alive for every sign-in. A client
//! is expensive to create and cheap to clone (the handle shares its pool), so the
//! process builds exactly one and every adapter clones the handle.
//!
//! # What the configuration says, and why it is here rather than per adapter
//!
//! - **A request timeout, and a shorter connect timeout.** §9.4's rule is that no
//!   outbound call is unbounded: a hung FCM endpoint must not hold a worker slot until
//!   the socket gives up. The adapters previously each spelled their own `10`; one
//!   spelling is one place to change and one number to review.
//! - **No redirects.** These are JSON APIs, not web pages. A 3xx from FCM or from the
//!   Google token endpoint is a fault, and following it would replay a `POST` —
//!   possibly carrying a bearer token — to wherever the `Location` pointed. With the
//!   policy set to none, `.error_for_status()` turns the 3xx into the error the
//!   adapter already knows how to classify. (This is also what `oauth2`'s own docs
//!   recommend for its `request_async`.)
//! - **Connection pooling and an idle timeout**, so a quiet period does not keep dead
//!   sockets open, and an active one does not churn.
//!
//! # Why the adapters still return their own error types
//!
//! This layer answers exactly one question — "could the client be built" — and that
//! failure is a startup failure. Each adapter maps [`BuildError`] into its own error
//! vocabulary at construction time, so a driver-level error still cannot cross into
//! request-serving code (§5.5).

use std::time::Duration;

use once_cell::sync::OnceCell;
use reqwest::Client;

/// Seconds a request may take end to end before it is abandoned.
///
/// Shared by FCM, Google OAuth and the mail adapter. The adapters used to spell their
/// own copies of this number (FCM and Google both said `10`); it lives here so the
/// outbound budget is one reviewable constant.
pub const REQUEST_TIMEOUT_SECS: u64 = 10;

/// Seconds a TCP connect (and the TLS handshake) may take.
///
/// Shorter than the request timeout on purpose: a host that cannot be connected to
/// should fail while the caller can still act on it, not after the full request
/// budget has already been spent.
pub const CONNECT_TIMEOUT_SECS: u64 = 5;

/// How long an idle pooled connection is kept before being closed.
pub const POOL_IDLE_TIMEOUT_SECS: u64 = 90;

/// The shared client could not be constructed.
///
/// Only reachable from startup: the configuration is constants, so the realistic cause
/// is a broken TLS backend or an exhausted file-descriptor limit on the host. That is
/// reported by `bootstrap` as a startup error (§4.3) rather than discovered by the
/// first outbound call.
#[derive(Debug, thiserror::Error)]
#[error("the outbound HTTP client could not be built: {0}")]
pub struct BuildError(#[source] reqwest::Error);

/// Build a client with this layer's policy.
///
/// Exposed separately from [`shared`] so a test — or an adapter that needs a variant —
/// can obtain a fresh client with the identical configuration rather than re-spelling
/// it.
///
/// # Errors
///
/// [`BuildError`] when the platform's TLS backend cannot be initialised.
pub fn build() -> Result<Client, BuildError> {
    Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .pool_idle_timeout(Duration::from_secs(POOL_IDLE_TIMEOUT_SECS))
        // See the module docs: a redirect from a JSON API is a fault to report, not a
        // route to follow.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(BuildError)
}

/// The process-wide client, built once.
///
/// Returns the *same* instance every time — adapters that need an owned handle clone
/// it, and the clone shares the pool. A reference rather than a clone so "shared" is
/// observable: the tests below compare addresses.
///
/// # Errors
///
/// [`BuildError`], as [`build`]. A failed initialisation is not cached (`OnceCell`
/// retries), which is the right behaviour: the realistic cause is a transient host
/// condition, and re-building costs microseconds — see the module docs on why this
/// error only ever surfaces at startup.
pub fn shared() -> Result<&'static Client, BuildError> {
    static CLIENT: OnceCell<Client> = OnceCell::new();

    CLIENT.get_or_try_init(build)
}

#[cfg(test)]
mod tests {
    //! The two properties adapters rely on: the configuration is buildable, and
    //! "shared" is literal — two callers must hold handles to the *same* pool, or the
    //! module's whole reason to exist is a lie.

    use super::*;

    #[test]
    fn the_client_builds() {
        build().expect("static configuration must be buildable");
    }

    #[test]
    fn shared_returns_the_same_pool() {
        let first = shared().expect("buildable");
        let second = shared().expect("buildable");

        assert!(
            std::ptr::eq(first, second),
            "two adapters must share one connection pool, not two"
        );
    }

    #[test]
    fn the_timeouts_are_ordered_sensibly() {
        // A connect timeout beyond the request timeout would never fire; a zero request
        // timeout would mean "no timeout", which is the bug this layer exists to end.
        // Compile-time constants, so the ordering is checked at compile time rather
        // than observed at runtime.
        const {
            assert!(CONNECT_TIMEOUT_SECS > 0);
            assert!(REQUEST_TIMEOUT_SECS > 0);
            assert!(CONNECT_TIMEOUT_SECS <= REQUEST_TIMEOUT_SECS);
            assert!(POOL_IDLE_TIMEOUT_SECS >= REQUEST_TIMEOUT_SECS);
        }
    }
}
