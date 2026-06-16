use crate::modules::chat::dto;
use crate::modules::chat::errors::ChatError;
use crate::shared::middleware::extractors::MenoQuery;
use crate::shared::pagination::CursorPage;
use crate::shared::types::meno_response::MenoResponse;
use crate::state::MenoState;
use axum::extract::State;
use std::sync::Arc;

pub async fn get_messages(
    State(app): State<Arc<MenoState>>,
    MenoQuery(query): MenoQuery<dto::ChatMessageQuery>,
) -> Result<MenoResponse<CursorPage<dto::ChatMessageResponse>>, ChatError> {
    let page = app.chat.service.get_messages(&query).await?;
    Ok(MenoResponse::ok("Messages retrieved", page))
}
