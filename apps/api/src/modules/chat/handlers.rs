//! The chat HTTP endpoint (plan §9.3).
//!
//! One function, and it is parse → delegate → respond. The four events that *write* are
//! dispatched from [`crate::routes::ws`], not from here, because they arrive on a socket
//! and a socket has no `Response` to return: the same rule — decide nothing that
//! [`super::service`] can decide — applies to both places.
//!
//! The authenticated identity arrives as `Extension<AuthUser>` and is turned into a
//! [`ChatMember`] here, so no layer below has to remember which `Uuid` is the caller's.

use axum::extract::{Extension, Path, Query, State};
use uuid::Uuid;

use super::dto::{ChatMessageQuery, ChatMessageResponse};
use super::service::ChatMember;
use super::state::ChatState;
use crate::middleware::auth::AuthUser;
use crate::types::meno_response::{MenoResponse, Outcome};
use meno_core::CursorPage;

/// `GET /broadcasts/{broadcastId}/chat/messages` — a page of a broadcast's chat.
///
/// # Errors
///
/// 403 `NOT_PARTICIPANT` when the caller is not in the room; 400 `INVALID_CURSOR` for a
/// `?cursor=` that cannot be decoded.
pub async fn get_messages(
    State(state): State<ChatState>,
    Extension(auth): Extension<AuthUser>,
    Path(broadcast_id): Path<Uuid>,
    Query(query): Query<ChatMessageQuery>,
) -> Outcome<CursorPage<ChatMessageResponse>> {
    let page = state
        .service
        .messages(
            ChatMember {
                broadcast_id,
                user_id: auth.id,
            },
            &query.pagination,
        )
        .await?;

    Ok(MenoResponse::ok("Messages retrieved", page))
}
