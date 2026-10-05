//! The Prometheus recorder behind `/metrics` (plan §7.5, §4.8).
//!
//! # Why this wrapper exists
//!
//! `axum_prometheus::PrometheusMetricLayer::pair()` installs the process-global
//! metrics recorder — and its `MakeDefaultHandle` impl **panics if a recorder is
//! already installed**. Anything that builds the pair more than once (a second
//! `build_routes`, two integration tests in one binary, a future hot-reload) would
//! crash the process. So the pair is created exactly once, behind a `OnceCell`, and
//! this type hands out clones: the layer to wrap a router with, and the handle the
//! `/metrics` endpoint renders.
//!
//! # What gets recorded
//!
//! The three metrics `axum-prometheus` documents — `axum_http_requests_total`,
//! `axum_http_requests_duration_seconds` and `axum_http_requests_pending`, each
//! labelled by method, endpoint and status. That is §4.8's "request latency
//! histograms by route, error rate" without hand-rolling a histogram; job-queue and
//! WS-connection gauges join the same registry when those subsystems land.

use axum_prometheus::PrometheusMetricLayer;
use axum_prometheus::metrics_exporter_prometheus::PrometheusHandle;
use once_cell::sync::OnceCell;

/// The process-wide recorder: one layer to clone per router, one handle to render.
struct Parts {
    layer: PrometheusMetricLayer<'static>,
    handle: PrometheusHandle,
}

fn parts() -> &'static Parts {
    static PARTS: OnceCell<Parts> = OnceCell::new();

    PARTS.get_or_init(|| {
        // `pair()` takes no borrowed input, so the layer's lifetime slot unifies with
        // `'static` here — the whole point of caching it.
        let (layer, handle) = PrometheusMetricLayer::pair();
        Parts { layer, handle }
    })
}

/// An owned view of the process-wide metrics, for the state to hold.
///
/// Cheap to clone — the handle is an `Arc` over the recorder — so axum's `State`
/// extractor can carry one per request without cost.
#[derive(Clone)]
pub struct Metrics {
    handle: PrometheusHandle,
}

impl Metrics {
    /// The process-wide instance, creating the recorder on first use.
    ///
    /// Must be called from inside the Tokio runtime: `axum-prometheus`'s default
    /// handle installs a periodic flush task. Every construction site — `state::build`
    /// via `bootstrap::build_context` — is async, so the constraint holds by
    /// construction rather than by convention.
    #[must_use]
    pub fn global() -> Self {
        Self {
            handle: parts().handle.clone(),
        }
    }

    /// The recording layer, to attach when building a router.
    #[must_use]
    pub fn layer() -> PrometheusMetricLayer<'static> {
        parts().layer.clone()
    }

    /// Render every recorded metric in the Prometheus exposition format.
    #[must_use]
    pub fn render(&self) -> String {
        self.handle.render()
    }
}

impl std::fmt::Debug for Metrics {
    /// The handle's own `Debug` prints recorder internals that say nothing to a
    /// reader of `MenoState`'s shape.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Metrics").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    //! The properties the wiring relies on: the recorder is built at most once no
    //! matter how often the accessors are called, and rendering produces a
    //! Prometheus-shaped payload rather than an empty string.

    use super::*;

    #[tokio::test]
    async fn the_recorder_is_built_at_most_once() {
        // The panic this guards against lives inside `axum-prometheus`: a second
        // `pair()` calls `metrics::set_global_recorder(...).expect(...)` and dies.
        // Reaching for the pair repeatedly — as a second `build_routes` or a second
        // test would — must therefore be boring. (`tokio::test` because the install
        // itself wants a runtime; see `Metrics::global`.)
        for _ in 0..3 {
            let _ = Metrics::global();
            let _ = Metrics::layer();
        }
    }

    #[tokio::test]
    async fn a_recorded_metric_reaches_the_rendered_payload() {
        // The end-to-end contract of this module: what the layer records is what the
        // endpoint will print. Without this, a mis-installed recorder would surface
        // as an empty scrape target long after the wiring that caused it.
        //
        // The install comes first: a counter incremented before the recorder exists
        // is silently dropped, so the order inside this test is the assertion.
        let metrics = Metrics::global();
        axum_prometheus::metrics::counter!("meno_test_smoke_total", "probe" => "metrics")
            .increment(1);

        let rendered = metrics.render();
        assert!(
            rendered.contains("meno_test_smoke_total"),
            "the handle must render what was recorded: {rendered}"
        );
    }
}
