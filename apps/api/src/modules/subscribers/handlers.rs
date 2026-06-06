use crate::modules::subscribers::errors::SubscribersError;
use crate::shared::middleware::auth::AuthUser;
use crate::shared::pagination::PaginationResponse;
use crate::shared::types::dto::UserSummary;
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
    app.subscribers.service.subscribe(&app, auth, id).await?;
    Ok(MenoResponse::no_content("Subscribed to user successfully"))
}

pub async fn unsubscribe(
    State(app): State<Arc<MenoState>>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<Uuid>,
) -> Result<MenoResponse<()>, SubscribersError> {
    app.subscribers.service.unsubscribe(&app, auth, id).await?;
    Ok(MenoResponse::no_content("Unsubscribed successfully"))
}

pub async fn get_my_subscribers(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<MenoResponse<PaginationResponse<UserSummary>>, SubscribersError> {
    Err(SubscribersError::SubscriberNotFound)
}

pub async fn get_my_subscriptions(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<MenoResponse<PaginationResponse<UserSummary>>, SubscribersError> {
    Err(SubscribersError::SubscriberNotFound)
}

pub async fn get_subscribers(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
) -> Result<MenoResponse<PaginationResponse<UserSummary>>, SubscribersError> {
    Err(SubscribersError::SubscriberNotFound)
}

pub async fn get_subscriptions(
    State(state): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(user_id): Path<Uuid>,
) -> Result<MenoResponse<PaginationResponse<UserSummary>>, SubscribersError> {
    Err(SubscribersError::SubscriberNotFound)
}
