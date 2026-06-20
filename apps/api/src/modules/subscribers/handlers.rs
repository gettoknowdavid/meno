use crate::modules::subscribers::dto::SubscriberItem;
use crate::modules::subscribers::errors::SubscribersError;
use crate::shared::middleware::auth::AuthUser;
use crate::shared::middleware::extractors::MenoQuery;
use crate::shared::pagination::{CursorPage, CursorParams};
use crate::shared::types::meno_response::MenoResponse;
use crate::state::MenoState;
use axum::Extension;
use axum::extract::{Path, State};
use std::sync::Arc;
use uuid::Uuid;

pub async fn subscribe(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<()>, SubscribersError> {
    app.subscribers.service.subscribe(auth, id).await?;
    Ok(MenoResponse::no_content("Subscribed to user successfully"))
}

pub async fn unsubscribe(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<()>, SubscribersError> {
    app.subscribers.service.unsubscribe(auth, id).await?;
    Ok(MenoResponse::no_content("Unsubscribed successfully"))
}

pub async fn get_my_subscribers(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    MenoQuery(params): MenoQuery<CursorParams>,
) -> Result<MenoResponse<CursorPage<SubscriberItem>>, SubscribersError> {
    let response = app
        .subscribers
        .service
        .get_my_subscribers(auth.id, &params)
        .await?;
    Ok(MenoResponse::ok("Subscribers retrieved", response))
}

pub async fn get_my_subscriptions(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    MenoQuery(params): MenoQuery<CursorParams>,
) -> Result<MenoResponse<CursorPage<SubscriberItem>>, SubscribersError> {
    let response = app
        .subscribers
        .service
        .get_my_subscriptions(auth.id, &params)
        .await?;
    Ok(MenoResponse::ok("Subscriptions retrieved", response))
}

pub async fn get_user_subscribers(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
    MenoQuery(params): MenoQuery<CursorParams>,
) -> Result<MenoResponse<CursorPage<SubscriberItem>>, SubscribersError> {
    let response = app
        .subscribers
        .service
        .get_user_subscribers(auth.id, user_id, &params)
        .await?;
    Ok(MenoResponse::ok("Subscribers retrieved", response))
}

pub async fn get_user_subscriptions(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
    MenoQuery(params): MenoQuery<CursorParams>,
) -> Result<MenoResponse<CursorPage<SubscriberItem>>, SubscribersError> {
    let response = app
        .subscribers
        .service
        .get_user_subscriptions(auth.id, user_id, &params)
        .await?;
    Ok(MenoResponse::ok("Subscriptions retrieved", response))
}
