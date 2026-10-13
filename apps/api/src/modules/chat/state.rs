//! The chat module's state: where concrete adapters are chosen (§5.6).
//!
//! The whole file's job: pool → [`PgChatRepo`], bridge → [`PubSubPublisher`]. Every
//! consumer above sees `Arc<dyn …>` and the module's tests assemble it from the in-memory
//! doubles instead, which is what lets the whole service suite run with no Postgres, no
//! Redis and no sockets.
//!
//! # Construction is infallible, and that is deliberate
//!
//! Nothing here reads a secret or dials anything: the pool is already connected and the
//! bridge is already verified at boot ([`crate::state::build`]). A `Result` that can only
//! be `Ok` is a worse API than none, because every caller writes the same dead match on
//! it — the same reasoning as [`crate::modules::broadcast::state`].

use std::sync::Arc;

use sqlx::PgPool;

use super::publisher::{ChatPublisher, PubSubPublisher};
use super::repository::{ChatRepo, PgChatRepo};
use super::service::ChatService;
use crate::infrastructure::ws::pubsub::WsPubSubBridge;

/// The chat module as the router sees it.
#[derive(Clone)]
pub struct ChatState {
    /// The rules. A handler reaches through this and nothing else.
    pub service: Arc<ChatService>,
}

impl std::fmt::Debug for ChatState {
    /// Names the field and stops: the service holds a pool and a pub/sub bridge, whose
    /// clients carry the Redis URL.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatState")
            .field("service", &" [chat rules]")
            .finish()
    }
}

/// The raw resources [`ChatState::new`] builds adapters from.
///
/// No object store, no mailer and no media server: chat needs one table pair and a way to
/// announce events, and the `Wiring` struct says so — an absent integration is a type
/// that cannot be requested, not an `Option` someone forgets to check.
pub struct Wiring {
    /// The application's pool.
    pub pool: PgPool,
    /// The cross-replica pub/sub bridge, for announcing events to a room's sockets.
    pub bridge: Arc<WsPubSubBridge>,
}

impl ChatState {
    /// Build the module from raw resources.
    #[must_use]
    pub fn new(wiring: Wiring) -> Self {
        let repo: Arc<dyn ChatRepo> = Arc::new(PgChatRepo::new(wiring.pool));
        let publisher: Arc<dyn ChatPublisher> = Arc::new(PubSubPublisher::new(wiring.bridge));
        Self::from_parts(repo, publisher)
    }

    /// Build the module from its trait objects — what `new` funnels into, and what tests
    /// call with the doubles.
    #[must_use]
    pub fn from_parts(repo: Arc<dyn ChatRepo>, publisher: Arc<dyn ChatPublisher>) -> Self {
        Self {
            service: Arc::new(ChatService::new(repo, publisher)),
        }
    }
}

#[cfg(test)]
mod tests {
    //! The wiring's one property: a module built from the doubles is usable without any
    //! infrastructure.

    use super::*;
    use crate::modules::chat::model::SenderSummary;
    use crate::modules::chat::publisher::recording::RecordingPublisher;
    use crate::modules::chat::repository::InMemoryChatRepo;
    use crate::modules::chat::service::ChatMember;

    #[tokio::test]
    async fn a_state_built_from_doubles_serves_a_page_without_any_infrastructure() {
        let repo = Arc::new(InMemoryChatRepo::new());
        let room = uuid::Uuid::from_u128(1);
        let ada = uuid::Uuid::from_u128(2);
        repo.add_user(SenderSummary {
            id: ada,
            full_name: "Ada Lovelace".to_owned(),
            bio: None,
            avatar_id: None,
            avatar_url: None,
        });
        repo.start_broadcast(room);
        repo.add_participant(room, ada);

        let state = ChatState::from_parts(
            Arc::clone(&repo) as Arc<dyn ChatRepo>,
            Arc::new(RecordingPublisher::new()) as Arc<dyn ChatPublisher>,
        );

        state
            .service
            .send_message(
                ChatMember {
                    broadcast_id: room,
                    user_id: ada,
                },
                "hello",
            )
            .await
            .expect("a participant of a live broadcast may post");

        let page = state
            .service
            .messages(
                ChatMember {
                    broadcast_id: room,
                    user_id: ada,
                },
                &meno_core::CursorParams::default(),
            )
            .await
            .expect("the room's own member may read it");

        assert_eq!(page.data.len(), 1);
        assert_eq!(page.data[0].content, "hello");
        assert!(!page.has_next_page, "one message is one page");
    }

    #[test]
    fn the_debug_impl_names_the_field_and_nothing_else() {
        let state = ChatState::from_parts(
            Arc::new(InMemoryChatRepo::new()) as Arc<dyn ChatRepo>,
            Arc::new(RecordingPublisher::new()) as Arc<dyn ChatPublisher>,
        );

        assert_eq!(
            format!("{state:?}"),
            "ChatState { service: \" [chat rules]\" }"
        );
    }
}
