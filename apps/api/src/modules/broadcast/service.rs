use crate::jobs::notification_jobs::BroadcastStartedFanOutJob;
use crate::modules::broadcast::dto::{
    BroadcastListCacheKey, BroadcastListItem, BroadcastQuery, BroadcastRefreshTokenResponse,
    BroadcastResponse, BroadcastSessionResponse, BroadcastSortBy, CohostSessionResponse,
    CreateBroadcastRequest, EndBroadcastResponse, LeaveBroadcastResponse, MAX_COHOSTS,
    ParticipantListCacheKey, ParticipantListItem, ParticipantQuery, ParticipantSortBy,
    UpdateBroadcastRequest,
};
use crate::modules::broadcast::errors::BroadcastError;
use crate::modules::broadcast::model::{
    Broadcast, BroadcastContext, BroadcastParticipant, BroadcastState, BroadcastStatus, EndReason,
    ParticipantRole,
};
use crate::modules::broadcast::repository::{
    BroadcastRepository, CreateBroadcastInput, SetActiveInput, UpdateBroadcastInput,
    UpsertParticipantInput,
};
use crate::shared::constants::{TTL_10_SECS, TTL_30_SECS, TTL_60_SECS};
use crate::shared::pagination::{Cursor, CursorPage, Order};
use crate::shared::services::livekit::LivekitService;
use crate::shared::services::livekit::dto::LivekitRole;
use crate::shared::services::redis::RedisService;
use crate::shared::services::redis::coalescing::coalesce_cache;
use crate::shared::services::redis::keys::RedisKey;
use crate::shared::services::ws::dto::WsPayload;
use crate::shared::services::ws::model::WsEvent;
use crate::shared::types::dto::UserSummary;
use crate::state::MenoState;
use std::collections::HashMap;
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct BroadcastService {
    repo: BroadcastRepository,
    livekit: LivekitService,
    redis: RedisService,
}
impl BroadcastService {
    pub fn new(repo: BroadcastRepository, livekit: LivekitService, redis: RedisService) -> Self {
        Self {
            repo,
            livekit,
            redis,
        }
    }

    pub async fn create(
        &self,
        app: &MenoState,
        req: CreateBroadcastRequest,
        creator_id: Uuid,
    ) -> Result<BroadcastResponse, BroadcastError> {
        if self.repo.is_active_host(creator_id).await? {
            return Err(BroadcastError::AlreadyLive);
        }

        if let Some(st) = req.start_time {
            if st <= OffsetDateTime::now_utc() {
                return Err(BroadcastError::StartTimeInPast);
            }
        }

        let cohost_ids = req.cohosts.clone().unwrap_or_default();
        let cohosts = self.deduplicate_cohosts(&cohost_ids, creator_id).await?;

        let mut tx = app.db.begin().await?;

        let broadcast = self
            .repo
            .create(
                &CreateBroadcastInput {
                    title: &req.title,
                    description: req.description.as_deref(),
                    image_id: req.image_id.as_deref(),
                    image_url: req.image_url.as_deref(),
                    time_zone: req.time_zone.as_deref(),
                    start_time: req.start_time,
                    recording_enabled: req.recording_enabled.unwrap_or(false),
                    creator_id,
                },
                &mut tx,
            )
            .await?;

        if !cohost_ids.is_empty() {
            self.repo
                .add_cohosts(broadcast.id, &cohost_ids, creator_id, &mut tx)
                .await?;
        }

        tx.commit().await?;

        // Schedule start notification job (fire-and-forget; not part of the tx).
        if broadcast.start_time.is_some() {
            // TODO: schedule apalis BroadcastStartJob here
        }

        let creator = self
            .repo
            .find_user_summary(broadcast.creator_id)
            .await?
            .ok_or(BroadcastError::UserNotFound)?;

        let ctx = BroadcastContext {
            participant_id: Some(creator_id),
            participant_role: ParticipantRole::Host,
            is_subscribed_to_creator: false, // creator can't subscribe to themselves
            is_bookmarked: false,
            live_count: 0,
            total_count: 0,
            ..Default::default()
        };

        self.build_response(broadcast, creator, cohosts, ctx).await
    }

    pub async fn update(
        &self,
        app: &MenoState,
        req: UpdateBroadcastRequest,
        broadcast_id: Uuid,
        creator_id: Uuid,
    ) -> Result<BroadcastResponse, BroadcastError> {
        let (broadcast_result, creator_result) = tokio::join!(
            self.repo.find_by_id(broadcast_id),
            self.repo.find_user_summary(creator_id),
        );

        let broadcast = broadcast_result?.ok_or(BroadcastError::NotFound)?;
        let creator = creator_result?.ok_or(BroadcastError::UserNotFound)?;

        if broadcast.creator_id != creator_id {
            return Err(BroadcastError::NotCreator);
        }

        if broadcast.status == BroadcastStatus::Active {
            return Err(BroadcastError::CannotModifyLiveBroadcast);
        }

        let cohost_ids = req.cohosts.clone().unwrap_or_default();
        let cohosts = self.deduplicate_cohosts(&cohost_ids, creator_id).await?;

        let mut tx = app.db.begin().await?;

        let broadcast = self
            .repo
            .update(
                &UpdateBroadcastInput {
                    title: req.title.as_deref(),
                    description: req.description.as_deref(),
                    image_id: req.image_id.as_deref(),
                    image_url: req.image_url.as_deref(),
                    time_zone: req.time_zone.as_deref(),
                    start_time: req.start_time,
                    recording_enabled: req.recording_enabled,
                    broadcast_id,
                },
                &mut tx,
            )
            .await?;

        if !cohost_ids.is_empty() {
            self.repo
                .add_cohosts(broadcast.id, &cohost_ids, creator_id, &mut tx)
                .await?;
        }

        tx.commit().await?;

        // Cache Invalidation
        self.invalidate_list_caches();
        self.invalidate_broadcast_cache(broadcast_id);

        let ctx = BroadcastContext {
            participant_id: Some(creator_id),
            participant_role: ParticipantRole::Host,
            is_subscribed_to_creator: false,
            is_bookmarked: false,
            live_count: 0,
            total_count: 0,
            ..Default::default()
        };

        self.build_response(broadcast, creator, cohosts, ctx).await
    }

    pub async fn delete(
        &self,
        app: &MenoState,
        broadcast_id: Uuid,
        creator_id: Uuid,
    ) -> Result<(), BroadcastError> {
        let (broadcast_result, creator_result) = tokio::join!(
            self.repo.find_by_id(broadcast_id),
            self.repo.find_user_summary(creator_id)
        );

        let broadcast = broadcast_result?.ok_or(BroadcastError::NotFound)?;
        let creator = creator_result?.ok_or(BroadcastError::UserNotFound)?;

        if broadcast.deleted_at.is_some() {
            return Ok(());
        }

        if broadcast.status == BroadcastStatus::Active {
            return Err(BroadcastError::CannotModifyLiveBroadcast);
        }

        if broadcast.creator_id != creator_id {
            return Err(BroadcastError::NotCreator);
        }

        self.repo.delete(broadcast_id).await?;

        let repo = self.repo.clone();
        let redis = self.redis.clone();
        let broadcast_clone = broadcast.clone();
        let creator_clone = creator.clone();
        let is_scheduled = broadcast.can_be_scheduled();
        let pubsub = Arc::clone(&app.pubsub);
        tokio::spawn(async move {
            let keys = vec![
                RedisKey::live_count(broadcast_id),
                RedisKey::host_grace(broadcast_id),
                RedisKey::started_at(broadcast_id),
            ];
            for key in keys {
                let _ = redis.del(&key).await;
            }

            if is_scheduled {
                if let Ok(subscriber_ids) = repo.get_subscriber_ids(creator_clone.id).await {
                    if !subscriber_ids.is_empty() {
                        let payload = WsPayload::new(
                            WsEvent::BroadcastDeleted,
                            serde_json::json!({
                                "broadcastId":    broadcast_clone.id,
                                "broadcastTitle": broadcast_clone.title,
                                "message": format!(
                                    "The broadcast scheduled by {} has been cancelled.",
                                    creator_clone.full_name,
                                ),
                            }),
                        );
                        // publish_to_users pipelines all PUBLISH commands in one round trip
                        // and reaches users on every instance.
                        pubsub.publish_to_users(&subscriber_ids, payload).await;
                    }
                }
            }
        });

        tracing::info!(
            "Broadcast deleted: id={}, title={}, deleted_by={}",
            broadcast_id,
            broadcast.title,
            creator_id,
        );

        Ok(())
    }

    #[tracing::instrument(
        name = "broadcast.start",
        skip(self, app),
        fields(
            broadcast_id = %broadcast_id,
            creator_id   = %user_id,
            room_created = tracing::field::Empty,
            token_minted = tracing::field::Empty,
        )
    )]
    pub async fn start(
        &self,
        app: &MenoState,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<BroadcastSessionResponse, BroadcastError> {
        let span = tracing::Span::current();

        // Fetch broadcast and user summary concurrently
        let (broadcast_result, creator_result) = tokio::join!(
            self.repo.find_by_id(broadcast_id),
            self.repo.find_user_summary(user_id),
        );

        let broadcast = broadcast_result?.ok_or(BroadcastError::NotFound)?;
        let creator = creator_result?.ok_or(BroadcastError::UserNotFound)?;

        // Confirm that the person trying to start the current broadcast is the creator
        if broadcast.creator_id != user_id {
            return Err(BroadcastError::NotCreator);
        }

        // Confirm that the current broadcast is not already live
        if broadcast.status == BroadcastStatus::Active {
            return Err(BroadcastError::AlreadyLive);
        }

        // Create new LiveKit room and generate a `broadcast_token` concurrently.
        let (room_result, broadcast_token_result) = tokio::join!(
            self.livekit.create_room(broadcast_id),
            self.livekit.mint_host_token(&creator, broadcast_id),
        );

        room_result?;
        span.record("room_created", true);
        tracing::debug!(broadcast_id = %broadcast_id, "LiveKit room created");

        let broadcast_token = broadcast_token_result.map_err(BroadcastError::LiveKitAccess)?;
        span.record("token_minted", true);
        tracing::debug!(broadcast_id = %broadcast_id, "HOST token minted");

        let now = OffsetDateTime::now_utc();
        let mut tx = app.db.begin().await?;

        // Set the broadcast status to `active`
        let set_active_input = SetActiveInput {
            broadcast_id,
            broadcast_token: broadcast_token.clone(),
            start_time: now.clone(),
        };
        let broadcast = self.repo.set_active(&set_active_input, &mut tx).await?;

        // Add the creator to the broadcast participants table, so others can see the creator in
        // the list of participants
        let part_input = UpsertParticipantInput {
            broadcast_id,
            participant_id: creator.id,
            role: &ParticipantRole::Host,
            joined_at: now,
        };
        self.repo
            .upsert_participant_tx(&part_input, &mut tx)
            .await?;

        // Commit everything using a transaction
        tx.commit().await?;

        // Cache Invalidation
        self.invalidate_list_caches();
        self.invalidate_broadcast_cache(broadcast_id);

        //  Seed the Redis room with the host as the first member so that
        if let Err(e) = app.pubsub.init_room(broadcast_id, &[creator.id]).await {
            tracing::warn!(error = %e, broadcast_id = %broadcast_id, "Failed to init room in Redis");
        }

        app.jobs
            .push_broadcast_started_fanout(BroadcastStartedFanOutJob {
                broadcast_id,
                creator_id: broadcast.creator_id,
                title: broadcast.title.clone(),
                image_url: broadcast.image_url.clone(),
            })
            .await
            .unwrap_or_else(|e| tracing::warn!(error=%e, "Failed to queue broadcast fanout"));

        // Generate the broadcast response struct
        let cohosts = self.repo.get_cohosts(broadcast_id).await?;
        let ctx = BroadcastContext {
            participant_id: Some(creator.id),
            participant_role: ParticipantRole::Host,
            live_count: 1,
            total_count: 1,
            ..Default::default()
        };
        let broadcast_response = self
            .build_response(broadcast, creator, cohosts, ctx)
            .await?;

        let redis = self.redis.clone();
        let svc = self.clone();
        let response_clone = broadcast_response.clone();
        let pubsub = Arc::clone(&app.pubsub);
        tokio::spawn(async move {
            let b_id = response_clone.id;
            let creator_id = response_clone.creator.id;

            // Store broadcast start time for quota
            let start_key = RedisKey::started_at(b_id);
            let _ = redis.set(&start_key, &now, None).await;

            // Set a live count key Redis to 1
            // This is the number of currently live participants, and the creator is the first
            // in the list
            let count_key = RedisKey::live_count(b_id);
            let _ = redis.set(&count_key, &1_i64, None).await;

            if let Ok(subscriber_ids) = svc.repo.get_subscriber_ids(creator_id).await {
                if !subscriber_ids.is_empty() {
                    let payload = WsPayload::new_broadcast(response_clone);
                    pubsub.publish_to_users(&subscriber_ids, payload).await;
                }
            }

            // Notify the FE of changes on the home page
            pubsub.broadcast_all(WsPayload::home_invalidated()).await;
        });

        tracing::info!(
            broadcast_id = %broadcast_id,
            creator_id   = %user_id,
            "broadcast went live"
        );

        Ok(BroadcastSessionResponse {
            broadcast: broadcast_response,
            token: broadcast_token,
        })
    }

    #[tracing::instrument(
        name = "broadcast.end",
        skip(self, app),
        fields(
            broadcast_id = %broadcast_id,
            requester_id = %user_id,
            total_participants = tracing::field::Empty,
            duration_secs = tracing::field::Empty,
        )
    )]
    pub async fn end(
        &self,
        app: &MenoState,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<EndBroadcastResponse, BroadcastError> {
        let span = tracing::Span::current();

        // Find the broadcast using the `broadcast_id`, return the appropriate error if not found
        let broadcast = self.repo.find_by_id_or_error(broadcast_id).await?;

        // Ensure the person trying to end the broadcast is the one that created it;
        // Only a broadcast creator (and the BE) can end a broadcast
        if broadcast.creator_id != user_id {
            return Err(BroadcastError::CannotEnd);
        }

        // Ensure that the broadcast is actually live; you cannot end what you did not start
        if broadcast.status != BroadcastStatus::Active {
            return Err(BroadcastError::NotLive);
        }

        // Calculate the total duration of the broadcast in seconds
        let ended_at = OffsetDateTime::now_utc();
        let duration_secs = broadcast
            .start_time
            .map(|s| (ended_at - s).whole_seconds())
            .unwrap_or(0);

        let mut tx = app.db.begin().await?;

        // We have to clear the list of participants from the `broadcast_participants` table,
        // but before we do, we need to return the list of participant IDs so we can calculate
        // the total number of participants and...
        let participant_ids = self
            .repo
            .get_participant_ids_and_clear(broadcast_id, &mut tx)
            .await?;

        // We set the broadcast status to `in-active`
        self.repo
            .set_inactive(broadcast_id, &EndReason::Normal, &mut tx)
            .await?;

        tx.commit().await?;

        // Cache Invalidation
        self.invalidate_list_caches();
        self.invalidate_broadcast_cache(broadcast_id);

        let livekit = self.livekit.clone();
        let redis = self.redis.clone();
        let reason = EndReason::Normal;
        let pubsub = Arc::clone(&app.pubsub);
        tokio::spawn(async move {
            let _ = livekit.delete_room(broadcast_id).await;

            let count_key = RedisKey::live_count(broadcast_id);
            let _ = redis.del(&count_key).await;

            let grace_key = RedisKey::host_grace(broadcast_id);
            let _ = redis.del(&grace_key).await;

            // Notify every participant on every instance that the broadcast ended.
            // publish_to_room targets only room members; broadcast_all targets
            // every connected user (non-members may also want to update their UI).
            // We do both: room members get it first via room channel, then
            // broadcast_all ensures the home feed update reaches non-participants.
            let ended_payload = WsPayload::ended_broadcast(broadcast_id, reason);
            pubsub.publish_to_room(broadcast_id, ended_payload).await;

            // HomeInvalidated is already sent via broadcast_all — no change needed there.

            // Tear down the Redis room membership set now that the broadcast is over.
            if let Err(e) = pubsub.destroy_room(broadcast_id).await {
                tracing::warn!(error = %e, broadcast_id = %broadcast_id, "Failed to destroy room");
            }
        });

        // Check if the recording is available and ready; this is only possible if the creator
        // enabled the `recording` flag when creating the broadcast
        let recording_ready = if broadcast.recording_enabled {
            let key = RedisKey::recording_ready(broadcast_id);
            self.redis.exists(&key).await.unwrap_or(false)
        } else {
            false
        };

        span.record("total_participants", participant_ids.len() as i64);
        span.record("duration_secs", duration_secs);
        tracing::info!(
            broadcast_id  = %broadcast_id,
            duration_secs = duration_secs,
            participants  = participant_ids.len(),
            "broadcast ended"
        );

        // Return the ended broadcast response
        Ok(EndBroadcastResponse {
            broadcast_id,
            broadcast_title: broadcast.title,
            broadcast_image_url: broadcast.image_url,
            creator_id: broadcast.creator_id,
            ended_reason: EndReason::Normal,
            ended_at,
            duration_secs,
            total_participants: participant_ids.iter().len() as i64,
            recording_enabled: broadcast.recording_enabled,
            recording_ready,
        })
    }

    #[tracing::instrument(
        name = "broadcast.join",
        skip(self, app),
        fields(
            broadcast_id = %broadcast_id,
            user_id = %user_id,
            role = tracing::field::Empty,
        )
    )]
    pub async fn join(
        &self,
        app: &MenoState,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<BroadcastSessionResponse, BroadcastError> {
        let span = tracing::Span::current();

        let (broadcast_result, user_result, is_cohost_result, is_participant_result) = tokio::join!(
            self.repo.find_by_id(broadcast_id),
            self.repo.find_user_summary(user_id),
            self.repo.is_cohost(broadcast_id, user_id),
            self.repo.find_active_participant(user_id)
        );

        let broadcast = broadcast_result?.ok_or(BroadcastError::NotFound)?;
        let user = user_result?.ok_or(BroadcastError::UserNotFound)?;
        let is_cohost = is_cohost_result?;

        if broadcast.status != BroadcastStatus::Active {
            return Err(BroadcastError::NotLive);
        }

        if broadcast.creator_id == user_id {
            return Err(BroadcastError::CreatorCannotJoin);
        }

        if let Some(participant) = is_participant_result? {
            if participant.broadcast_id == broadcast_id {
                return Err(BroadcastError::AlreadyParticipant);
            }

            return Err(BroadcastError::AlreadyAParticipant);
        }

        let role = if is_cohost {
            ParticipantRole::Cohost
        } else {
            ParticipantRole::Participant
        };
        span.record("role", role.clone().to_string().as_str());

        let livekit_role = match &role {
            ParticipantRole::Cohost => LivekitRole::Cohost,
            ParticipantRole::Participant => LivekitRole::Participant,
            _ => LivekitRole::Participant,
        };

        let broadcast_token = self
            .livekit
            .mint_token(user_id, &user.full_name, broadcast_id, livekit_role)
            .await
            .map_err(BroadcastError::LiveKitAccess)?;

        let now = OffsetDateTime::now_utc();

        let mut tx = app.db.begin().await?;
        self.repo
            .upsert_participant_tx(
                &UpsertParticipantInput {
                    broadcast_id,
                    participant_id: user_id,
                    role: &role,
                    joined_at: now,
                },
                &mut tx,
            )
            .await?;
        tx.commit().await?;

        // Cache Invalidation
        self.invalidate_list_caches();
        self.invalidate_broadcast_cache(broadcast_id);

        app.ws.join_room(user_id, broadcast.id, &app.pubsub).await;

        let pubsub = Arc::clone(&app.pubsub);
        let redis = self.redis.clone();
        let b_id = broadcast.id;
        let user_clone = user.clone();
        tokio::spawn(async move {
            let count_key = RedisKey::live_count(b_id);
            let new_count = redis.incr(&count_key).await.unwrap_or(1);

            let joined_payload = WsPayload::participant_joined(user_clone);
            pubsub.publish_to_room(b_id, joined_payload).await;

            let count_payload = WsPayload::number_of_live_participants(b_id, new_count);
            pubsub.publish_to_room(b_id, count_payload).await;
        });

        let (creator_result, cohosts_result, total_count_result) = tokio::join!(
            self.repo.find_user_summary(broadcast.creator_id),
            self.repo.get_cohosts(broadcast_id),
            self.repo.get_total_participants(broadcast_id),
        );

        let creator = creator_result?.ok_or(BroadcastError::UserNotFound)?;
        let cohosts = cohosts_result?;
        let total_count = total_count_result?;

        let live_count_key = RedisKey::live_count(broadcast_id);
        let live_count = self.redis.get::<i64>(&live_count_key).await?.unwrap_or(1);

        let ctx = self
            .build_ctx(
                &broadcast,
                Some(user_id),
                Some(role.clone()),
                live_count,
                total_count,
            )
            .await?;

        let broadcast_response = self
            .build_response(broadcast, creator, cohosts, ctx)
            .await?;

        tracing::info!(
            broadcast_id = %broadcast_id,
            user_id      = %user_id,
            role         = %role,
            "user joined broadcast"
        );

        Ok(BroadcastSessionResponse {
            broadcast: broadcast_response,
            token: broadcast_token,
        })
    }

    pub async fn leave(
        &self,
        app: &MenoState,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<LeaveBroadcastResponse, BroadcastError> {
        let (broadcast_result, user_result, participant_result) = tokio::join!(
            self.repo.find_by_id(broadcast_id),
            self.repo.find_user_summary(user_id),
            self.repo.find_participant(broadcast_id, user_id),
        );

        let broadcast = broadcast_result?.ok_or(BroadcastError::NotFound)?;
        let user = user_result?.ok_or(BroadcastError::UserNotFound)?;
        let participant = participant_result?.ok_or(BroadcastError::NotParticipant)?;

        if broadcast.is_not_active() || participant.left_at.is_some() {
            let left_at = participant.left_at.unwrap_or_else(OffsetDateTime::now_utc);
            return Ok(LeaveBroadcastResponse {
                success: true,
                broadcast_id,
                user_id,
                left_at,
            });
        }

        self.repo.remove_participant(broadcast_id, user_id).await?;

        // Cache Invalidation
        self.invalidate_list_caches();
        self.invalidate_broadcast_cache(broadcast_id);

        app.ws.leave_room(user_id, broadcast_id, &app.pubsub).await;

        let pubsub = Arc::clone(&app.pubsub);
        let redis = self.redis.clone();
        let user_clone = user.clone();
        tokio::spawn(async move {
            let count_key = RedisKey::live_count(broadcast_id);
            let remaining = redis.decr(&count_key).await.unwrap_or(0).max(0);
            if remaining == 0 {
                let _ = redis.del(&count_key).await;
            }

            let payload = WsPayload::participant_left(user_clone);
            pubsub.publish_to_room(broadcast_id, payload).await;
        });

        tracing::info!(
            broadcast_id = %broadcast_id,
            user_id      = %user_id,
            "user left broadcast"
        );

        Ok(LeaveBroadcastResponse {
            success: true,
            broadcast_id,
            user_id,
            left_at: OffsetDateTime::now_utc(),
        })
    }

    #[tracing::instrument(
        name = "broadcast.add_cohost",
        skip(self, app),
        fields(broadcast_id = %broadcast_id, requester = %requester_id, cohost = %cohost_id)
    )]
    pub async fn add_cohost(
        &self,
        app: &MenoState,
        broadcast_id: Uuid,
        requester_id: Uuid,
        cohost_id: Uuid,
    ) -> Result<CohostSessionResponse, BroadcastError> {
        if self.repo.is_cohost(broadcast_id, cohost_id).await? {
            return Err(BroadcastError::AlreadyCohost);
        }

        let (broadcast_result, cohost_user_result, cohosts_result) = tokio::join!(
            self.repo.find_by_id(broadcast_id),
            self.repo.find_user_summary(cohost_id),
            self.repo.get_cohosts(broadcast_id),
        );

        let broadcast = broadcast_result?.ok_or(BroadcastError::NotFound)?;
        let cohost = cohost_user_result?.ok_or(BroadcastError::UserNotFound)?;

        if let Ok(cohosts) = cohosts_result {
            if cohosts.len() == MAX_COHOSTS {
                return Err(BroadcastError::CohostLimitExceeded(MAX_COHOSTS));
            }
        }

        if broadcast.status != BroadcastStatus::Active {
            return Err(BroadcastError::NotLive);
        }

        if broadcast.creator_id != requester_id {
            return Err(BroadcastError::NotCreator);
        }

        if broadcast.creator_id == cohost_id {
            return Err(BroadcastError::CannotAddSelfAsCohost);
        }

        let broadcast_token = self
            .livekit
            .mint_cohost_token(&cohost, broadcast_id)
            .await
            .map_err(BroadcastError::LiveKitAccess)?;

        let now = OffsetDateTime::now_utc();

        let mut tx = app.db.begin().await?;

        self.repo
            .add_cohosts(broadcast_id, &[cohost_id], requester_id, &mut tx)
            .await?;

        self.repo
            .upsert_participant_tx(
                &UpsertParticipantInput {
                    broadcast_id,
                    participant_id: cohost_id,
                    role: &ParticipantRole::Cohost,
                    joined_at: now,
                },
                &mut tx,
            )
            .await?;

        tx.commit().await?;

        let pubsub = Arc::clone(&app.pubsub);
        let token = broadcast_token.clone();
        tokio::spawn(async move {
            let payload = WsPayload::cohost_invitation(broadcast_id, token);
            pubsub.publish_to_user(cohost_id, payload).await;
        });

        tracing::info!(
            broadcast_id = %broadcast_id,
            cohost_id    = %cohost_id,
            "cohost added"
        );

        Ok(CohostSessionResponse {
            user: cohost,
            token: broadcast_token,
        })
    }

    #[tracing::instrument(
        name = "broadcast.remove_cohost",
        skip(self, app),
        fields(
            broadcast_id    = %broadcast_id,
            cohost_id       = %cohost_id,
            remove_from_room = remove_from_room,
        )
    )]
    pub async fn remove_cohost(
        &self,
        app: &MenoState,
        broadcast_id: Uuid,
        cohost_id: Uuid,
        requester_id: Uuid,
        remove_from_room: bool,
    ) -> Result<(), BroadcastError> {
        let (broadcast_result, cohost_user_result, is_cohost_result) = tokio::join!(
            self.repo.find_by_id(broadcast_id),
            self.repo.find_user_summary(cohost_id),
            self.repo.is_cohost(broadcast_id, cohost_id),
        );

        let broadcast = broadcast_result?.ok_or(BroadcastError::NotFound)?;
        let cohost_user = cohost_user_result?.ok_or(BroadcastError::UserNotFound)?;
        let is_cohost = is_cohost_result?;

        if broadcast.status != BroadcastStatus::Active {
            return Err(BroadcastError::NotLive);
        }

        if broadcast.creator_id != requester_id {
            return Err(BroadcastError::NotCreator);
        }

        if !is_cohost {
            return Ok(());
        }

        let mut tx = app.db.begin().await?;

        self.repo
            .remove_cohost_tx(broadcast_id, cohost_id, &mut tx)
            .await?;

        let new_role = if remove_from_room {
            self.repo
                .remove_participant_tx(broadcast_id, cohost_id, &mut tx)
                .await?;
            ParticipantRole::None
        } else {
            ParticipantRole::Participant
        };

        if !remove_from_room {
            self.repo
                .upsert_participant_tx(
                    &UpsertParticipantInput {
                        broadcast_id,
                        participant_id: cohost_id,
                        role: &new_role,
                        joined_at: OffsetDateTime::now_utc(),
                    },
                    &mut tx,
                )
                .await?;
        }

        tx.commit().await?;

        if remove_from_room {
            // Remove from room completely
            let _ = self
                .livekit
                .remove_participant(broadcast_id, cohost_id)
                .await;

            // Decrement live count (cohost was a live participant)
            let redis = self.redis.clone();
            let pubsub = Arc::clone(&app.pubsub);
            tokio::spawn(async move {
                let count_key = RedisKey::live_count(broadcast_id);
                let remaining = redis.decr(&count_key).await.unwrap_or(0).max(0);

                let payload = WsPayload::number_of_live_participants(broadcast_id, remaining);
                pubsub.publish_to_room(broadcast_id, payload).await;
            });
        } else {
            // Update LiveKit permissions (revokes publish capability)
            // On Cloud: This automatically invalidates the old token
            // On Self-hosted: Permissions change, but token remains valid
            let _ = self
                .livekit
                .update_permission(broadcast_id, cohost_id, false)
                .await;

            // Mint new participant token (lower permissions)
            let new_token = self
                .livekit
                .mint_participant_token(&cohost_user, broadcast_id)
                .await
                .map_err(BroadcastError::LiveKitAccess)?;

            // Send token via WebSocket
            // Client will use this to either:
            //   a) Reconnect if disconnected (Cloud), or
            //   b) Refresh their token (Self-hosted)
            let payload = WsPayload::cohost_demotion(broadcast_id, new_token);
            app.pubsub.publish_to_user(cohost_id, payload).await;
        }

        let pubsub = Arc::clone(&app.pubsub);
        let cohost_clone = cohost_user.clone();
        tokio::spawn(async move {
            let payload = WsPayload::removed_cohost(cohost_clone);
            pubsub.publish_to_room(broadcast_id, payload).await;
        });

        tracing::info!(
            broadcast_id     = %broadcast_id,
            cohost_id        = %cohost_id,
            remove_from_room = remove_from_room,
            "cohost removed"
        );

        Ok(())
    }

    pub async fn get_broadcast(
        &self,
        broadcast_id: Uuid,
    ) -> Result<BroadcastResponse, BroadcastError> {
        let key = RedisKey::broadcast(broadcast_id);
        let response = coalesce_cache(&self.redis, &key.as_ref(), TTL_60_SECS, || async {
            let broadcast = self
                .repo
                .find_by_id(broadcast_id)
                .await?
                .ok_or(BroadcastError::NotFound)?;

            let (creator_result, cohosts_result, total_participants_result) = tokio::join!(
                self.repo.find_user_summary(broadcast.creator_id),
                self.repo.get_cohosts(broadcast_id),
                self.repo.get_total_participants(broadcast_id),
            );

            let creator = creator_result?.ok_or(BroadcastError::UserNotFound)?;
            let cohosts = cohosts_result?;
            let total_count = total_participants_result?;

            let live_count = if broadcast.is_active() {
                let key = RedisKey::live_count(broadcast_id);
                self.redis.get::<i64>(&key).await?.unwrap_or(1)
            } else {
                0
            };

            let ctx = self
                .build_ctx(&broadcast, None, None, live_count, total_count)
                .await?;

            self.build_response(broadcast, creator, cohosts, ctx).await
        })
        .await?;

        Ok(response)
    }

    #[tracing::instrument(name = "broadcast_service.get_broadcasts", skip(self))]
    pub async fn get_broadcasts(
        &self,
        query: &BroadcastQuery,
        requester_id: Option<Uuid>,
    ) -> Result<CursorPage<BroadcastListItem>, BroadcastError> {
        let ttl = if query.status == Some(BroadcastStatus::Active) {
            TTL_10_SECS
        } else {
            TTL_30_SECS
        };

        let start = std::time::Instant::now();

        let broadcast_list_cache_key = BroadcastListCacheKey::build(&query);
        if !broadcast_list_cache_key.is_some() {
            // Personalized query - go directly to DB without caching
            tracing::debug!("Personalized query (only_subscriptions=true) — skipping cache");
            return self.get_broadcasts_from_db(&query, requester_id).await;
        }

        let cache_key = broadcast_list_cache_key.unwrap();

        // Use coalescing cache to prevent thundering herd
        let page = coalesce_cache(&self.redis, &cache_key, ttl, || async {
            self.get_broadcasts_from_db(&query, requester_id).await
        })
        .await?;

        let elapsed = start.elapsed();
        tracing::debug!(
            returned = page.data.len(),
            elapsed_ms = elapsed.as_millis(),
            "broadcast list query complete"
        );

        Ok(page)
    }

    /// Now live — all active broadcasts, sorted by live participant count desc.
    /// Cached for 15 seconds: fast-changing but expensive to compute.
    pub async fn get_now_live(
        &self,
        query: &BroadcastQuery,
    ) -> Result<CursorPage<BroadcastListItem>, BroadcastError> {
        let mut q = query.clone();
        q.status = Some(BroadcastStatus::Active);
        q.sort_by = Some(BroadcastSortBy::TotalParticipants);
        q.order = Some(Order::Desc);

        let limit = q.limit();
        let cursor_str = q.cursor().map(|c| c.0.clone()).unwrap_or_default();
        let cache_key = format!("bc:now_live:cur={cursor_str}:lim={limit}");

        coalesce_cache(&self.redis, &cache_key, TTL_10_SECS, || async {
            self.get_broadcasts_from_db(&q, None).await
        })
        .await
    }

    /// Live for you — active broadcasts from creators the user follows.
    /// NOT cached (personalized per user).
    pub async fn get_live_for_you(
        &self,
        query: &BroadcastQuery,
        user_id: Uuid,
    ) -> Result<CursorPage<BroadcastListItem>, BroadcastError> {
        let mut q = query.clone();
        q.status = Some(BroadcastStatus::Active);
        q.only_subscriptions = Some(true);
        q.sort_by = Some(BroadcastSortBy::CreatedAt);
        q.order = Some(Order::Desc);

        let limit = q.limit();
        let cursor_str = q.cursor().map(|c| c.0.clone()).unwrap_or_default();
        let cache_key = format!(
            "bc:live_for_you:v={v}:cur={cur}:lim={lim}",
            v = user_id,
            cur = cursor_str,
            lim = limit,
        );

        coalesce_cache(&self.redis, &cache_key, TTL_10_SECS, || async {
            self.get_broadcasts_from_db(&q, None).await
        })
        .await
    }

    /// Recently live — ended in the last 24 hours, sorted by end_time desc.
    /// Cached for 5 minutes: content changes infrequently.
    pub async fn get_recently_live(
        &self,
        query: &BroadcastQuery,
    ) -> Result<CursorPage<BroadcastListItem>, BroadcastError> {
        let mut q = query.clone();
        q.status = Some(BroadcastStatus::Inactive);
        q.end_time_exists = Some(true);
        q.sort_by = Some(BroadcastSortBy::EndTime);
        q.order = Some(Order::Desc);

        let now = OffsetDateTime::now_utc();
        let yesterday = now - time::Duration::hours(24);
        q.end_time_gte = Some(yesterday);
        q.end_time_lte = Some(now);

        let limit = q.limit();
        let cursor_str = q.cursor().map(|c| c.0.clone()).unwrap_or_default();
        let cache_key = format!("bc:recently_live:cur={cursor_str}:lim={limit}");

        coalesce_cache(&self.redis, &cache_key, 300, || async {
            self.get_broadcasts_from_db(&q, None).await
        })
        .await
    }

    pub async fn get_participants(
        &self,
        broadcast_id: Uuid,
        query: &ParticipantQuery,
    ) -> Result<CursorPage<ParticipantListItem>, BroadcastError> {
        let cache_key = ParticipantListCacheKey::all(broadcast_id, &query);
        coalesce_cache(&self.redis, &cache_key, TTL_30_SECS, || async {
            self.get_participants_from_db(broadcast_id, &query).await
        })
        .await
    }

    #[tracing::instrument(
        name = "broadcast_service.get_live_participants",
        skip(self),
        fields(broadcast_id = %broadcast_id)
    )]
    pub async fn get_live_participants(
        &self,
        broadcast_id: Uuid,
        query: &ParticipantQuery,
    ) -> Result<CursorPage<ParticipantListItem>, BroadcastError> {
        let broadcast = self
            .repo
            .find_by_id(broadcast_id)
            .await?
            .ok_or(BroadcastError::NotFound)?;

        if !broadcast.is_active() {
            return Err(BroadcastError::NotLive);
        }

        let lk_participants = self.livekit.list_participants(broadcast_id).await?;

        if lk_participants.is_empty() {
            return Ok(CursorPage {
                data: Vec::new(),
                next_cursor: None,
                has_next_page: false,
            });
        }

        let user_ids: Vec<Uuid> = lk_participants.iter().map(|p| p.id).collect();
        let (users, role_map) = tokio::join!(
            self.repo.find_users_batch(&user_ids),
            self.repo.get_participant_roles_batch(broadcast_id)
        );

        let users = users?;
        let role_map = role_map?;
        let user_map: HashMap<Uuid, UserSummary> = users.into_iter().map(|u| (u.id, u)).collect();

        // Get roles from broadcast_participants table
        let mut enriched: Vec<ParticipantListItem> = lk_participants
            .into_iter()
            .map(|lp| {
                let user = user_map.get(&lp.id);

                // Role from DB map; fall back to None if somehow missing
                let role = role_map.get(&lp.id).cloned().unwrap_or_else(|| {
                    if broadcast.creator_id == lp.id {
                        ParticipantRole::Host
                    } else {
                        ParticipantRole::Participant
                    }
                });

                ParticipantListItem {
                    id: lp.id,
                    full_name: user
                        .map(|u| u.full_name.clone())
                        .unwrap_or_else(|| "Unknown".to_string()),
                    avatar_id: user.and_then(|u| u.avatar_id.clone()),
                    avatar_url: user.and_then(|u| u.avatar_url.clone()),
                    role,
                    joined_at: lp.joined_at,
                }
            })
            .collect();

        // Apply search filter (keywords)
        if let Some(ref kw) = query.keywords {
            let kw_lower = kw.to_lowercase();
            enriched.retain(|p| p.full_name.to_lowercase().contains(&kw_lower));
        }
        if let Some(ref role_filter) = query.role {
            enriched.retain(|p| &p.role == role_filter);
        }

        let sort = query.sort_by.unwrap_or(ParticipantSortBy::Role);
        let order = query.order.unwrap_or(Order::Asc);

        enriched.sort_by(|a, b| {
            let comparison = match sort {
                ParticipantSortBy::Role => {
                    let a_priority = a.role.priority();
                    let b_priority = b.role.priority();
                    a_priority.cmp(&b_priority)
                }
                ParticipantSortBy::JoinedAt => a.joined_at.cmp(&b.joined_at),
                ParticipantSortBy::Name => a.full_name.cmp(&b.full_name),
            };

            match order {
                Order::Asc => comparison,
                Order::Desc => comparison.reverse(),
            }
        });

        let limit = query.limit();
        let cursor = query.cursor();

        let start_index = if let Some(cursor) = cursor {
            match sort {
                ParticipantSortBy::Role => {
                    let (priority, id) = cursor.to_score_id().map_err(BroadcastError::Cursor)?;
                    enriched
                        .iter()
                        .position(|p| {
                            let p_priority = p.role.priority();
                            (p_priority, p.id) > (priority, id)
                        })
                        .unwrap_or(0)
                }
                ParticipantSortBy::JoinedAt => {
                    let (ts, id) = cursor.to_timestamp_id().map_err(BroadcastError::Cursor)?;
                    enriched
                        .iter()
                        .position(|p| (p.joined_at, p.id) > (ts, id))
                        .unwrap_or(0)
                }
                ParticipantSortBy::Name => {
                    let (name, id) = cursor.to_name_id().map_err(BroadcastError::Cursor)?;
                    enriched
                        .iter()
                        .position(|p| (p.full_name.clone(), p.id) > (name.clone(), id))
                        .unwrap_or(0)
                }
            }
        } else {
            0
        };

        let items: Vec<ParticipantListItem> = enriched
            .into_iter()
            .skip(start_index)
            .take(limit as usize)
            .collect();

        tracing::debug!(
            broadcast_id = %broadcast_id,
            total_live = items.len(),
            "live participant list resolved with cursor pagination"
        );

        Ok(CursorPage::from_rows(items, limit, |p| match sort {
            ParticipantSortBy::Role => Cursor::from_score_id(p.role.priority(), p.id),
            ParticipantSortBy::JoinedAt => Cursor::from_timestamp_id(p.joined_at, p.id),
            ParticipantSortBy::Name => Cursor::from_name_id(&p.full_name, p.id),
        }))
    }

    #[tracing::instrument(
        name = "broadcast_service.refresh_token",
        skip(self),
        fields(broadcast_id = %broadcast_id, user_id = %user_id)
    )]
    pub async fn refresh_token(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<BroadcastRefreshTokenResponse, BroadcastError> {
        let (broadcast_result, participant_result, user_result) = tokio::join!(
            self.repo.find_by_id(broadcast_id),
            self.repo.find_participant(broadcast_id, user_id),
            self.repo.find_user_summary(user_id),
        );

        let broadcast = broadcast_result?.ok_or(BroadcastError::NotFound)?;
        let user = user_result?.ok_or(BroadcastError::UserNotFound)?;

        if !broadcast.is_active() {
            return Err(BroadcastError::NotLive);
        }

        // Determine the participant's CURRENT role from DB (never trust the client)
        let role = if broadcast.creator_id == user_id {
            ParticipantRole::Host
        } else {
            match participant_result? {
                None => return Err(BroadcastError::NotParticipant),
                Some(p) if p.left_at.is_some() => return Err(BroadcastError::NotParticipant),
                Some(p) => p.role,
            }
        };

        let livekit_role =
            LivekitRole::try_from(role.clone()).map_err(|_| BroadcastError::NotParticipant)?;

        // Mint a fresh token with the current (possibly downgraded) role
        let token = self
            .livekit
            .mint_token_with_attributes(
                user_id,
                &user.full_name,
                broadcast_id,
                livekit_role,
                HashMap::from([
                    ("meno_role".to_string(), role.to_string()),
                    ("meno_broadcast_id".to_string(), broadcast_id.to_string()),
                ]),
            )
            .await
            .map_err(BroadcastError::LiveKitAccess)?;

        // Token TTL is 6 hours from now
        let expires_at = OffsetDateTime::now_utc() + time::Duration::hours(6);

        tracing::info!(
            broadcast_id = %broadcast_id,
            user_id = %user_id,
            role = %role,
            "LiveKit token refreshed"
        );

        Ok(BroadcastRefreshTokenResponse {
            broadcast_id,
            token,
            expires_at,
        })
    }

    /// Find active broadcast hosted by user
    /// Returns the broadcast if found, otherwise None
    pub async fn find_active_hosted_by(
        &self,
        user_id: Uuid,
    ) -> Result<Option<Broadcast>, BroadcastError> {
        self.repo.find_active_broadcast_hosted_by_id(user_id).await
    }

    pub async fn find_active_participant(
        &self,
        user_id: Uuid,
    ) -> Result<Option<BroadcastParticipant>, BroadcastError> {
        self.repo.find_active_participant(user_id).await
    }

    pub async fn remove_participant(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), BroadcastError> {
        self.repo.remove_participant(broadcast_id, user_id).await
    }

    pub async fn get_participants_ids(
        &self,
        broadcast_id: Uuid,
    ) -> Result<Vec<Uuid>, BroadcastError> {
        self.repo.get_participant_ids(broadcast_id).await
    }

    pub async fn find_by_id(
        &self,
        broadcast_id: Uuid,
    ) -> Result<Option<Broadcast>, BroadcastError> {
        self.repo.find_by_id(broadcast_id).await
    }

    /// Check if user is currently hosting any active broadcast
    /// Useful for quick checks without fetching full broadcast data
    pub async fn is_active_host(&self, user_id: Uuid) -> Result<bool, BroadcastError> {
        self.repo.is_active_host(user_id).await
    }

    /// Invalidate ALL broadcast-list cache entries.
    ///
    /// Call this whenever the global list could have changed:
    ///   - `go_live` (a new active broadcast appears)
    ///   - `end_broadcast` (an active broadcast disappears)
    ///   - `create` (a new draft / scheduled broadcast appears)
    ///   - `delete` (a broadcast disappears)
    fn invalidate_list_caches(&self) {
        let redis = self.redis.clone();
        tokio::spawn(async move {
            if let Err(e) = redis.delete_by_pattern("home:now_live:*").await {
                tracing::warn!(
                    error = %e,
                    "Failed to invalidate `Now Live` list cache"
                );
            } else {
                tracing::debug!("`Now Live` list cache invalidated");
            }

            if let Err(e) = redis.delete_by_pattern("home:recently_live:*").await {
                tracing::warn!(
                    error = %e,
                    "Failed to invalidate `Recently Live` list cache"
                );
            } else {
                tracing::debug!("`Recently Live` list cache invalidated");
            }

            if let Err(e) = redis.delete_by_pattern("bl:*").await {
                tracing::warn!(
                    error = %e,
                    "Failed to invalidate broadcast list cache"
                );
            } else {
                tracing::debug!("Broadcast list cache invalidated");
            }

            if let Err(e) = redis.delete_by_pattern("pl:*").await {
                tracing::warn!(
                    error = %e,
                    "Failed to invalidate participant list cache"
                );
            } else {
                tracing::debug!("Participant list cache invalidated");
            }
        });
    }

    fn invalidate_broadcast_cache(&self, broadcast_id: Uuid) {
        let redis = self.redis.clone();
        tokio::spawn(async move {
            let key = RedisKey::broadcast(broadcast_id);
            let _ = redis.del(&key).await;
        });
    }

    // ==================== HELPERS ====================
    async fn build_response(
        &self,
        broadcast: Broadcast,
        creator: UserSummary,
        cohosts: Vec<UserSummary>,
        ctx: BroadcastContext,
    ) -> Result<BroadcastResponse, BroadcastError> {
        let broadcast_state = if broadcast.status == BroadcastStatus::Active {
            if ctx.is_reconnecting {
                BroadcastState::Reconnecting
            } else {
                BroadcastState::Live
            }
        } else {
            broadcast.get_partial_state()
        };

        let duration_in_seconds = match (broadcast.start_time, broadcast.end_time) {
            (Some(start), Some(end)) => {
                let difference = end - start;
                Some(difference.whole_seconds())
            }
            _ => None,
        };

        Ok(BroadcastResponse {
            id: broadcast.id,
            title: broadcast.title,
            description: broadcast.description,
            time_zone: broadcast.time_zone,
            image_id: broadcast.image_id,
            image_url: broadcast.image_url,
            created_at: broadcast.created_at,
            start_time: broadcast.start_time,
            end_time: broadcast.end_time,
            end_reason: broadcast.end_reason,
            published_at: broadcast.published_at,
            recording_enabled: broadcast.recording_enabled,
            recording_url: broadcast.recording_url,
            status: broadcast.status,
            state: broadcast_state,
            duration_seconds: duration_in_seconds,
            participant_role: ctx.participant_role,
            is_subscribed_to_creator: ctx.is_subscribed_to_creator,
            is_bookmarked: ctx.is_bookmarked,
            live_participants_count: ctx.live_count,
            total_participants: ctx.total_count,
            time_remaining_seconds: ctx.time_remaining_seconds,
            last_listened_at: ctx.last_listened_at,
            creator,
            cohosts,
        })
    }

    /// Gathers all viewer-specific signals for a broadcast in one batch.
    /// Runs the Redis + subscription + bookmark checks concurrently.
    async fn build_ctx(
        &self,
        broadcast: &Broadcast,
        participant_id: Option<Uuid>,
        participant_role: Option<ParticipantRole>,
        live_count: i64,
        total_count: i64,
    ) -> Result<BroadcastContext, BroadcastError> {
        let grace_key = RedisKey::host_grace(broadcast.id);
        let is_reconnecting = self.redis.exists(&grace_key).await.unwrap_or(false);

        let (is_subscribed, is_bookmarked) = match participant_id {
            None => (false, false),
            Some(pid) => {
                let (is_sub_res, is_bm_res) = tokio::join!(
                    self.repo.is_subscribed(pid, broadcast.creator_id),
                    self.repo.is_bookmarked(pid, broadcast.creator_id),
                );
                (is_sub_res?, is_bm_res?)
            }
        };

        Ok(BroadcastContext {
            participant_id,
            is_reconnecting,
            live_count,
            total_count,
            participant_role: participant_role.unwrap_or(ParticipantRole::Participant),
            participant_is_in_room: false,
            is_subscribed_to_creator: is_subscribed,
            is_bookmarked,
            time_remaining_seconds: None,
            last_listened_at: None,
        })
    }

    async fn deduplicate_cohosts(
        &self,
        cohost_ids: &Vec<Uuid>,
        creator_id: Uuid,
    ) -> Result<Vec<UserSummary>, BroadcastError> {
        if cohost_ids.is_empty() {
            Ok(vec![])
        } else {
            if cohost_ids.len() > 3 {
                return Err(BroadcastError::CohostLimitExceeded(MAX_COHOSTS));
            }

            if cohost_ids.contains(&creator_id) {
                return Err(BroadcastError::CannotAddSelfAsCohost);
            }

            // Handle Deduplicate here
            let mut deduped = cohost_ids.clone();
            deduped.sort_unstable();
            deduped.dedup();

            let users = self.repo.find_users_batch(&deduped).await?;
            if users.len() != deduped.len() {
                return Err(BroadcastError::OneOrMoreUsersNotFound);
            }

            Ok(users)
        }
    }

    /// Actual database fetch (extracted for use in coalesce_cache)
    async fn get_broadcasts_from_db(
        &self,
        query: &BroadcastQuery,
        requester_id: Option<Uuid>,
    ) -> Result<CursorPage<BroadcastListItem>, BroadcastError> {
        let limit = query.limit();
        let sort = query.effective_sort();
        let rows = self.repo.find_broadcasts(&query, requester_id).await?;
        let page = self.build_broadcast_page(rows, limit, sort);
        Ok(page)
    }

    async fn get_participants_from_db(
        &self,
        broadcast_id: Uuid,
        query: &ParticipantQuery,
    ) -> Result<CursorPage<ParticipantListItem>, BroadcastError> {
        let limit = query.limit();
        let rows = self.repo.find_participants(broadcast_id, &query).await?;
        Ok(CursorPage::from_rows(rows, limit, |r| {
            Cursor::from_timestamp_id(r.joined_at, r.id)
        }))
    }

    fn build_broadcast_page(
        &self,
        rows: Vec<BroadcastListItem>,
        limit: i64,
        sort: BroadcastSortBy,
    ) -> CursorPage<BroadcastListItem> {
        CursorPage::from_rows(rows, limit, |r: &BroadcastListItem| match sort {
            BroadcastSortBy::Title | BroadcastSortBy::CreatedAt => {
                Cursor::from_timestamp_id(r.created_at, r.id)
            }
            BroadcastSortBy::StartTime => {
                let ts = r.start_time.unwrap_or(OffsetDateTime::now_utc());
                Cursor::from_timestamp_id(ts, r.id)
            }
            BroadcastSortBy::EndTime => {
                let ts = r.end_time.unwrap_or(r.created_at);
                Cursor::from_timestamp_id(ts, r.id)
            }
            BroadcastSortBy::TotalParticipants => Cursor::from_score_id(r.total_participants, r.id),
        })
    }
}
