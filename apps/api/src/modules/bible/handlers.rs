//! `GET /bible/translations` — what scripture this deployment publishes.
//!
//! Parse → respond, and nothing else: there is no repository to consult and no rule to
//! apply, only a `const` table projected onto the wire. The two branches a handler is
//! allowed are the two it has — an unconfigured CDN, and everything else.

use axum::extract::State;
use meno_core::Error as MenoError;

use crate::modules::bible::catalogue::TRANSLATIONS;
use crate::modules::bible::dto::BibleTranslationDto;
use crate::modules::bible::state::BibleState;
use crate::types::meno_response::{MenoResponse, Outcome};

/// `GET /bible/translations` — every payload a client may download, with its hash and URL.
///
/// Public: scripture is the same for every caller and the CDN read is unsigned (§3.3), so
/// there is no `AuthUser` to require. The route's only protection is the read rate limit
/// on `/bible/*` — this answer is ~400 bytes of JSON, and the payloads themselves never
/// touch the API.
///
/// # Errors
///
/// `503 UPSTREAM_UNAVAILABLE` when this deployment has no object storage configured: a
/// catalogue whose URLs cannot be built would send clients to a CDN base that does not
/// exist, and the honest answer is that the feature is not available here. The detail is
/// the name of the missing setting, phrased for a log rather than for the client.
pub async fn get_translations(
    State(state): State<BibleState>,
) -> Outcome<Vec<BibleTranslationDto>> {
    let Some(public_url) = state.payload_base_url() else {
        return Err(MenoError::Upstream {
            service: "storage",
            detail: "STORAGE_PUBLIC_URL is not configured, so no payload URL can be built"
                .to_owned(),
        }
        .into());
    };

    let translations = TRANSLATIONS
        .iter()
        .map(|entry| BibleTranslationDto::for_entry(entry, public_url))
        .collect();

    Ok(MenoResponse::ok(
        "Bible translations retrieved successfully",
        translations,
    ))
}

#[cfg(test)]
mod tests {
    //! The endpoint's contract: the envelope, the rows, the URLs, and the honest refusal
    //! when this deployment cannot serve payloads.

    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    /// The handler alone, mounted the way `routes::bible` mounts it minus the limiter —
    /// the rate limit needs Redis and is covered by the middleware's own tests.
    fn router(state: BibleState) -> Router {
        Router::new()
            .route("/bible/translations", get(get_translations))
            .with_state(state)
    }

    async fn body(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 16 * 1024)
            .await
            .expect("the body buffers");
        serde_json::from_slice(&bytes).expect("the body is JSON")
    }

    #[tokio::test]
    async fn the_catalogue_is_the_envelope_with_one_row_per_translation() {
        let response = router(BibleState::new(Some("https://cdn.test".to_owned())))
            .oneshot(
                Request::builder()
                    .uri("/bible/translations")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("a response");

        assert_eq!(response.status(), StatusCode::OK);

        let json = body(response).await;
        assert_eq!(json["status"], true);
        assert_eq!(json["code"], "OK");

        let rows = json["data"].as_array().expect("an array of rows");
        assert_eq!(rows.len(), TRANSLATIONS.len());
        assert_eq!(rows[0]["code"], "asv");
        assert_eq!(
            rows[0]["payloadUrl"],
            "https://cdn.test/bible/asv/1.json.gz"
        );
        // The hash travels with the row: this is what the client verifies the download
        // against, so a response without it would be unusable.
        assert_eq!(rows[0]["sha256"].as_str().map(str::len), Some(64));
        assert!(rows[0]["bytes"].as_u64().is_some_and(|bytes| bytes > 0));
    }

    #[tokio::test]
    async fn every_row_names_a_url_of_its_own() {
        // A copy-paste in the table would otherwise advertise one payload under three
        // codes, and a download would store the wrong text under the right name.
        let response = router(BibleState::new(Some("https://cdn.test".to_owned())))
            .oneshot(
                Request::builder()
                    .uri("/bible/translations")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("a response");

        let json = body(response).await;
        let urls: std::collections::HashSet<&str> = json["data"]
            .as_array()
            .expect("an array")
            .iter()
            .map(|row| row["payloadUrl"].as_str().expect("a URL"))
            .collect();

        assert_eq!(urls.len(), TRANSLATIONS.len());
    }

    #[tokio::test]
    async fn a_deployment_without_storage_refuses_rather_than_answering_dead_urls() {
        // `STORAGE_ENABLED=false` is a supported deployment. Answering `"/bible/asv/..."`
        // or a URL against a base that was never set would send every client to a 404
        // that looks like a permissions problem.
        let response = router(BibleState::new(None))
            .oneshot(
                Request::builder()
                    .uri("/bible/translations")
                    .body(Body::empty())
                    .expect("a valid request"),
            )
            .await
            .expect("a response");

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let json = body(response).await;
        assert_eq!(json["status"], false);
        assert_eq!(json["code"], "UPSTREAM_UNAVAILABLE");
        // The missing setting is named in the log, not to the client (§4.8).
        assert!(
            !json["message"]
                .as_str()
                .unwrap_or_default()
                .contains("STORAGE_PUBLIC_URL"),
            "the client must not be handed server configuration: {json}"
        );
    }
}
