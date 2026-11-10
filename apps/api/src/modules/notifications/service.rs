//! The notifications rules, as orchestration (§5.1's `service` row).
//!
//! # What changed from `master`
//!
//! - **It holds trait objects, not a pool.** `master`'s service built its own
//!   repository from a `sqlx::PgPool`, so neither the rendering rules nor the push
//!   gating could be exercised without a database. It now holds
//!   [`NotificationsRepo`], [`Notifier`], [`UnreadCounter`] and [`PushSender`], which
//!   is what lets `tests/notifications_service.rs` run every rule hermetically.
//! - **The push fan-out is bounded and observable, not spawned blindly.** The
//!   single-recipient path awaits the send (so a test can assert the payload — the
//!   part that regresses); the fan-out path is detached, because a thousand-device
//!   multicast must not hold a request open, and it is skipped outright when the
//!   sender reports itself disabled.
//! - **A list renders every placeholder.** `master` substituted `{actor}` only, so
//!   "`{actor} is live: {title}`" reached the client with the literal `{title}` in it.
//!   The page rows now carry the broadcast's title (joined in the statement), and one
//!   variable map renders the list and the push payload alike.
//! - **A missing template is an explicit error, not an `unwrap`.** `master`'s
//!   `get_template` returned `TemplateNotFound`; here
//!   [`error::template_missing`] keeps that as a logged `500` and nothing is written.
//!
//! # The counter is advisory
//!
//! `increment` is best-effort; `unread_count` falls back to the database whenever the
//! cache has no answer. A stale badge is a cosmetic bug; a notification that failed
//! to store because Redis was busy is a lost notification.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::instrument;
use uuid::Uuid;

use super::counter::UnreadCounter;
use super::dto::{
    MarkAllReadResponse, MarkReadResponse, NotificationItem, NotificationQuery, UnreadCountResponse,
};
use super::error;
use super::model::{
    ACTOR_VAR, ActorSummary, BROADCAST_TITLE_VAR, NotificationRow, NotificationTemplate, codes,
};
use super::publisher::Notifier;
use super::repository::{NotificationFilter, NotificationsRepo, RepoTokenStore};
use crate::infrastructure::push::model::{PushMessage, PushTarget};
use crate::infrastructure::push::{DATA_DEEP_LINK, NotifyOutcome, PushSender};
use meno_core::Error as MenoError;
use meno_core::{Cursor, CursorPage};

/// The in-memory cache of templates, keyed by type code.
///
/// Read-mostly: a notification is created far more often than a template changes, so
/// an `RwLock` lets every writer proceed concurrently and the miss path pays for the
/// write lock once.
type TemplateCache = Arc<RwLock<HashMap<String, NotificationTemplate>>>;

/// The notifications rules.
#[derive(Clone)]
pub struct NotificationsService {
    repo: Arc<dyn NotificationsRepo>,
    notifier: Arc<dyn Notifier>,
    counter: Arc<dyn UnreadCounter>,
    push: Arc<dyn PushSender>,
    templates: TemplateCache,
}

impl std::fmt::Debug for NotificationsService {
    /// Names the fields and stops: the repository and the push sender hold pools and
    /// credentials.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificationsService")
            .field("repo", &self.repo)
            .field("notifier", &self.notifier)
            .field("counter", &self.counter)
            .field("push", &"[sender]")
            .finish()
    }
}

impl NotificationsService {
    /// Build the service over its collaborators.
    #[must_use]
    pub fn new(
        repo: Arc<dyn NotificationsRepo>,
        notifier: Arc<dyn Notifier>,
        counter: Arc<dyn UnreadCounter>,
        push: Arc<dyn PushSender>,
    ) -> Self {
        Self {
            repo,
            notifier,
            counter,
            push,
            templates: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load every active template into the cache.
    ///
    /// Optional: [`Self::template`] fills the cache on a miss, so a deployment that
    /// never calls this pays one extra query per type. It exists for a future startup
    /// task and for tests that want the cold path exercised deliberately.
    ///
    /// # Errors
    ///
    /// [`MenoError::Internal`] if the templates cannot be read.
    pub async fn warm_template_cache(&self) -> Result<(), MenoError> {
        let templates = self.repo.find_all_templates().await?;
        let mut cache = self.templates.write().await;
        *cache = templates
            .into_iter()
            .map(|template| (template.type_code.clone(), template))
            .collect();
        tracing::info!(count = cache.len(), "notification templates cached");
        Ok(())
    }

    /// Create one notification and deliver it.
    ///
    /// Steps, in order: resolve the template, render the sentence, store the row,
    /// publish the in-app event (if the recipient wants one), send the push (if push
    /// is enabled and they have a device), then count it unread.
    ///
    /// # Errors
    ///
    /// [`MenoError::Internal`] for a missing template or a storage failure. A push
    /// failure does not fail the call: the notification is already in the inbox.
    #[instrument(skip(self, actor, broadcast_title), fields(owner_id = %owner_id, type_code))]
    pub async fn notify(
        &self,
        owner_id: Uuid,
        type_code: &str,
        actor: Option<&ActorSummary>,
        broadcast_id: Option<Uuid>,
        broadcast_title: Option<&str>,
    ) -> Result<(), MenoError> {
        let template = self.template(type_code).await?;
        let vars = build_vars(actor, broadcast_title);
        let (title, body, image_url) = render(&template, &vars);
        let deep_link = deep_link_for(type_code, broadcast_id, actor.map(|a| a.id));

        let notification_id = self
            .repo
            .create(owner_id, template.id, actor.map(|a| a.id), broadcast_id)
            .await?;

        if self
            .repo
            .app_notifications_enabled(owner_id)
            .await
            .unwrap_or(false)
        {
            self.notifier.notify(owner_id, &title, &body).await;
        }

        // Awaited, not spawned: the outcome is worth logging, and push is off the
        // request path anyway (§4.9). `notify_user` skips the token lookup entirely
        // when the sender is disabled, so a push-less deployment pays nothing.
        let message =
            PushMessage::for_user(title.clone(), body.clone(), owner_id, deep_link.as_str());
        let outcome = self
            .push
            .notify_user(
                owner_id,
                &RepoTokenStore::new(Arc::clone(&self.repo)),
                message,
            )
            .await;
        log_push(owner_id, &outcome, image_url.as_deref());

        self.counter.increment(owner_id).await;

        tracing::debug!(
            notification_id = %notification_id,
            owner_id = %owner_id,
            type_code,
            "notification created"
        );
        Ok(())
    }

    /// Create one notification for each recipient and deliver them.
    ///
    /// The fan-out path: broadcasts and scheduled reminders notify every subscriber at
    /// once. The insert is one statement; delivery is detached, because it must not
    /// extend the caller's request.
    ///
    /// # Errors
    ///
    /// [`MenoError::Internal`] for a missing template or a storage failure. Delivery
    /// failures are logged, never returned: the rows are already stored.
    #[instrument(skip(self, owner_ids, actor, broadcast_title), fields(count = owner_ids.len(), type_code))]
    pub async fn notify_many(
        &self,
        owner_ids: &[Uuid],
        type_code: &str,
        actor: Option<&ActorSummary>,
        broadcast_id: Option<Uuid>,
        broadcast_title: Option<&str>,
    ) -> Result<(), MenoError> {
        if owner_ids.is_empty() {
            return Ok(());
        }

        let template = self.template(type_code).await?;
        let vars = build_vars(actor, broadcast_title);
        let (title, body, image_url) = render(&template, &vars);
        let deep_link = deep_link_for(type_code, broadcast_id, actor.map(|a| a.id));

        let inserted = self
            .repo
            .create_bulk(owner_ids, template.id, actor.map(|a| a.id), broadcast_id)
            .await?;

        tracing::info!(type_code, inserted, "bulk notification insert complete");

        // The unread counters, one per recipient. Detached after the first chunk: a
        // 10k-subscriber fan-out must not turn into 10k round trips inside a request.
        let counter = Arc::clone(&self.counter);
        let recipients = owner_ids.to_vec();
        tokio::spawn(async move {
            for owner_id in recipients {
                counter.increment(owner_id).await;
            }
        });

        // The push fan-out, likewise detached, and skipped when push is disabled.
        let push = Arc::clone(&self.push);
        let repo = Arc::clone(&self.repo);
        let recipients = owner_ids.to_vec();
        tokio::spawn(async move {
            if !push.is_enabled() {
                return;
            }
            let targets: Vec<PushTarget> = match repo.push_targets(&recipients).await {
                Ok(targets) => targets,
                Err(failure) => {
                    tracing::warn!(error = %failure, "push targets could not be read");
                    return;
                }
            };
            if targets.is_empty() {
                return;
            }
            let mut message = PushMessage::new(title.clone(), body.clone());
            message = message.with_data(DATA_DEEP_LINK, deep_link.clone());
            if let Some(image) = image_url {
                message = message.with_image(image);
            }
            let outcome = push.send_multicast(targets, message).await;
            tracing::info!(
                succeeded = outcome.succeeded,
                failed = outcome.failed.len(),
                "notification push fan-out complete"
            );
        });

        Ok(())
    }

    /// One page of `owner_id`'s inbox, rendered.
    ///
    /// # Errors
    ///
    /// [`MenoError::BadRequest`] `INVALID_CURSOR` for a `?cursor=` that cannot be
    /// decoded; [`MenoError::Internal`] for a storage failure.
    #[instrument(skip(self, query), fields(owner_id = %owner_id, unread_only = query.unread_only, limit = query.limit()))]
    pub async fn list(
        &self,
        owner_id: Uuid,
        query: &NotificationQuery,
    ) -> Result<CursorPage<NotificationItem>, MenoError> {
        let (cursor_ts, cursor_id) = match query.cursor() {
            None => (None, None),
            Some(cursor) => {
                let (ts, id) = cursor.to_timestamp_id()?;
                (Some(ts), Some(id))
            }
        };

        let rows = self
            .repo
            .find_page(&NotificationFilter {
                owner_id,
                unread_only: query.unread_only,
                limit_plus_one: query.limit_plus_one(),
                cursor: cursor_ts.zip(cursor_id),
            })
            .await?;

        let limit = query.limit();
        let items: Vec<NotificationItem> = rows.into_iter().map(item_from_row).collect();

        Ok(CursorPage::from_rows(items, limit, |item| {
            Cursor::from_timestamp_id(item.created_at, item.id)
        }))
    }

    /// `GET /notifications/unread-count`.
    ///
    /// # Errors
    ///
    /// [`MenoError::Internal`] only when the cache has no answer *and* the database
    /// read fails.
    pub async fn unread_count(&self, owner_id: Uuid) -> Result<UnreadCountResponse, MenoError> {
        let count = match self.counter.read(owner_id).await {
            Some(count) => count,
            None => self.repo.count_unread(owner_id).await?,
        };

        Ok(UnreadCountResponse { count })
    }

    /// `PATCH /notifications/{id}/read`.
    ///
    /// Marking an already-read notification read again is a success: the postcondition
    /// holds. Marking one that does not exist — or belongs to somebody else — is a
    /// `404`, because nothing changed and there is no such resource for this caller.
    ///
    /// # Errors
    ///
    /// [`MenoError::NotFound`] when no live notification carries the id for this owner.
    pub async fn mark_read(
        &self,
        notification_id: Uuid,
        owner_id: Uuid,
    ) -> Result<MarkReadResponse, MenoError> {
        let changed = self.repo.mark_read(notification_id, owner_id).await?;

        if changed {
            self.counter.decrement(owner_id).await;
        } else if !self.repo.exists(notification_id, owner_id).await? {
            return Err(error::notification_not_found());
        }

        Ok(MarkReadResponse {
            id: notification_id,
            read: true,
        })
    }

    /// `PATCH /notifications/read-all`.
    ///
    /// # Errors
    ///
    /// [`MenoError::Internal`] if the update fails.
    pub async fn mark_all_read(&self, owner_id: Uuid) -> Result<MarkAllReadResponse, MenoError> {
        let updated = self.repo.mark_all_read(owner_id).await?;

        // Reset rather than subtract `updated`: a counter that was never set would
        // otherwise stay at zero-by-subtraction instead of becoming correct, and a
        // cached zero is still a cache hit on the next read.
        self.counter.reset(owner_id).await;

        Ok(MarkAllReadResponse { updated })
    }

    /// `DELETE /notifications/{id}`.
    ///
    /// Idempotent, like every other `DELETE` in this API: deleting what is not there
    /// leaves the postcondition the caller asked for satisfied, so it is a success.
    ///
    /// # Errors
    ///
    /// [`MenoError::Internal`] if the delete fails.
    pub async fn delete(&self, notification_id: Uuid, owner_id: Uuid) -> Result<(), MenoError> {
        let was_unread = self.repo.is_unread(notification_id, owner_id).await?;
        self.repo.delete(notification_id, owner_id).await?;

        if was_unread {
            self.counter.decrement(owner_id).await;
        }
        Ok(())
    }

    /// The template for `type_code`, from the cache or — on a miss — the database.
    ///
    /// A miss is cached after it is filled, so the common path is a read lock and a
    /// clone.
    async fn template(&self, type_code: &str) -> Result<NotificationTemplate, MenoError> {
        {
            let cache = self.templates.read().await;
            if let Some(template) = cache.get(type_code) {
                return Ok(template.clone());
            }
        }

        let template = self
            .repo
            .find_template_by_code(type_code)
            .await?
            .ok_or_else(|| error::template_missing(type_code))?;

        self.templates
            .write()
            .await
            .insert(type_code.to_owned(), template.clone());
        Ok(template)
    }
}

/// The template variables an event contributes.
fn build_vars<'a>(
    actor: Option<&'a ActorSummary>,
    broadcast_title: Option<&'a str>,
) -> HashMap<&'a str, &'a str> {
    let mut vars: HashMap<&str, &str> = HashMap::new();
    if let Some(actor) = actor {
        vars.insert(ACTOR_VAR, actor.full_name.as_str());
    }
    if let Some(title) = broadcast_title {
        vars.insert(BROADCAST_TITLE_VAR, title);
        // The seeded templates use `{broadcast}` for the same fact.
        vars.insert("broadcast", title);
    }
    vars
}

/// Substitute every `{key}` in `template` with its value.
///
/// A placeholder with no value is left intact rather than replaced with an empty
/// string: `"{actor} is live"` rendering as `" is live"` hides a bug, while the
/// literal placeholder makes it visible to whoever reads the client.
fn interpolate(template: &str, vars: &HashMap<&str, &str>) -> String {
    let mut rendered = template.to_owned();
    for (key, value) in vars {
        rendered = rendered.replace(&format!("{{{key}}}"), value);
    }
    rendered
}

/// Render a template's three text fields.
fn render(
    template: &NotificationTemplate,
    vars: &HashMap<&str, &str>,
) -> (String, String, Option<String>) {
    (
        interpolate(&template.title, vars),
        interpolate(&template.body, vars),
        template
            .image_url
            .as_ref()
            .map(|url| interpolate(url, vars)),
    )
}

/// Turn a stored row into the wire item, rendering its sentence.
fn item_from_row(row: NotificationRow) -> NotificationItem {
    let actor = row.actor();
    // The broadcast's title is the event's; nothing else is needed to render, because
    // an actor's name is the only other variable the seeded templates use.
    let mut vars: HashMap<&str, &str> = HashMap::new();
    if let Some(actor) = actor.as_ref() {
        vars.insert(ACTOR_VAR, actor.full_name.as_str());
    }
    if let Some(title) = row.broadcast_title.as_deref() {
        vars.insert(BROADCAST_TITLE_VAR, title);
        vars.insert("broadcast", title);
    }

    NotificationItem {
        id: row.id,
        type_code: row.type_code.clone(),
        title: interpolate(&row.title_template, &vars),
        body: interpolate(&row.body_template, &vars),
        image_url: row
            .image_url_template
            .as_ref()
            .map(|url| interpolate(url, &vars)),
        read: row.read,
        broadcast_id: row.broadcast_id,
        deep_link: deep_link_for(
            &row.type_code,
            row.broadcast_id,
            actor.as_ref().map(|a| a.id),
        ),
        actor,
        created_at: row.created_at,
    }
}

/// The deep link a notification opens.
///
/// Kept in step with [`codes`] rather than inferred from the shape of the data: a
/// notification about a broadcast with no `broadcast_id` must not produce
/// `meno://broadcasts/null`.
fn deep_link_for(type_code: &str, broadcast_id: Option<Uuid>, actor_id: Option<Uuid>) -> String {
    match type_code {
        codes::ADDED_AS_COHOST
        | codes::LIVE_BROADCAST_STARTED
        | codes::SCHEDULED_BROADCAST
        | codes::BROADCAST_ENDED => broadcast_id.map_or_else(
            || "meno://home".to_owned(),
            |id| format!("meno://broadcasts/{id}"),
        ),
        codes::USER_SUBSCRIBED => actor_id.map_or_else(
            || "meno://home".to_owned(),
            |id| format!("meno://profile/{id}"),
        ),
        _ => "meno://home".to_owned(),
    }
}

/// Log a single push attempt's outcome. Non-fatal by design: the row is stored.
fn log_push(owner_id: Uuid, outcome: &NotifyOutcome, image_url: Option<&str>) {
    match outcome {
        NotifyOutcome::Delivered => {
            tracing::debug!(owner_id = %owner_id, "notification push delivered");
        }
        NotifyOutcome::Skipped => {
            tracing::debug!(
                owner_id = %owner_id,
                "notification push skipped (disabled, or no device token)"
            );
        }
        NotifyOutcome::TokenCleared => {
            tracing::info!(
                owner_id = %owner_id,
                "notification push rejected the device token; it has been cleared"
            );
        }
        NotifyOutcome::LookupFailed(failure) => {
            tracing::warn!(owner_id = %owner_id, error = %failure, "device token lookup failed");
        }
        NotifyOutcome::Failed(failure) => {
            tracing::warn!(owner_id = %owner_id, error = %failure, "notification push failed (non-fatal)");
        }
    }
    if let Some(image) = image_url {
        tracing::debug!(owner_id = %owner_id, image_url = image, "push image attached");
    }
}

#[cfg(test)]
mod tests {
    //! The pure helpers: rendering and deep links.

    use super::*;

    fn template() -> NotificationTemplate {
        NotificationTemplate {
            id: Uuid::from_u128(1),
            type_code: codes::LIVE_BROADCAST_STARTED.to_owned(),
            title: "Live Now".to_owned(),
            body: "{actor} is live: {title}".to_owned(),
            image_url: None,
        }
    }

    #[test]
    fn every_placeholder_is_substituted() {
        let actor = ActorSummary {
            id: Uuid::from_u128(2),
            full_name: "Ada Lovelace".to_owned(),
            bio: None,
            avatar_id: None,
            avatar_url: None,
        };
        let vars = build_vars(Some(&actor), Some("Morning Notes"));

        let (title, body, _) = render(&template(), &vars);

        assert_eq!(title, "Live Now");
        assert_eq!(body, "Ada Lovelace is live: Morning Notes");
    }

    #[test]
    fn a_placeholder_with_no_value_is_left_visible() {
        // Better a literal `{title}` than " is live: " — the first is a bug someone can
        // see, the second is a bug nobody notices.
        let (_, body, _) = render(&template(), &HashMap::new());
        assert_eq!(body, "{actor} is live: {title}");
    }

    #[test]
    fn both_broadcast_spellings_are_filled() {
        let vars = build_vars(None, Some("Morning Notes"));
        let template = NotificationTemplate {
            body: "{broadcast} / {title}".to_owned(),
            ..template()
        };

        let (_, body, _) = render(&template, &vars);
        assert_eq!(body, "Morning Notes / Morning Notes");
    }

    #[test]
    fn a_broadcast_notification_links_to_the_broadcast() {
        let id = Uuid::from_u128(7);
        for type_code in [
            codes::ADDED_AS_COHOST,
            codes::LIVE_BROADCAST_STARTED,
            codes::SCHEDULED_BROADCAST,
            codes::BROADCAST_ENDED,
        ] {
            assert_eq!(
                deep_link_for(type_code, Some(id), None),
                format!("meno://broadcasts/{id}")
            );
            assert_eq!(
                deep_link_for(type_code, None, None),
                "meno://home",
                "a broadcast notification without a broadcast must not spell 'null'"
            );
        }
    }

    #[test]
    fn a_follow_links_to_the_actor_profile() {
        let id = Uuid::from_u128(9);
        assert_eq!(
            deep_link_for(codes::USER_SUBSCRIBED, None, Some(id)),
            format!("meno://profile/{id}")
        );
        assert_eq!(
            deep_link_for(codes::USER_SUBSCRIBED, None, None),
            "meno://home"
        );
    }

    #[test]
    fn an_unknown_type_is_home() {
        assert_eq!(
            deep_link_for("something_new", Some(Uuid::nil()), None),
            "meno://home"
        );
    }
}
