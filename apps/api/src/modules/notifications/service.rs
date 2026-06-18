use crate::modules::notifications::dto::{
    MarkAllReadResponse, MarkReadResponse, NotificationListItem, NotificationQuery,
    UnreadCountResponse,
};
use crate::modules::notifications::error::NotificationError;
use crate::modules::notifications::model::{NotificationTemplate, codes};
use crate::modules::notifications::repository::NotificationRepository;
use crate::shared::pagination::{Cursor, CursorPage};
use crate::shared::services::push::PushNotificationService;
use crate::shared::services::redis::Redis;
use crate::shared::services::redis::keys::RedisKey;
use crate::shared::services::ws::dto::WsPayload;
use crate::shared::types::dto::UserSummary;
use crate::state::MenoState;
use fred::prelude::KeysInterface;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::instrument;
use uuid::Uuid;

/// In-memory cache of notification templates, keyed by type code.
/// Loaded at startup and refreshed every 10 minutes.
///
/// Using `Arc<RwLock<...>>` lets many readers proceed concurrently
/// while the refresh task holds a write lock only briefly.
type TemplateCache = Arc<RwLock<HashMap<String, NotificationTemplate>>>;

#[derive(Clone)]
pub struct NotificationService {
    repo: NotificationRepository,
    redis: Redis,
    push: PushNotificationService,
    templates: TemplateCache,
}
impl NotificationService {
    pub fn new(db: sqlx::PgPool, redis: Redis, push: PushNotificationService) -> Self {
        Self {
            repo: NotificationRepository::new(db),
            redis,
            push,
            templates: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load all active templates into the in-memory cache.
    /// Call once from `build_meno_router` after the pool is ready,
    /// and again every 10 minutes from a background task.
    pub async fn warm_template_cache(&self) -> Result<(), NotificationError> {
        let templates = self.repo.find_all_templates().await?;
        let mut cache = self.templates.write().await;
        *cache = templates
            .into_iter()
            .map(|t| (t.r#type.clone(), t))
            .collect();
        tracing::info!(count = cache.len(), "Notification template cache warmed");
        Ok(())
    }

    /// Send a notification to a single user.
    ///
    /// Steps:
    /// 1. Look up the template (in-memory cache → DB fallback).
    /// 2. Resolve template variables into title/body strings.
    /// 3. Persist to DB.
    /// 4. Send an in-app WebSocket event (if `app_notifications` is on).
    /// 5. Send a push notification (if `push_notifications` is on).
    /// 6. Increment the Redis unread counter.
    ///
    /// This function is **fire-and-forget safe** — callers may `tokio::spawn` it.
    #[instrument(
        skip(self, app, actor, broadcast_title),
        fields(owner_id = %owner_id, type_code = %type_code)
    )]
    pub async fn notify(
        &self,
        app: &MenoState,
        owner_id: Uuid,
        type_code: &str,
        actor: Option<&UserSummary>,
        broadcast_id: Option<Uuid>,
        broadcast_title: Option<&str>,
    ) -> Result<(), NotificationError> {
        let actor_id = actor.map(|a| a.id);
        let template = self.get_template(type_code).await?;
        let vars = Self::build_vars(actor, broadcast_title);
        let (title, body, image_url) = Self::resolve_template(&template, &vars);
        let deep_link = Self::build_deep_link(type_code, broadcast_id, actor_id);

        let notification = self
            .repo
            .create(owner_id, template.id, actor_id, broadcast_id, None)
            .await?;

        let ws_payload = WsPayload::notification(owner_id, &title, &body);
        app.pubsub.publish_to_user(owner_id, ws_payload).await;

        // Handle Push notifications
        let push = self.push.clone();
        let repo = self.repo.clone();
        let title_clone = title.clone();
        let body_clone = body.clone();
        let image_clone = image_url.clone();
        let deep_link_clone = deep_link.clone();

        tokio::spawn(async move {
            push.send_to_user_if_enabled(
                owner_id,
                &repo,
                &title_clone,
                &body_clone,
                image_clone,
                &deep_link_clone,
                None,
            )
            .await;
        });

        // Increase Redis unread-notifications counter
        let _ = self.redis.incr(&RedisKey::unread_count(owner_id)).await;

        tracing::debug!(
            notification_id = %notification.id,
            owner_id = %owner_id,
            type_code = %type_code,
            "Notification sent"
        );

        Ok(())
    }

    /// Fan-out: create a notification for every user in `owner_ids`.
    ///
    /// Used for "broadcast started" / "scheduled broadcast" fan-out
    /// where the same event should notify potentially thousands of users.
    ///
    /// DB insert is a single `INSERT … SELECT unnest(…)` statement.
    /// Push notifications are sent with bounded concurrency (50 parallel FCM calls).
    #[instrument(
        skip(self, owner_ids, actor, broadcast_title),
        fields(count = owner_ids.len(), type_code = %type_code)
    )]
    pub async fn notify_many(
        &self,
        owner_ids: &[Uuid],
        type_code: &str,
        actor: Option<&UserSummary>,
        broadcast_id: Option<Uuid>,
        broadcast_title: Option<&str>,
    ) -> Result<(), NotificationError> {
        if owner_ids.is_empty() {
            return Ok(());
        }

        let actor_id = actor.map(|a| a.id);
        let template = self.get_template(type_code).await?;
        let vars = Self::build_vars(actor, broadcast_title);
        let (title, body, image_url) = Self::resolve_template(&template, &vars);
        let deep_link = Self::build_deep_link(type_code, broadcast_id, actor_id);

        let inserted = self
            .repo
            .create_bulk(owner_ids, template.id, actor_id, broadcast_id)
            .await?;

        tracing::info!(
            type_code = %type_code,
            inserted = inserted,
            "bulk notification insert complete"
        );

        // Increment Redis unread counters for all recipients in a pipeline.
        // Use Lua to increment each key, setting TTL only on the first creation.
        // We do this in chunks to avoid building enormous pipelines.
        let redis = self.redis.clone();
        let ids_clone = owner_ids.to_vec();
        tokio::spawn(async move {
            for chunk in ids_clone.chunks(500) {
                let pipeline = redis.pipeline();
                for id in chunk {
                    let key = RedisKey::unread_count(*id);
                    // Silently ignore individual errors — the count is advisory.
                    let _ = pipeline.incr::<(), _>(key.as_ref()).await;
                }
                let _ = pipeline.all::<()>().await;
            }
        });

        // Handle Push notifications — bounded concurrency fan-out.
        let push = self.push.clone();
        let repo = self.repo.clone();
        let owner_ids_clone = owner_ids.to_vec();
        let title_clone = title.clone();
        let body_clone = body.clone();
        let image_clone = image_url.clone();
        let deep_link_clone = deep_link.clone();

        tokio::spawn(async move {
            if let Ok(tokens) = repo.get_push_tokens_batch(&owner_ids_clone).await {
                let token_pairs: Vec<(Uuid, String)> = tokens.into_iter().collect();
                push.send_multicast(
                    token_pairs,
                    &title_clone,
                    &body_clone,
                    image_clone,
                    &deep_link_clone,
                )
                .await;
            }
        });

        Ok(())
    }

    /// Paginated list for `GET /notifications`.
    /// Reads from DB and enriches with Redis unread count.
    pub async fn list(
        &self,
        owner_id: Uuid,
        query: &NotificationQuery,
    ) -> Result<CursorPage<NotificationListItem>, NotificationError> {
        let limit = query.limit();
        let mut rows = self.repo.find_notifications(query, owner_id).await?;

        for item in &mut rows {
            let vars = Self::build_item_vars(&item);
            let interpolated_title = Self::interpolate(&item.title, &vars);
            let interpolated_body = Self::interpolate(&item.body, &vars);

            item.title = interpolated_title;
            item.body = interpolated_body;
            item.deep_link = Self::build_deep_link(
                &item.type_code,
                item.broadcast_id,
                item.actor.as_ref().map(|a| a.id),
            );
        }

        let page = CursorPage::from_rows(rows, limit, |r| {
            Cursor::from_timestamp_id(r.created_at, r.id)
        });

        // // Unread count from Redis (fast path); fall back to DB if the key is missing.
        // let key = RedisKey::unread_count(owner_id);
        // let unread_count = match self.redis.get::<i64>(&key).await {
        //     Ok(Some(n)) => n,
        //     _ => self.repo.count_unread(owner_id).await.unwrap_or(0),
        // };

        Ok(page)
    }

    /// `GET /notifications/unread-count` — served from Redis.
    pub async fn unread_count(
        &self,
        owner_id: Uuid,
    ) -> Result<UnreadCountResponse, NotificationError> {
        let key = RedisKey::unread_count(owner_id);
        let count = match self.redis.get::<i64>(&key).await {
            Ok(Some(n)) => n,
            _ => self.repo.count_unread(owner_id).await?,
        };

        Ok(UnreadCountResponse { count })
    }

    /// `PATCH /notifications/:id/read`
    pub async fn mark_read(
        &self,
        notification_id: Uuid,
        owner_id: Uuid,
    ) -> Result<MarkReadResponse, NotificationError> {
        let was_updated = self.repo.mark_read(notification_id, owner_id).await?;

        if was_updated {
            // Decrement Redis counter, clamping at 0.
            self.decrement_unread(owner_id).await;
        }

        Ok(MarkReadResponse {
            id: notification_id,
            read: true,
        })
    }

    /// `PATCH /notifications/read-all`
    pub async fn mark_all_read(
        &self,
        owner_id: Uuid,
    ) -> Result<MarkAllReadResponse, NotificationError> {
        let updated = self.repo.mark_all_read(owner_id).await?;

        // Reset Redis unread counter to 0 (more reliable than decrementing by `updated`
        // if the key was never set).
        let key = RedisKey::unread_count(owner_id);
        let _ = self.redis.set(&key, &0_i64, Some(3600)).await;

        Ok(MarkAllReadResponse { updated })
    }

    /// `DELETE /notifications/:id`
    pub async fn delete(
        &self,
        notification_id: Uuid,
        owner_id: Uuid,
    ) -> Result<(), NotificationError> {
        // Check unread status before deleting so we can adjust the counter.
        let was_unread = self.repo.is_unread(notification_id, owner_id).await?;

        self.repo.delete(notification_id, owner_id).await?;

        if was_unread {
            self.decrement_unread(owner_id).await;
        }

        Ok(())
    }

    // ==================== PRIVATE HELPERS ====================

    /// Get a template from the in-memory cache, falling back to a DB query.
    async fn get_template(
        &self,
        type_code: &str,
    ) -> Result<NotificationTemplate, NotificationError> {
        // Fast path: read cache without blocking.
        {
            let cache = self.templates.read().await;
            if let Some(t) = cache.get(type_code) {
                return Ok(t.clone());
            }
        }

        let template = self
            .repo
            .find_template_by_code(type_code)
            .await?
            .ok_or_else(|| NotificationError::TemplateNotFound(type_code.to_owned()))?;

        {
            let mut cache = self.templates.write().await;
            cache.insert(type_code.to_owned(), template.clone());
        }

        Ok(template)
    }

    /// Replace `{actor}`, `{title}`, `{broadcast}` placeholders in a template string.
    pub fn resolve_template(
        template: &NotificationTemplate,
        vars: &HashMap<&str, &str>,
    ) -> (String, String, Option<String>) {
        let title = Self::interpolate(&template.title, vars);
        let body = Self::interpolate(&template.body, vars);
        let image_url = template
            .image_url
            .as_ref()
            .map(|u| Self::interpolate(u, vars));

        (title, body, image_url)
    }

    /// Generic string interpolation: replaces `{key}` with `value` for every entry in `vars`.
    fn interpolate(template: &str, vars: &HashMap<&str, &str>) -> String {
        let mut result = template.to_owned();
        for (key, value) in vars {
            result = result.replace(&format!("{{{}}}", key), value);
        }
        result
    }

    /// Build the template variable map from optional actor and broadcast title.
    fn build_vars<'a>(
        actor: Option<&'a UserSummary>,
        broadcast_title: Option<&'a str>,
    ) -> HashMap<&'a str, &'a str> {
        let mut vars: HashMap<&str, &str> = HashMap::new();
        if let Some(a) = actor {
            vars.insert("actor", a.full_name.as_str());
        }
        if let Some(t) = broadcast_title {
            vars.insert("title", t);
            vars.insert("broadcast", t);
        }
        vars
    }

    /// Build template vars from an already-assembled `NotificationListItem`
    /// (used during the list-render pass).
    fn build_item_vars(item: &NotificationListItem) -> HashMap<&str, &str> {
        let mut vars: HashMap<&str, &str> = HashMap::new();
        if let Some(ref a) = item.actor {
            vars.insert("actor", a.full_name.as_str());
        }
        vars
    }

    /// Returns the deep link URI for a given notification type.
    fn build_deep_link(
        type_code: &str,
        broadcast_id: Option<Uuid>,
        actor_id: Option<Uuid>,
    ) -> String {
        match type_code {
            codes::ADDED_AS_COHOST
            | codes::LIVE_BROADCAST_STARTED
            | codes::SCHEDULED_BROADCAST
            | codes::BROADCAST_ENDED => broadcast_id
                .map(|id| format!("meno://broadcasts/{}", id))
                .unwrap_or_else(|| "meno://home".to_string()),
            codes::USER_SUBSCRIBED => actor_id
                .map(|id| format!("meno://profile/{}", id))
                .unwrap_or_else(|| "meno://home".to_string()),
            _ => "meno://home".to_string(),
        }
    }

    /// Atomically decrement the Redis unread counter, clamping at 0.
    async fn decrement_unread(&self, owner_id: Uuid) {
        let key = RedisKey::unread_count(owner_id);

        // Lua: decrement but never go below 0.
        let script = r#"
            local v = redis.call('DECR', KEYS[1])
            if v < 0 then
                redis.call('SET', KEYS[1], '0')
                return 0
            end
            return v
        "#;

        let _ = self
            .redis
            .eval::<i64, Vec<&str>, Vec<i64>>(script, vec![key.as_ref()], vec![])
            .await;
    }
}
