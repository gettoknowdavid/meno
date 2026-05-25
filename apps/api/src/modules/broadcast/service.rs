use crate::modules::broadcast::dto::{
    BroadcastEndedPayload, BroadcastResponse, BroadcastSessionResponse, CreateBroadcastRequest,
    EndBroadcastResponse, MAX_COHOSTS, UserSummary,
};
use crate::modules::broadcast::errors::BroadcastError;
use crate::modules::broadcast::model::{
    Broadcast, BroadcastContext, BroadcastParticipant, BroadcastState, BroadcastStatus, EndReason,
    ParticipantRole,
};
use crate::modules::broadcast::repository::{
    BroadcastRepository, CreateBroadcastInput, SetActiveInput, UpsertParticipantInput,
};
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
        if let Some(st) = req.start_time {
            if st <= OffsetDateTime::now_utc() {
                return Err(BroadcastError::StartTimeInPast);
            }
        }

        let cohost_ids = req.cohosts.clone().unwrap_or_default();
        let cohosts = if cohost_ids.is_empty() {
            vec![]
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

            users
        };

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

    pub async fn go_live(
        &self,
        state: &MenoState,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<BroadcastSessionResponse, BroadcastError> {
        // Get the broadcast details, if not found through the appropriate error
        let broadcast = self
            .repo
            .find_by_id(broadcast_id)
            .await?
            .ok_or(BroadcastError::NotFound)?;

        // Confirm that the person trying to start the current broadcast is the creator
        if broadcast.creator_id != user_id {
            return Err(BroadcastError::NotCreator);
        }

        // Confirm that the current broadcast is not already live
        if broadcast.status == BroadcastStatus::Active {
            return Err(BroadcastError::AlreadyLive);
        }

        // Retrieve the creator details from the DB
        let creator = self
            .repo
            .find_user_summary(user_id)
            .await?
            .ok_or(BroadcastError::UserNotFound)?;

        // Create a LiveKit room, using the `broadcast_id` as the main identifier
        self.livekit
            .create_room(broadcast_id)
            .await
            .map_err(BroadcastError::LiveKit)?;

        // Generate a `broadcast_token` for LiveKit. This will be sent in the response to the FE
        // to give access to the LiveKit room
        let broadcast_token = self
            .livekit
            .create_token(
                creator.id,
                &creator.full_name,
                broadcast_id,
                LivekitRole::Host,
            )
            .await?;

        let now = OffsetDateTime::now_utc();
        let mut tx = state.db.begin().await?;

        // Set the broadcast status to `active`
        let set_active_input = SetActiveInput {
            broadcast_id,
            broadcast_token: broadcast_token.clone(),
        };
        let broadcast = self.repo.set_active(&set_active_input, &mut tx).await?;

        // Add the creator to the broadcast participants table, so others can see the creator in
        // the list of participants
        let part_input = UpsertParticipantInput {
            broadcast_id,
            participant_id: creator.id,
            role: ParticipantRole::Host,
            joined_at: now,
        };
        self.repo.upsert_participant(&part_input, &mut tx).await?;

        // Commit everything using a transaction
        tx.commit().await?;

        // Set a live count key Redis to 1
        // This is the number of currently live participants, and the creator is the first
        // in the list
        let count_key = RedisKey::live_count(broadcast_id);
        let _ = self.redis.set(&count_key, &1_i64, None).await;

        // Inner block for background tasks
        {
            let svc = self.clone();
            let ws = self.ws.clone();
            let broadcast_clone = broadcast.clone();
            let creator_clone = creator.clone();

            // Create a background task to:
            // - get the list of users subscribed to the creator of this broadcast
            // - send in-app notifications for each subscriber
            // - generate the broadcast response data
            // - emit a `newBroadcast` event to all the users subscribed to the creator
            tokio::spawn(async move {
                if let Ok(subscriber_ids) = svc
                    .repo
                    .get_subscriber_ids(broadcast_clone.creator_id)
                    .await
                {
                    // TODO: create in-app notifications for each subscriber

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
                        ws.send_to_users(&subscriber_ids, payload).await;
                    }
                }
            });
        }

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

    pub async fn end_broadcast(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<EndBroadcastResponse, BroadcastError> {
        let broadcast = self.repo.find_by_id_or_error(broadcast_id).await?;
        if broadcast.creator_id != user_id {
            return Err(BroadcastError::CannotEnd);
        }
        if broadcast.status != BroadcastStatus::Active {
            return Err(BroadcastError::NotLive);
        }

        let participant_ids = self
            .repo
            .get_participant_ids_and_clear(broadcast_id)
            .await?;

        self.repo
            .set_inactive(broadcast_id, &EndReason::Normal)
            .await?;

        if let Err(e) = self.livekit.delete_room(broadcast_id).await {
            tracing::warn!(broadcast_id = %broadcast_id, error = %e, "LiveKit room deletion failed");
        }

        let count_key = RedisKey::live_count(broadcast_id);
        let _ = self.redis.del(&count_key).await;

        let cohosts_and_participants: Vec<Uuid> = participant_ids
            .iter()
            .copied()
            .filter(|&i| i != broadcast.creator_id)
            .collect();

        if !cohosts_and_participants.is_empty() {
            let data = WsPayload {
                event: WsEvent::EndedBroadcast,
                data: to_value(BroadcastEndedPayload::normal_for(broadcast_id)).unwrap_or_default(),
            };
            self.ws.send_to_users(&cohosts_and_participants, data).await;
        }

        let recording_ready = if broadcast.recording_enabled {
            let key = RedisKey::recording_ready(broadcast_id);
            self.redis.exists(&key).await.unwrap_or(false)
        } else {
            false
        };

        let ended_at = OffsetDateTime::now_utc();
        let duration_secs = broadcast
            .start_time
            .map(|s| (ended_at - s).whole_seconds())
            .unwrap_or(0);

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
    async fn build_context(
        &self,
        broadcast: &Broadcast,
        participant_id: Option<Uuid>,
        live_count: i64,
        total_count: i64,
    ) -> Result<BroadcastContext, BroadcastError> {
        let grace_key = RedisKey::host_grace(broadcast.id);
        let is_reconnecting = self.redis.exists(&grace_key).await.unwrap_or(false);

        let (participant_role, is_subscribed, is_bookmarked) = match participant_id {
            None => (ParticipantRole::None, false, false),
            Some(pid) => {
                let (role_res, is_sub_res, is_bm_res) = tokio::join!(
                    self.get_participant_role(&broadcast, pid),
                    self.repo.is_subscribed(pid, broadcast.creator_id),
                    self.repo.is_bookmarked(pid, broadcast.creator_id),
                );
                (role_res?, is_sub_res?, is_bm_res?)
            }
        };

        Ok(BroadcastContext {
            participant_id,
            is_reconnecting,
            live_count,
            total_count,
            participant_role,
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
}
