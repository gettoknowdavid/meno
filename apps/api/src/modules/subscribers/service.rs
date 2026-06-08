use crate::modules::subscribers::dto::SubscriberItem;
use crate::modules::subscribers::errors::SubscribersError;
use crate::modules::subscribers::repository::SubscribersRepository;
use crate::shared::middleware::auth::AuthUser;
use crate::shared::pagination::{Cursor, CursorPage, CursorParams};
use crate::shared::services::ws::WsService;
use crate::shared::services::ws::dto::WsPayload;
use crate::state::MenoState;
use tracing::instrument;
use uuid::Uuid;

#[derive(Clone)]
pub struct SubscribersService {
    pub repo: SubscribersRepository,
    pub ws: WsService,
}
impl SubscribersService {
    pub fn new(db: sqlx::PgPool, ws: WsService) -> Self {
        Self {
            repo: SubscribersRepository::new(db),
            ws,
        }
    }

    #[instrument(skip(self, app, auth_user), fields(auth_user_id = %auth_user.id, subscription_id = %subscription_id))]
    pub async fn subscribe(
        &self,
        app: &MenoState,
        auth_user: AuthUser,
        subscription_id: Uuid,
    ) -> Result<(), SubscribersError> {
        if auth_user.id == subscription_id {
            return Err(SubscribersError::CannotSubscribeToSelf);
        }

        app.auth
            .service
            .find_user_by_id(subscription_id)
            .await
            .map_err(|_| SubscribersError::SubscriptionNotFound)?
            .ok_or(SubscribersError::SubscriptionNotFound)?;

        self.repo.create(auth_user.id, subscription_id).await?;

        let ws = self.ws.clone();
        let subscriber_id = auth_user.id;
        let subscriber_name = auth_user.full_name.clone();
        tokio::spawn(async move {
            let payload = WsPayload::notification(
                subscriber_id,
                "You've got a new subscription",
                format!("{} is now subscribed to you.", subscriber_name),
            );
            ws.send_to_user(subscription_id, payload).await;
        });
        tracing::info!("Subscription created successfully");
        Ok(())
    }

    #[instrument(skip(self, app, auth_user), fields(auth_user_id = %auth_user.id, subscription_id = %subscription_id))]
    pub async fn unsubscribe(
        &self,
        app: &MenoState,
        auth_user: AuthUser,
        subscription_id: Uuid,
    ) -> Result<(), SubscribersError> {
        if auth_user.id == subscription_id {
            return Err(SubscribersError::CannotSubscribeToSelf);
        }

        app.auth
            .service
            .find_user_by_id(subscription_id)
            .await
            .map_err(|_| SubscribersError::SubscriptionNotFound)?
            .ok_or(SubscribersError::SubscriptionNotFound)?;

        self.repo.delete(auth_user.id, subscription_id).await?;

        let ws = self.ws.clone();
        let subscriber_id = auth_user.id;
        let subscriber_name = auth_user.full_name.clone();
        tokio::spawn(async move {
            let payload = WsPayload::notification(
                subscriber_id,
                "Someone unsubscribed",
                format!("{} is no longer subscribed to you", subscriber_name),
            );
            ws.send_to_user(subscription_id, payload).await;
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
            .find_subscribers(auth_id, Some(auth_id), &params)
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
            .find_subscriptions(auth_id, Some(auth_id), &params)
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
            .find_subscribers(user_id, Some(auth_id), &params)
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
            .find_subscriptions(user_id, Some(auth_id), &params)
            .await?;

        Ok(CursorPage::from_rows(rows, params.limit(), |r| {
            Cursor::from_timestamp_id(r.subscribed_at, r.id)
        }))
    }
}
