use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Clone, Debug, Copy, PartialEq)]
#[repr(u8)]
pub enum CircuitState {
    /// Normal operations
    Closed = 0,

    /// Failing fast
    Open = 1,

    /// Probing
    HalfOpen = 2,
}
impl From<u8> for CircuitState {
    fn from(v: u8) -> Self {
        match v {
            0 => CircuitState::Closed,
            1 => CircuitState::Open,
            2 => CircuitState::HalfOpen,
            _ => CircuitState::Closed,
        }
    }
}

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
    pub fn new(failure_threshold: u64, open_duration_secs: u64) -> Arc<Self> {
        Arc::new(Self {
            state: AtomicU8::new(CircuitState::Closed as u8),
            failure_count: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            last_failure_at: Mutex::new(None),
            failure_threshold,
            success_threshold: 2,
            open_duration: Duration::from_secs(open_duration_secs),
        })
    }

    pub fn state(&self) -> CircuitState {
        CircuitState::from(self.state.load(Ordering::Acquire))
    }

    /// Call this before making a LiveKit request.
    /// Returns Err if the circuit is Open (failing fast).
    pub async fn check(&self) -> Result<(), &'static str> {
        match self.state() {
            CircuitState::Closed => Ok(()),
            CircuitState::Open => {
                // Check if we should transition to HalfOpen
                let last = self.last_failure_at.lock().await;
                if let Some(t) = *last
                    && t.elapsed() >= self.open_duration
                {
                    drop(last);
                    self.state
                        .store(CircuitState::HalfOpen as u8, Ordering::Release);
                    tracing::info!("LiveKit circuit breaker → HalfOpen");

                    // allow one probe request
                    return Ok(());
                }
                Err("LiveKit circuit breaker is Open — failing fast")
            }

            // Allow probe requests
            CircuitState::HalfOpen => Ok(()),
        }
    }

    /// Call this after a successful LiveKit request.
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
                // Reset failure window on success in Closed state
                self.failure_count.store(0, Ordering::Relaxed);
            }
        }
    }

    /// Call this after a failed LiveKit request.
    pub async fn on_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        *self.last_failure_at.lock().await = Some(Instant::now());

        if failures >= self.failure_threshold || self.state() == CircuitState::HalfOpen {
            self.state
                .store(CircuitState::Open as u8, Ordering::Release);
            self.success_count.store(0, Ordering::Relaxed);
            tracing::error!(failures = failures, "LiveKit circuit breaker → Open");
        }
    }
}
