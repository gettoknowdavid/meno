use crate::modules::user::dto::MeResponse;
use crate::modules::user::errors::UserError;
use crate::shared::middleware::auth::AuthUser;
use crate::shared::types::meno_response::MenoResponse;
use crate::state::MenoState;
use axum::extract::{Extension, State};
use std::sync::Arc;

pub async fn get_me(
    State(app): State<Arc<MenoState>>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<MenoResponse<MeResponse>, UserError> {
    let me = app.user_service.get_me(auth_user.id).await?;
    Ok(MenoResponse::ok("User profile retrieved", me))
}
