//! The health surface: `GET /health` and `GET /health/ready` (plan §4.3).
//!
//! # Two probes, two questions
//!
//! | path | asks | touches dependencies |
//! | --- | --- | --- |
//! | `/health` | is this container running? | no |
//! | `/health/ready` | should this replica get traffic? | Postgres and Redis |
//!
//! Keeping them apart is not a naming preference. A liveness probe that fails when a
//! *dependency* is down gets the container killed and restarted, which fixes nothing and
//! turns a database blip into an outage; only readiness may depend on the dependencies.
//!
//! # Why this is its own file
//!
//! It was inlined in `mod.rs`, where the route table's real content is the mounts and
//! the layer stack. These two handlers are the only ones in the API that are not a
//! module's business, they carry their own tests, and the module docs of `mod.rs` now
//! have room to say what the table *is* rather than describing a probe.
//!
//! # The timeout
//!
//! [`PROBE_TIMEOUT`] is what makes a hanging dependency visible. A probe that waits
//! forever is indistinguishable from an outage, and one that waits ten seconds turns
//! every blip into a deployment event. Two seconds is comfortably above a local round
//! trip and well below any load-balancer timeout.

use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;

use crate::state::MenoState;

/// How long one dependency probe may take before it is reported as down.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// The paths this module mounts.
///
/// Listed here for the same reason the auth and broadcast modules list theirs: the
/// route-table tests in [`super`] assert against these, so a probe that is written and
/// forgotten fails a test instead of shipping as dead code.
pub const HEALTH_PATHS: &[&str] = &["/health", "/health/ready"];

/// `GET /health` — is the process up.
///
/// Deliberately does not touch Postgres or Redis. This endpoint answers "is this
/// container running", and a liveness probe that fails when a *dependency* is down gets
/// the container killed and restarted, which fixes nothing and turns a database blip
/// into an outage. Dependency health is [`readiness`]'s job.
#[must_use]
pub async fn liveness() -> &'static str {
    "ok"
}

/// What `/health/ready` reports (plan §4.3).
///
/// The exact shape the plan's gate curls for — `{"status":"ok","db":true,"redis":true}`
/// — with `status` flipping to `degraded` when either probe fails, and the status line
/// with it: 200 while the instance may take traffic, 503 when it may not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Readiness {
    /// `"ok"` or `"degraded"` — the one field an operator reads first.
    pub status: &'static str,
    /// Postgres answered `SELECT 1` inside the probe budget.
    pub db: bool,
    /// Redis answered `PING` inside the probe budget.
    pub redis: bool,
}

impl Readiness {
    /// Assemble the report for two probe outcomes.
    #[must_use]
    pub const fn new(db: bool, redis: bool) -> Self {
        Self {
            status: if db && redis { "ok" } else { "degraded" },
            db,
            redis,
        }
    }

    /// Whether this instance should receive traffic.
    #[must_use]
    pub const fn is_ready(self) -> bool {
        self.db && self.redis
    }

    /// 200 when ready, 503 when not — decided by the probes, never assumed.
    #[must_use]
    pub const fn status_code(self) -> StatusCode {
        if self.is_ready() {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}

/// `GET /health/ready` — should this instance receive traffic.
///
/// A real dependency check (§4.3): one round trip to Postgres and one to Redis, each
/// bounded by [`PROBE_TIMEOUT`], reported honestly. The previous version answered a
/// constant `"ready"` from configuration alone, which asserted nothing — it said
/// "booted" while the database was unreachable, and an orchestrator believes the
/// status line, not the prose around it.
///
/// Both probes run concurrently: a readiness endpoint is polled on every probe
/// interval, and serialising them doubles the worst-case latency of a path that sits
/// in front of every other one.
pub async fn readiness(State(state): State<MenoState>) -> (StatusCode, Json<Readiness>) {
    let (db, redis) = tokio::join!(probe_db(&state), probe_redis(&state));
    let report = Readiness::new(db, redis);

    if !report.is_ready() {
        // Named fields only (§4.8): which dependency failed is what the next reader
        // needs, and it goes to the log, not to the client.
        tracing::warn!(db, redis, "instance is not ready");
    }

    (report.status_code(), Json(report))
}

/// Whether Postgres answers a round trip inside the budget.
async fn probe_db(state: &MenoState) -> bool {
    matches!(
        tokio::time::timeout(
            PROBE_TIMEOUT,
            sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.db)
        )
        .await,
        Ok(Ok(1))
    )
}

/// Whether Redis answers a round trip inside the budget.
async fn probe_redis(state: &MenoState) -> bool {
    matches!(
        tokio::time::timeout(PROBE_TIMEOUT, state.redis.ping()).await,
        Ok(Ok(()))
    )
}

#[cfg(test)]
mod tests {
    //! The probes' own contract. They were in `routes::mod`'s test module and moved here
    //! with the handlers, because a test that lives three files away from the code it
    //! pins is a test nobody updates.

    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use tower::ServiceExt;

    #[tokio::test]
    async fn the_liveness_endpoint_answers_without_touching_a_dependency() {
        // `build_routes` needs a `MenoState`, which needs a pool and Redis, so this one
        // is mounted on its own router — which is exactly how it is reached in
        // production, before any merge. It answers with no `State` at all, which is the
        // structural reason a dependency outage cannot fail it.
        let router = Router::new().route("/health", get(liveness));

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("a response");

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 64)
            .await
            .expect("the body buffers");
        assert_eq!(bytes.as_ref(), b"ok");
    }

    #[test]
    fn readiness_renders_the_shape_the_plan_gate_curls() {
        // §4.3's contract, asserted on the encoded value: a renamed field here breaks
        // every probe script and dashboard that reads `db` or `redis`.
        let report = Readiness::new(true, true);

        assert_eq!(report.status_code(), StatusCode::OK);
        let json = serde_json::to_value(report).expect("serialisable");
        assert_eq!(
            json,
            serde_json::json!({ "status": "ok", "db": true, "redis": true })
        );
    }

    #[test]
    fn a_failed_probe_degrades_the_report_and_the_status_line() {
        // The half that actually matters: an orchestrator branches on the status code,
        // so a `true`-looking body with a 200 while Redis is down is the bug shape.
        for (db, redis) in [(false, true), (true, false), (false, false)] {
            let report = Readiness::new(db, redis);

            assert_eq!(report.status_code(), StatusCode::SERVICE_UNAVAILABLE);
            assert!(!report.is_ready());
            let json = serde_json::to_value(report).expect("serialisable");
            assert_eq!(json["status"], "degraded", "{json}");
            assert_eq!(json["db"], db, "{json}");
            assert_eq!(json["redis"], redis, "{json}");
        }
    }

    /// The distinction the split exists for, asserted at the type level.
    ///
    /// `liveness` coerces to a zero-argument function, which is only true because it
    /// takes no `State` extractor — so giving it one stops this compiling, rather than
    /// leaving a liveness probe that silently depends on Postgres. `readiness` takes
    /// `State<MenoState>` and therefore cannot satisfy this bound.
    #[test]
    fn the_liveness_probe_takes_no_state() {
        fn assert_takes_no_state<F, Fut>(_: F)
        where
            F: Fn() -> Fut,
            Fut: std::future::Future<Output = &'static str>,
        {
        }

        assert_takes_no_state(liveness);
    }
}
