use crate::modules::broadcast::dto::{
    BroadcastResponse, BroadcastSessionResponse, CohostSessionResponse, CreateBroadcastRequest,
    EndBroadcastResponse, LeaveBroadcastResponse, MAX_COHOSTS, UpdateBroadcastRequest, UserSummary,
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
use crate::shared::constants::TTL_60_SECS;
use crate::shared::services::livekit::LivekitService;
use crate::shared::services::livekit::dto::LivekitRole;
use crate::shared::services::redis::RedisService;
use crate::shared::services::redis::keys::RedisKey;
use crate::shared::services::ws::WsService;
use crate::shared::services::ws::dto::WsPayload;
use crate::shared::services::ws::model::WsEvent;
use crate::state::MenoState;
use serde_json::to_value;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct BroadcastService {
    repo: BroadcastRepository,
    livekit: LivekitService,
    redis: RedisService,
    ws: WsService,
}
impl BroadcastService {
    pub fn new(
        repo: BroadcastRepository,
        livekit: LivekitService,
        redis: RedisService,
        ws: WsService,
    ) -> Self {
        Self {
            repo,
            livekit,
            redis,
            ws,
        }
    }

    pub async fn create(
        &self,
        state: &MenoState,
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

        let mut tx = state.db.begin().await?;

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

        self.broadcast_to_response(broadcast, creator, cohosts, ctx)
            .await
    }

    pub async fn update(
        &self,
        state: &MenoState,
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

        let mut tx = state.db.begin().await?;

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

        let ctx = BroadcastContext {
            participant_id: Some(creator_id),
            participant_role: ParticipantRole::Host,
            is_subscribed_to_creator: false,
            is_bookmarked: false,
            live_count: 0,
            total_count: 0,
            ..Default::default()
        };

        self.broadcast_to_response(broadcast, creator, cohosts, ctx)
            .await
    }

    pub async fn delete(&self, broadcast_id: Uuid, creator_id: Uuid) -> Result<(), BroadcastError> {
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
        let ws = self.ws.clone();
        let broadcast_clone = broadcast.clone();
        let creator_clone = creator.clone();
        let is_scheduled = broadcast.can_be_scheduled();

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
                                "broadcastId": broadcast_clone.id,
                                "broadcastTitle": broadcast_clone.title,
                                "message": format!(
                                    "The broadcast scheduled for this {:?}, by {} has been cancelled.",
                                    broadcast_clone.start_time,
                                    creator_clone.full_name,
                                ),
                            }),
                        );
                        ws.send_to_users(&subscriber_ids, payload).await;
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

    pub async fn start(
        &self,
        state: &MenoState,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<BroadcastSessionResponse, BroadcastError> {
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

        room_result.map_err(BroadcastError::LiveKit)?;
        let broadcast_token = broadcast_token_result.map_err(BroadcastError::LiveKitAccess)?;

        let now = OffsetDateTime::now_utc();
        let mut tx = state.db.begin().await?;

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

        let redis = self.redis.clone();
        let ws = self.ws.clone();
        let broadcast_clone = broadcast.clone();
        let creator_clone = creator.clone();
        let svc = self.clone();

        tokio::spawn(async move {
            // Store broadcast start time for quota
            let start_key = RedisKey::started_at(broadcast_clone.id);
            let _ = redis.set(&start_key, &now, None).await;

            // Set a live count key Redis to 1
            // This is the number of currently live participants, and the creator is the first
            // in the list
            let count_key = RedisKey::live_count(broadcast_clone.id);
            let _ = redis.set(&count_key, &1_i64, None).await;

            let online_users = ws.get_online_users();
            if !online_users.is_empty() {
                if let Ok(response) = svc
                    .broadcast_to_response(
                        broadcast_clone,
                        creator_clone,
                        vec![],
                        BroadcastContext {
                            participant_role: ParticipantRole::None,
                            live_count: 1,
                            ..Default::default()
                        },
                    )
                    .await
                {
                    let payload = WsPayload {
                        event: WsEvent::NewBroadcast,
                        data: to_value(&response).unwrap_or_default(),
                    };
                    ws.send_to_users(&online_users, payload).await;
                }
            }
        });

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
            .broadcast_to_response(broadcast, creator, cohosts, ctx)
            .await?;

        Ok(BroadcastSessionResponse {
            broadcast: broadcast_response,
            token: broadcast_token,
        })
    }

    pub async fn end(
        &self,
        state: &MenoState,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<EndBroadcastResponse, BroadcastError> {
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

        let mut tx = state.db.begin().await?;

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

        let livekit = self.livekit.clone();
        let redis = self.redis.clone();
        let ws = self.ws.clone();
        let broadcast_id_clone = broadcast_id;
        let reason = EndReason::Normal;

        tokio::spawn(async move {
            // Delete the LiveKit room
            let _ = livekit.delete_room(broadcast_id).await;

            // Clean up the Redis key holding the number of currently live participants
            let count_key = RedisKey::live_count(broadcast_id);
            let _ = redis.del(&count_key).await;

            // Clean the `grace-key` from Redis
            let grace_key = RedisKey::host_grace(broadcast_id);
            let _ = redis.del(&grace_key).await;

            let online_users = ws.get_online_users();
            let payload = WsPayload::ended_broadcast(broadcast_id_clone, reason);
            ws.send_to_users(&online_users, payload).await;
        });

        // Check if the recording is available and ready; this is only possible if the creator
        // enabled the `recording` flag when creating the broadcast
        let recording_ready = if broadcast.recording_enabled {
            let key = RedisKey::recording_ready(broadcast_id);
            self.redis.exists(&key).await.unwrap_or(false)
        } else {
            false
        };

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

    pub async fn join(
        &self,
        state: &MenoState,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<BroadcastSessionResponse, BroadcastError> {
        let (broadcast_result, user_result, is_cohost_result) = tokio::join!(
            self.repo.find_by_id(broadcast_id),
            self.repo.find_user_summary(user_id),
            self.repo.is_cohost(broadcast_id, user_id),
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

        let role = if is_cohost {
            ParticipantRole::Cohost
        } else {
            ParticipantRole::Participant
        };

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

        let mut tx = state.db.begin().await?;
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

        let repo = self.repo.clone();
        let ws = self.ws.clone();
        let redis = self.redis.clone();
        let broadcast_clone = broadcast.clone();
        let user_clone = user.clone();

        tokio::spawn(async move {
            let count_key = RedisKey::live_count(broadcast_clone.id);
            let new_count = redis.incr(&count_key).await.unwrap_or(1);
            let _ = redis.set(&count_key, &new_count, None).await;

            if let Ok(participant_ids) = repo.get_participant_ids(broadcast_clone.id).await {
                if !participant_ids.is_empty() {
                    let payload = WsPayload::participant_joined(user_clone);
                    ws.send_to_users(&participant_ids, payload).await;

                    let count_payload =
                        WsPayload::number_of_live_participants(broadcast_clone.id, new_count);
                    ws.send_to_users(&participant_ids, count_payload).await;
                }
            }
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
                Some(role),
                live_count,
                total_count,
            )
            .await?;

        let broadcast_response = self
            .broadcast_to_response(broadcast, creator, cohosts, ctx)
            .await?;

        Ok(BroadcastSessionResponse {
            broadcast: broadcast_response,
            token: broadcast_token,
        })
    }

    pub async fn leave(
        &self,
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

        if broadcast.status != BroadcastStatus::Active || participant.left_at.is_some() {
            return Ok(LeaveBroadcastResponse {
                success: true,
                broadcast_id,
                user_id,
                left_at: participant.left_at.unwrap(),
            });
        }

        self.repo.remove_participant(broadcast_id, user_id).await?;

        let ws = self.ws.clone();
        let redis = self.redis.clone();
        let repo = self.repo.clone();
        let user_clone = user.clone();

        tokio::spawn(async move {
            let count_key = RedisKey::live_count(broadcast_id);
            let remaining_count = redis.decr(&count_key).await.unwrap_or(0).max(0);

            if remaining_count == 0 {
                let _ = redis.del(&count_key).await;
            } else {
                let _ = redis.set(&count_key, &remaining_count, None).await;
            }

            if let Ok(participant_ids) = repo.get_participant_ids(broadcast_id).await {
                let payload = WsPayload::participant_left(user_clone);
                ws.send_to_users(&participant_ids, payload).await;
            }
        });

        tracing::info!("User {} left broadcast {}", user_id, broadcast_id);

        Ok(LeaveBroadcastResponse {
            success: true,
            broadcast_id,
            user_id,
            left_at: OffsetDateTime::now_utc(),
        })
    }

    pub async fn add_cohost(
        &self,
        state: &MenoState,
        broadcast_id: Uuid,
        requester_id: Uuid,
        cohost_id: Uuid,
    ) -> Result<CohostSessionResponse, BroadcastError> {
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

        let mut tx = state.db.begin().await?;

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

        let ws = self.ws.clone();
        let token = broadcast_token.clone();

        tokio::spawn(async move {
            let payload = WsPayload::cohost_invitation(broadcast_id, token);
            ws.send_to_user(cohost_id, payload).await;
        });

        Ok(CohostSessionResponse {
            user: cohost,
            token: broadcast_token,
        })
    }

    pub async fn remove_cohost(
        &self,
        state: &MenoState,
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

        let mut tx = state.db.begin().await?;

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
            self.ws.send_to_user(cohost_id, payload).await;
        }

        let ws = self.ws.clone();
        let repo = self.repo.clone();
        let cohost_clone = cohost_user.clone();

        tokio::spawn(async move {
            if let Ok(participant_ids) = repo.get_participant_ids(broadcast_id).await {
                let payload = WsPayload::removed_cohost(cohost_clone);
                ws.send_to_users(&participant_ids, payload).await;
            }
        });

        Ok(())
    }

    pub async fn get_broadcast(
        &self,
        broadcast_id: Uuid,
    ) -> Result<BroadcastResponse, BroadcastError> {
        let key = RedisKey::broadcast(broadcast_id);

        if let Ok(Some(cached_response)) = self.redis.get::<BroadcastResponse>(&key).await {
            return Ok(cached_response);
        }

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

        let broadcast_response = self
            .broadcast_to_response(broadcast, creator, cohosts, ctx)
            .await?;

        let key_clone = key.clone();
        let redis = self.redis.clone();
        let response_clone = broadcast_response.clone();

        tokio::spawn(async move {
            let _ = redis
                .set(&key_clone, &response_clone, Some(TTL_60_SECS))
                .await;
        });

        Ok(broadcast_response)
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

    /// Check if user is currently hosting any active broadcast
    /// Useful for quick checks without fetching full broadcast data
    pub async fn is_active_host(&self, user_id: Uuid) -> Result<bool, BroadcastError> {
        self.repo.is_active_host(user_id).await
    }

    // ==================== HELPERS ====================
    async fn broadcast_to_response(
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

    async fn get_participant_role(
        &self,
        broadcast: &Broadcast,
        participant_id: Uuid,
    ) -> Result<ParticipantRole, BroadcastError> {
        if broadcast.creator_id == participant_id {
            return Ok(ParticipantRole::Host);
        }

        match self
            .repo
            .find_participant(broadcast.id, participant_id)
            .await?
        {
            None => Ok(ParticipantRole::None),
            Some(p) => Ok(p.role),
        }
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
}
