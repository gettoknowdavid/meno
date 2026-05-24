use crate::modules::broadcast::dto::{
    BroadcastResponse, CreateBroadcastRequest, MAX_COHOSTS, UserSummary,
};
use crate::modules::broadcast::errors::BroadcastError;
use crate::modules::broadcast::model::{
    Broadcast, BroadcastContext, BroadcastState, BroadcastStatus, ParticipantRole,
};
use crate::modules::broadcast::repository::{BroadcastRepository, CreateBroadcastInput};
use crate::shared::services::livekit::service::LivekitService;
use crate::shared::services::redis::RedisService;
use crate::shared::services::ws::service::WsService;
use crate::state::MenoState;
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
        let grace_key = RedisService::host_grace_key(broadcast.id);
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
