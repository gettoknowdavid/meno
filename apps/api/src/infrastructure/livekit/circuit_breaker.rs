//! Circuit breaker for the LiveKit media service.
//!
//! Ported from `apps/api/src/shared/services/livekit/circuit_breaker.rs` on `master`
//! (`903c3ba`), with the transition logic made testable without waiting on a clock.
//!
//! # Why a breaker is needed at all
//!
//! LiveKit is an external dependency on every join, which plan §1.5 lists as the p95
//! budget for `/broadcasts/:id/join`. When it is slow or down, every join request
//! blocks for the full timeout and the web replica's request budget is consumed by
//! calls that cannot succeed. The breaker turns that into an immediate, cheap failure
//! and gives LiveKit room to recover.
//!
//! # The three states
//!
//! - **Closed** — normal. Failures are counted; `failure_threshold` consecutive
//!   failures trip it.
//! - **Open** — failing fast. Every call is rejected without touching the network,
//!   until `open_duration` has elapsed since the last failure.
//! - **HalfOpen** — probing. The first call is allowed through to test recovery. It
//!   takes `success_threshold` successes to close again; a single failure re-opens
//!   immediately, because a service that just failed once while being probed is not
//!   recovered.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// What the breaker is currently doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CircuitState {
    /// Normal operation.
    Closed = 0,
    /// Failing fast; calls are rejected without a network round trip.
    Open = 1,
    /// Probing recovery with a single call.
    HalfOpen = 2,
}

impl CircuitState {
    /// Whether calls pass through.
    #[must_use]
    pub const fn allows_calls(self) -> bool {
        !matches!(self, Self::Open)
    }
}

impl From<u8> for CircuitState {
    /// Unknown values map to `Closed`.
    ///
    /// Safe rather than conservative on purpose: `Closed` still fails *fast* in the
    /// sense that it tries the call, and a corrupt state byte must not wedge the
    /// feature permanently. The alternative — mapping to `Open` — turns a memory
    /// glitch into an outage with no recovery path short of a restart.
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Closed,
            1 => Self::Open,
            2 => Self::HalfOpen,
            _ => Self::Closed,
        }
    }
}

/// Guards calls to LiveKit.
pub struct CircuitBreaker {
    state: AtomicU8,
    failure_count: AtomicU64,
    success_count: AtomicU64,
    last_failure_at: Mutex<Option<Instant>>,

    failure_threshold: u64,
    success_threshold: u64,
    open_duration: Duration,
}

impl CircuitBreaker {
    /// Build a breaker.
    ///
    /// `failure_threshold` is the consecutive failures that trip it; `open_duration`
    /// is how long it stays open before allowing a probe. `success_threshold` is
    /// fixed at 2: one success could be a fluke, two is convincing enough to restore
    /// traffic without making recovery feel slow.
    #[must_use]
    pub fn new(failure_threshold: u64, open_duration: Duration) -> Arc<Self> {
        Arc::new(Self {
            state: AtomicU8::new(CircuitState::Closed as u8),
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            last_failure_at: Mutex::new(None),
            // A zero threshold would trip before any failure is recorded, wedging the
            // breaker open from construction.
            failure_threshold: failure_threshold.max(1),
            success_threshold: 2,
            open_duration,
        })
    }

    /// The current state.
    #[must_use]
    pub fn state(&self) -> CircuitState {
        CircuitState::from(self.state.load(Ordering::Acquire))
    }

    /// Consecutive failures recorded since the last success.
    #[must_use]
    pub fn failure_count(&self) -> u64 {
        self.failure_count.load(Ordering::Relaxed)
    }

    /// Whether a call may proceed. Call *before* making the request.
    ///
    /// Transitions `Open` → `HalfOpen` once `open_duration` has elapsed, which is
    /// what makes recovery automatic rather than requiring a restart.
    ///
    /// # Errors
    ///
    /// Returns a message when the circuit is open and still within its cool-down.
    pub async fn check(&self) -> Result<(), &'static str> {
        match self.state() {
            CircuitState::Closed | CircuitState::HalfOpen => Ok(()),

            CircuitState::Open => {
                let elapsed = {
                    let last = self.last_failure_at.lock().await;
                    last.map(|at| at.elapsed())
                };

                if elapsed.is_some_and(|elapsed| elapsed >= self.open_duration) {
                    self.state
                        .store(CircuitState::HalfOpen as u8, Ordering::Release);
                    tracing::info!("LiveKit circuit breaker → HalfOpen");
                    return Ok(());
                }

                Err(OPEN_MESSAGE)
            }
        }
    }

    /// Record a success. Call *after* a successful request.
    pub async fn on_success(&self) {
        match self.state() {
            CircuitState::HalfOpen => {
                let successes = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
                if successes >= self.success_threshold {
                    self.state
                        .store(CircuitState::Closed as u8, Ordering::Release);
                    self.failure_count.store(0, Ordering::Relaxed);
                    self.success_count.store(0, Ordering::Relaxed);
                    tracing::info!("LiveKit circuit breaker → Closed (recovered)");
                }
            }
            _ => {
                // A success in Closed state resets the window, so only *consecutive*
                // failures trip the breaker.
                self.failure_count.store(0, Ordering::Relaxed);
            }
        }
    }

    /// Record a failure. Call *after* a failed request.
    pub async fn on_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        *self.last_failure_at.lock().await = Some(Instant::now());

        // Any failure while probing re-opens immediately: the service was just
        // observed broken, and half a recovery is not recovery.
        if failures >= self.failure_threshold || self.state() == CircuitState::HalfOpen {
            self.state
                .store(CircuitState::Open as u8, Ordering::Release);
            self.success_count.store(0, Ordering::Relaxed);
            tracing::error!(failures, "LiveKit circuit breaker → Open");
        }
    }

    /// Force the breaker open.
    ///
    /// For tests, and for a deliberate kill-switch; a `check()` before the duration
    /// elapses still fails fast, so this does not defeat the cool-down.
    pub async fn trip(&self) {
        self.state
            .store(CircuitState::Open as u8, Ordering::Release);
        *self.last_failure_at.lock().await = Some(Instant::now());
    }
}

/// The rejection message, a constant so tests can assert on it.
pub const OPEN_MESSAGE: &str = "LiveKit circuit breaker is Open — failing fast";

#[cfg(test)]
mod tests {
    use super::*;

    /// A breaker with a zero-length open duration, so recovery can be tested without
    /// sleeping. The state machine is identical; only the clock differs.
    fn instant_recovery() -> Arc<CircuitBreaker> {
        CircuitBreaker::new(3, Duration::ZERO)
    }

    #[tokio::test]
    async fn starts_closed_and_allows_calls() {
        let breaker = instant_recovery();
        assert_eq!(breaker.state(), CircuitState::Closed);
        assert!(breaker.state().allows_calls());
        breaker
            .check()
            .await
            .expect("closed breaker must allow calls");
    }

    #[tokio::test]
    async fn trips_only_after_the_threshold_is_reached() {
        let breaker = CircuitBreaker::new(3, Duration::from_secs(60));

        breaker.on_failure().await;
        assert_eq!(
            breaker.state(),
            CircuitState::Closed,
            "one failure is noise"
        );
        breaker.on_failure().await;
        assert_eq!(
            breaker.state(),
            CircuitState::Closed,
            "two failures is still under the threshold"
        );

        breaker.on_failure().await;
        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(!breaker.state().allows_calls());
        assert_eq!(breaker.failure_count(), 3);
    }

    #[tokio::test]
    async fn an_open_breaker_fails_fast_without_touching_the_network() {
        // The entire point: a down LiveKit must not consume the request budget.
        let breaker = CircuitBreaker::new(1, Duration::from_secs(60));
        breaker.on_failure().await;

        let err = breaker.check().await.expect_err("open breaker must reject");
        assert_eq!(err, OPEN_MESSAGE);
    }

    #[tokio::test]
    async fn a_success_resets_the_failure_window() {
        // Failures must be *consecutive*. An occasional 503 from a healthy service
        // should never accumulate into an outage.
        let breaker = CircuitBreaker::new(3, Duration::from_secs(60));

        breaker.on_failure().await;
        breaker.on_failure().await;
        breaker.on_success().await;

        assert_eq!(breaker.failure_count(), 0);
        assert_eq!(breaker.state(), CircuitState::Closed);

        // And the next two failures do not trip it, because the window restarted.
        breaker.on_failure().await;
        breaker.on_failure().await;
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn recovers_through_half_open_after_the_cool_down() {
        let breaker = instant_recovery();

        for _ in 0..3 {
            breaker.on_failure().await;
        }
        assert_eq!(breaker.state(), CircuitState::Open);

        // The duration has elapsed, so the next check admits a probe.
        breaker
            .check()
            .await
            .expect("must probe after the cool-down");
        assert_eq!(breaker.state(), CircuitState::HalfOpen);

        breaker.on_success().await;
        assert_eq!(
            breaker.state(),
            CircuitState::HalfOpen,
            "one success is not enough to restore traffic"
        );

        breaker.on_success().await;
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn one_failure_while_probing_reopens_immediately() {
        // A service that fails the probe has not recovered. Requiring two successes
        // first would send real traffic to a still-broken LiveKit.
        let breaker = instant_recovery();
        for _ in 0..3 {
            breaker.on_failure().await;
        }
        breaker.check().await.expect("probe");
        assert_eq!(breaker.state(), CircuitState::HalfOpen);

        breaker.on_failure().await;
        assert_eq!(
            breaker.state(),
            CircuitState::Open,
            "a failed probe must re-open at once"
        );
    }

    #[tokio::test]
    async fn a_breaker_that_is_still_within_its_cool_down_keeps_rejecting() {
        let breaker = CircuitBreaker::new(1, Duration::from_secs(300));
        breaker.on_failure().await;

        // Repeated checks inside the window must not consume the cool-down or let a
        // probe through early.
        for _ in 0..5 {
            assert!(breaker.check().await.is_err());
        }
        assert_eq!(breaker.state(), CircuitState::Open);
    }

    #[tokio::test]
    async fn a_zero_threshold_cannot_wedge_the_breaker_open_at_construction() {
        // A `new(0, ..)` would trip before any failure is recorded and never recover.
        let breaker = CircuitBreaker::new(0, Duration::from_secs(60));
        assert_eq!(breaker.state(), CircuitState::Closed);
        breaker.check().await.expect("must allow calls");
    }

    #[tokio::test]
    async fn trip_opens_the_breaker_without_waiting_for_failures() {
        let breaker = CircuitBreaker::new(100, Duration::from_secs(60));
        breaker.trip().await;

        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(breaker.check().await.is_err());
    }

    #[test]
    fn an_unrecognised_state_byte_reads_as_closed() {
        // A corrupt byte must not wedge the feature; a wrong answer is recoverable,
        // a permanently-open breaker is not.
        for byte in 3..=255u8 {
            assert_eq!(CircuitState::from(byte), CircuitState::Closed);
        }
        assert_eq!(CircuitState::from(0), CircuitState::Closed);
        assert_eq!(CircuitState::from(1), CircuitState::Open);
        assert_eq!(CircuitState::from(2), CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn concurrent_failures_still_trip_exactly_once() {
        // Several joins failing at once must not push the counter past the threshold
        // and leave it there for a later, unrelated failure to compound.
        let breaker = CircuitBreaker::new(3, Duration::from_secs(60));

        let mut tasks = Vec::new();
        for _ in 0..10 {
            let breaker = Arc::clone(&breaker);
            tasks.push(tokio::spawn(async move { breaker.on_failure().await }));
        }
        for task in tasks {
            task.await.expect("join");
        }

        assert_eq!(breaker.state(), CircuitState::Open);
        assert_eq!(
            breaker.failure_count(),
            10,
            "every failure is still counted"
        );
    }
}
