use crate::modules::subscribers::dto::SubscriberItem;
use crate::modules::subscribers::errors::SubscribersError;
use crate::modules::subscribers::repository::{SubscribersRepo, SubscribersRepository};
use crate::shared::middleware::auth::AuthUser;
use crate::shared::pagination::{Cursor, CursorPage, CursorParams};
use crate::shared::services::ws::dto::WsPayload;
use crate::shared::services::ws::pubsub::WsPubSubBridge;
use std::sync::Arc;
use tracing::instrument;
use uuid::Uuid;

#[derive(Clone)]
pub struct SubscribersService {
    repo: Arc<dyn SubscribersRepo>,
    pubsub: Arc<WsPubSubBridge>,
}
impl SubscribersService {
    pub fn new(db: sqlx::PgPool, pubsub: Arc<WsPubSubBridge>) -> Self {
        let repo: Arc<dyn SubscribersRepo> = Arc::new(SubscribersRepository::new(db));
        Self {
            repo: Arc::clone(&repo),
            pubsub,
        }
    }

    #[instrument(skip(self, auth_user), fields(auth_user_id = %auth_user.id, subscription_id = %subscription_id))]
    pub async fn subscribe(
        &self,
        auth_user: AuthUser,
        subscription_id: Uuid,
    ) -> Result<(), SubscribersError> {
        if auth_user.id == subscription_id {
            return Err(SubscribersError::CannotSubscribeToSelf);
        }

        let (user_result, create_result) = tokio::join!(
            self.repo.find_user_by_id(subscription_id),
            self.repo.create(auth_user.id, subscription_id),
        );

        user_result?;
        create_result?;

        let pubsub = Arc::clone(&self.pubsub);
        let subscriber_id = auth_user.id;
        let subscriber_name = auth_user.full_name.clone();
        tokio::spawn(async move {
            let payload = WsPayload::notification(
                subscriber_id,
                "You've got a new subscription",
                format!("{} is now subscribed to you.", subscriber_name),
            );
            pubsub.publish_to_user(subscription_id, payload).await;
        });
        tracing::info!("Subscription created successfully");
        Ok(())
    }

    #[instrument(skip(self, auth_user), fields(auth_user_id = %auth_user.id, subscription_id = %subscription_id))]
    pub async fn unsubscribe(
        &self,
        auth_user: AuthUser,
        subscription_id: Uuid,
    ) -> Result<(), SubscribersError> {
        if auth_user.id == subscription_id {
            return Err(SubscribersError::CannotSubscribeToSelf);
        }

        let (user_result, delete_result) = tokio::join!(
            self.repo.find_user_by_id(subscription_id),
            self.repo.delete(auth_user.id, subscription_id)
        );

        user_result?;
        delete_result?;

        let pubsub = Arc::clone(&self.pubsub);
        let subscriber_id = auth_user.id;
        let subscriber_name = auth_user.full_name.clone();
        tokio::spawn(async move {
            let payload = WsPayload::notification(
                subscriber_id,
                "Someone unsubscribed",
                format!("{} is no longer subscribed to you", subscriber_name),
            );
            pubsub.publish_to_user(subscription_id, payload).await;
        });

        Ok(())
    }

    #[instrument(skip(self, params), fields(auth_id = %auth_id))]
    pub async fn get_my_subscribers(
        &self,
        auth_id: Uuid,
        params: &CursorParams,
    ) -> Result<CursorPage<SubscriberItem>, SubscribersError> {
        if !self.repo.user_exists(auth_id).await? {
            return Err(SubscribersError::SubscriberNotFound);
        }

        let rows = self
            .repo
            .find_subscribers(auth_id, Some(auth_id), params)
            .await?;

        Ok(CursorPage::from_rows(rows, params.limit(), |r| {
            Cursor::from_timestamp_id(r.subscribed_at, r.id)
        }))
    }

    #[instrument(skip(self, params), fields(auth_id = %auth_id))]
    pub async fn get_my_subscriptions(
        &self,
        auth_id: Uuid,
        params: &CursorParams,
    ) -> Result<CursorPage<SubscriberItem>, SubscribersError> {
        if !self.repo.user_exists(auth_id).await? {
            return Err(SubscribersError::SubscriberNotFound);
        }

        let rows = self
            .repo
            .find_subscriptions(auth_id, Some(auth_id), params)
            .await?;

        Ok(CursorPage::from_rows(rows, params.limit(), |r| {
            Cursor::from_timestamp_id(r.subscribed_at, r.id)
        }))
    }

    #[instrument(skip(self, params), fields(auth_id = %auth_id, user_id = %user_id))]
    pub async fn get_user_subscribers(
        &self,
        auth_id: Uuid,
        user_id: Uuid,
        params: &CursorParams,
    ) -> Result<CursorPage<SubscriberItem>, SubscribersError> {
        if !self.repo.user_exists(user_id).await? {
            return Err(SubscribersError::SubscriberNotFound);
        }

        let rows = self
            .repo
            .find_subscribers(user_id, Some(auth_id), params)
            .await?;

        Ok(CursorPage::from_rows(rows, params.limit(), |r| {
            Cursor::from_timestamp_id(r.subscribed_at, r.id)
        }))
    }

    #[instrument(skip(self, params), fields(auth_id = %auth_id, user_id = %user_id))]
    pub async fn get_user_subscriptions(
        &self,
        auth_id: Uuid,
        user_id: Uuid,
        params: &CursorParams,
    ) -> Result<CursorPage<SubscriberItem>, SubscribersError> {
        if !self.repo.user_exists(user_id).await? {
            return Err(SubscribersError::SubscriberNotFound);
        }

        let rows = self
            .repo
            .find_subscriptions(user_id, Some(auth_id), params)
            .await?;

        Ok(CursorPage::from_rows(rows, params.limit(), |r| {
            Cursor::from_timestamp_id(r.subscribed_at, r.id)
        }))
    }
}
