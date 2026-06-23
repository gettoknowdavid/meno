use crate::modules::notifications::model::codes;
use crate::modules::notifications::service::NotificationService;
use crate::modules::subscribers::dto::SubscriberItem;
use crate::modules::subscribers::errors::SubscribersError;
use crate::modules::subscribers::repository::{SubscribersRepo, SubscribersRepository};
use crate::shared::identity::IdentityReader;
use crate::shared::middleware::auth::AuthUser;
use crate::shared::pagination::{Cursor, CursorPage, CursorParams};
use crate::shared::types::dto::UserSummary;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::instrument;
use uuid::Uuid;

#[derive(Clone)]
pub struct SubscribersService {
    repo: Arc<dyn SubscribersRepo>,
    identity: Arc<dyn IdentityReader>,
    notifications: Arc<NotificationService>,
}
impl SubscribersService {
    pub fn new(
        db: PgPool,
        identity: Arc<dyn IdentityReader>,
        notifications: NotificationService,
    ) -> Self {
        let repo: Arc<dyn SubscribersRepo> = Arc::new(SubscribersRepository::new(db));
        let notifications = Arc::new(notifications);
        Self {
            repo: Arc::clone(&repo),
            identity,
            notifications,
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

        let user = self
            .identity
            .find_user_by_id(subscription_id)
            .await
            .map_err(SubscribersError::Database)?
            .ok_or(SubscribersError::SubscriberNotFound)?;

        self.repo.create(auth_user.id, subscription_id).await?;

        self.notifications
            .notify(
                subscription_id,
                codes::USER_SUBSCRIBED,
                Some(&UserSummary::from(user)),
                None,
                None,
            )
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(error=%e, "Failed to send subscribe notification");
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

        self.identity
            .find_user_by_id(subscription_id)
            .await
            .map_err(SubscribersError::Database)?
            .ok_or(SubscribersError::SubscriberNotFound)?;

        self.repo.delete(auth_user.id, subscription_id).await?;
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
