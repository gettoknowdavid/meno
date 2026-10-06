//! In what order a broadcast's rules apply (plan §4.1: `service` answers "in what
//! order things happen").
//!
//! # The shape of every flow here
//!
//! Validate → authorise → act → report. Validation is [`super::dto`]'s job and has
//! already happened at the handler; this layer authorises (is the caller the creator?
//! is it live?), performs the write *through* the media server, and renders.
//!
//! # Three rules that are enforced here and nowhere else
//!
//! - **Only the creator starts, ends, edits or deletes a broadcast.** A cohost may
//!   publish and moderate; they may not end the broadcast, because ending it is the
//!   creator's decision about their own channel.
//! - **A live broadcast is immutable.** A title changed under a listener is a lie they
//!   saw, and a schedule changed under a running room is meaningless.
//! - **The creator cannot join.** They are the host; joining would replace their host
//!   grant with a participant one and hand them a token that cannot administer the room
//!   they own.
//!
//! # Ordering of the media call
//!
//! The room is created *before* the row is marked live, and the token is minted *after*.
//! The reverse would leave a live broadcast whose room does not exist, and the failure
//! would appear to a listener as a silent room rather than as a 503.

use std::sync::Arc;

use meno_core::{Cursor, CursorPage, Error as MenoError};
use time::OffsetDateTime;
use uuid::Uuid;

use super::error;
use super::media::BroadcastMedia;
use super::model::{
    Broadcast, BroadcastPatch, EndReason, JoinOutcome, JoinRequest, LeaveOutcome, NewBroadcast,
    ParticipantRole, UserSummary, now,
};
use super::repository::{BroadcastFilter, BroadcastRepo, ParticipantFilter};
use crate::modules::broadcast::dto::{
    AddCohostRequest, BroadcastListItem, BroadcastQuery, BroadcastResponse,
    BroadcastSessionResponse, CohostResponse, CreateBroadcastRequest, EndBroadcastResponse,
    LeaveBroadcastResponse, ParticipantListItem, ParticipantQuery, UpdateBroadcastRequest, Viewer,
    cohost_response, user_summary,
};
use crate::modules::broadcast::model::{BroadcastCohost, BroadcastParticipant};

/// Everything [`BroadcastService::new`] needs.
#[derive(Clone)]
pub struct ServiceDeps {
    /// Storage.
    pub repo: Arc<dyn BroadcastRepo>,
    /// The media server seam (§5.5).
    pub media: Arc<dyn BroadcastMedia>,
}

impl std::fmt::Debug for ServiceDeps {
    /// Both fields are trait objects whose `Debug` would name an adapter with a
    /// credential in it, and a `{:?}` in a log line is not worth that.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceDeps").finish_non_exhaustive()
    }
}

/// The broadcast module's rules.
#[derive(Clone)]
pub struct BroadcastService {
    repo: Arc<dyn BroadcastRepo>,
    media: Arc<dyn BroadcastMedia>,
}

impl std::fmt::Debug for BroadcastService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BroadcastService")
            .field("repo", &self.repo)
            .field("media", &self.media)
            .finish()
    }
}

impl BroadcastService {
    /// Wire the service to its collaborators.
    #[must_use]
    pub fn new(deps: ServiceDeps) -> Self {
        Self {
            repo: deps.repo,
            media: deps.media,
        }
    }

    /// `POST /broadcasts` — create a draft or a scheduled broadcast.
    ///
    /// # Errors
    ///
    /// - [`super::error::invalid_time_zone`] when `time_zone` is not an IANA
    ///   identifier.
    /// - [`error::start_time_in_past`] when `start_time` has passed — re-checked here
    ///   even though `dto::validate` refused it, because a request validated at `t` and
    ///   handled at `t + 1s` has moved.
    /// - [`error::user_not_found`] when a cohost id has no live account.
    #[tracing::instrument(skip_all, fields(creator_id = %creator.id))]
    pub async fn create(
        &self,
        request: CreateBroadcastRequest,
        creator: UserSummary,
    ) -> Result<BroadcastResponse, MenoError> {
        if let Some(zone) = request.time_zone.as_deref() {
            check_time_zone(zone)?;
        }
        if request.start_time.is_some_and(|start| start <= now()) {
            return Err(error::start_time_in_past());
        }

        let cohosts = self.resolve_cohosts(&creator.id, request.cohosts).await?;

        let row = self
            .repo
            .create(NewBroadcast {
                creator_id: creator.id,
                title: request.title,
                description: request.description,
                time_zone: request.time_zone,
                image_id: request.image_id,
                image_url: request.image_url,
                start_time: request.start_time,
                recording_enabled: request.recording_enabled.unwrap_or(false),
                cohosts,
            })
            .await?;

        self.render(row, Some(creator), None).await
    }

    /// `GET /broadcasts/{id}` — one broadcast, with the caller's view of it.
    ///
    /// # Errors
    ///
    /// [`error::not_found`] when no live broadcast carries that id, including one that
    /// was soft-deleted — a deleted broadcast and a broadcast that never existed answer
    /// identically so the endpoint cannot be used to discover deleted ids.
    #[tracing::instrument(skip_all, fields(broadcast_id = %id))]
    pub async fn get(
        &self,
        id: Uuid,
        viewer: Option<&UserSummary>,
    ) -> Result<BroadcastResponse, MenoError> {
        let row = self.require(id).await?;
        let viewer = match viewer {
            Some(viewer) => Some(self.viewer_for(&row, viewer).await?),
            None => None,
        };

        self.render(row, None, viewer).await
    }

    /// `GET /broadcasts` — a keyset-paginated feed.
    ///
    /// # Errors
    ///
    /// [`MenoError::BadRequest`] with [`meno_core::ErrorCode::InvalidCursor`] when the
    /// `?cursor=` cannot be decoded. The decoding happens here rather than in a handler
    /// so the SQL below cannot be built from an opaque string by any other caller.
    #[tracing::instrument(skip_all)]
    pub async fn list(
        &self,
        query: &BroadcastQuery,
    ) -> Result<CursorPage<BroadcastListItem>, MenoError> {
        let cursor = decode_cursor(query.cursor())?;
        let rows = self
            .repo
            .list(BroadcastFilter {
                creator_id: query.creator_id,
                status: query.status,
                limit_plus_one: query.limit_plus_one(),
                cursor,
                order: query.effective_order(),
            })
            .await?;

        // One query for every creator in the page, not one per row (§9.4).
        let creators = self
            .repo
            .find_users(&rows.iter().map(|row| row.creator_id).collect::<Vec<_>>())
            .await?;
        let by_id: std::collections::HashMap<Uuid, UserSummary> =
            creators.into_iter().map(|user| (user.id, user)).collect();

        Ok(CursorPage::from_rows(
            rows.iter()
                .map(|row| BroadcastListItem {
                    id: row.id,
                    title: row.title.clone(),
                    description: row.description.clone(),
                    time_zone: row.time_zone.clone(),
                    image_url: row.image_url.clone(),
                    image_id: row.image_id.clone(),
                    status: row.status,
                    state: row.state(),
                    created_at: row.created_at,
                    start_time: row.start_time,
                    end_time: row.end_time,
                    total_participants: row.total_participants,
                    creator_id: row.creator_id,
                    creator_name: by_id
                        .get(&row.creator_id)
                        .map_or("Unknown".to_owned(), |user| user.full_name.clone()),
                    creator_avatar_id: by_id
                        .get(&row.creator_id)
                        .and_then(|user| user.avatar_id.clone()),
                    creator_avatar_url: by_id
                        .get(&row.creator_id)
                        .and_then(|user| user.avatar_url.clone()),
                })
                .collect::<Vec<_>>(),
            query.limit(),
            |item| Cursor::from_timestamp_id(item.created_at, item.id),
        ))
    }

    /// `PATCH /broadcasts/{id}` — edit, as the creator.
    ///
    /// # Errors
    ///
    /// [`error::not_creator`], [`error::broadcast_is_live`] when it is live,
    /// [`error::invalid_time_zone`], [`error::start_time_in_past`], or
    /// [`error::cohost_limit_reached`] — the limit is a service rule because the
    /// database has no constraint for it.
    #[tracing::instrument(skip_all, fields(broadcast_id = %id, actor = %actor.id))]
    pub async fn update(
        &self,
        id: Uuid,
        request: UpdateBroadcastRequest,
        actor: &UserSummary,
    ) -> Result<BroadcastResponse, MenoError> {
        let existing = self.require(id).await?;
        require_creator(&existing, actor)?;
        if existing.is_live() {
            return Err(error::broadcast_is_live());
        }

        if let Some(zone) = request.time_zone.as_deref() {
            check_time_zone(zone)?;
        }
        if request.start_time.is_some_and(|start| start <= now()) {
            return Err(error::start_time_in_past());
        }

        let cohosts = match &request.cohosts {
            Some(list) => Some(self.resolve_cohosts(&actor.id, Some(list.clone())).await?),
            None => None,
        };

        let row = self
            .repo
            .update(
                id,
                BroadcastPatch {
                    title: request.title,
                    description: request.description,
                    time_zone: request.time_zone,
                    image_id: request.image_id,
                    image_url: request.image_url,
                    start_time: request.start_time,
                    recording_enabled: request.recording_enabled,
                    cohosts,
                },
            )
            .await?;

        let creator = self.creator_of(&row).await?;
        let viewer = self.viewer_for(&row, actor).await?;
        self.render(row, Some(creator), Some(viewer)).await
    }

    /// `DELETE /broadcasts/{id}` — soft-delete, as the creator.
    ///
    /// # Errors
    ///
    /// [`error::not_creator`], or [`error::broadcast_is_live`] — deleting a live
    /// broadcast would strand every participant in a room nobody owns.
    #[tracing::instrument(skip_all, fields(broadcast_id = %id, actor = %actor.id))]
    pub async fn delete(&self, id: Uuid, actor: &UserSummary) -> Result<(), MenoError> {
        let existing = self.require(id).await?;
        require_creator(&existing, actor)?;
        if existing.is_live() {
            return Err(error::broadcast_is_live());
        }

        self.repo.soft_delete(id).await?;
        Ok(())
    }

    /// `POST /broadcasts/{id}/live` — go live, as the creator.
    ///
    /// # Errors
    ///
    /// [`error::not_creator`], [`error::already_live`], [`error::media_unavailable`]
    /// when the room cannot be created, or [`error::internal`].
    #[tracing::instrument(skip_all, fields(broadcast_id = %id, actor = %actor.id))]
    pub async fn go_live(
        &self,
        id: Uuid,
        actor: &UserSummary,
    ) -> Result<BroadcastSessionResponse, MenoError> {
        let existing = self.require(id).await?;
        require_creator(&existing, actor)?;
        if existing.is_live() {
            return Err(error::already_live());
        }

        // Room first: a row marked live with no room behind it is a broadcast that
        // looks live to every client and is not.
        self.media.create_room(id).await?;

        self.repo
            .mark_live(id, now())
            .await?
            .ok_or_else(error::already_live)?;

        // The host's participant row is written by the same join path a listener uses,
        // so "who is in this room" has one answer rather than two.
        self.repo
            .join(JoinRequest {
                broadcast_id: id,
                participant_id: actor.id,
                role: ParticipantRole::Host,
            })
            .await?;

        let token = self
            .media
            .mint_join_token(actor, id, ParticipantRole::Host)
            .await?;
        let row = self.require(id).await?;
        let creator = self.creator_of(&row).await?;
        let viewer = self.viewer_for(&row, actor).await?;

        Ok(BroadcastSessionResponse {
            broadcast: self.assemble(&row, Some(creator), Some(viewer)).await?,
            token,
        })
    }

    /// `POST /broadcasts/{id}/end` — end it, as the creator.
    ///
    /// # Errors
    ///
    /// [`error::not_creator`] or [`error::not_live`]. A media failure while tearing the
    /// room down is logged and swallowed: the broadcast is already ended in the
    /// database, and refusing the response would tell the client it is still live.
    #[tracing::instrument(skip_all, fields(broadcast_id = %id, actor = %actor.id))]
    pub async fn end(
        &self,
        id: Uuid,
        actor: &UserSummary,
    ) -> Result<EndBroadcastResponse, MenoError> {
        let existing = self.require(id).await?;
        require_creator(&existing, actor)?;
        if !existing.is_live() {
            return Err(error::not_live());
        }

        let ended_at = now();
        let row = self
            .repo
            .mark_ended(id, EndReason::Normal, ended_at)
            .await?
            .ok_or_else(error::not_live)?;

        if let Err(failure) = self.media.end_room(id).await {
            tracing::warn!(%failure, broadcast_id = %id, "room teardown failed after end");
        }

        let duration_secs = row.published_at.map_or(0, |started| {
            ended_at.unix_timestamp() - started.unix_timestamp()
        });

        Ok(EndBroadcastResponse {
            broadcast_id: row.id,
            broadcast_title: row.title.clone(),
            broadcast_image_url: row.image_url.clone(),
            creator_id: row.creator_id,
            ended_reason: row.end_reason.unwrap_or(EndReason::Normal),
            ended_at,
            duration_secs,
            total_participants: row.total_participants,
            recording_enabled: row.recording_enabled,
        })
    }

    /// `POST /broadcasts/{id}/join` — join a live broadcast.
    ///
    /// # Errors
    ///
    /// [`error::not_live`], [`error::creator_cannot_join`], [`error::already_joined`],
    /// or [`error::media_unavailable`].
    #[tracing::instrument(skip_all, fields(broadcast_id = %id, user = %user.id))]
    pub async fn join(
        &self,
        id: Uuid,
        user: &UserSummary,
    ) -> Result<BroadcastSessionResponse, MenoError> {
        let row = self.require(id).await?;
        if !row.is_live() {
            return Err(error::not_live());
        }
        if row.creator_id == user.id {
            return Err(error::creator_cannot_join());
        }

        let role = self.role_for(&row, user.id).await?;

        match self
            .repo
            .join(JoinRequest {
                broadcast_id: id,
                participant_id: user.id,
                role,
            })
            .await?
        {
            JoinOutcome::Joined | JoinOutcome::Rejoined => {}
            JoinOutcome::AlreadyPresent => return Err(error::already_joined()),
        }

        let token = self.media.mint_join_token(user, id, role).await?;
        let row = self.require(id).await?;
        let creator = self.creator_of(&row).await?;
        let viewer = self.viewer_for(&row, user).await?;

        Ok(BroadcastSessionResponse {
            broadcast: self.assemble(&row, Some(creator), Some(viewer)).await?,
            token,
        })
    }

    /// `POST /broadcasts/{id}/leave`.
    ///
    /// # Errors
    ///
    /// [`error::not_joined`] or [`error::host_cannot_leave`].
    #[tracing::instrument(skip_all, fields(broadcast_id = %id, user = %user.id))]
    pub async fn leave(
        &self,
        id: Uuid,
        user: &UserSummary,
    ) -> Result<LeaveBroadcastResponse, MenoError> {
        self.require(id).await?;

        match self.repo.leave(id, user.id).await? {
            LeaveOutcome::IsHost => return Err(error::host_cannot_leave()),
            LeaveOutcome::NotPresent => return Err(error::not_joined()),
            LeaveOutcome::Left => {}
        }

        // Best effort, and deliberately not fatal: the row is closed either way, and a
        // de-participant still in the room is a media problem rather than a lost write.
        if let Err(failure) = self.media.remove_participant(id, user.id).await {
            tracing::warn!(%failure, broadcast_id = %id, "evicting a departed participant failed");
        }

        Ok(LeaveBroadcastResponse {
            success: true,
            broadcast_id: id,
            user_id: user.id,
            left_at: now(),
        })
    }

    /// `POST /broadcasts/{id}/cohosts` — add one, as the creator.
    ///
    /// # Errors
    ///
    /// [`error::not_creator`], [`error::cannot_add_self_as_cohost`],
    /// [`error::user_not_found`], [`error::already_cohost`] or
    /// [`error::cohost_limit_reached`].
    #[tracing::instrument(skip_all, fields(broadcast_id = %id, actor = %actor.id))]
    pub async fn add_cohost(
        &self,
        id: Uuid,
        request: AddCohostRequest,
        actor: &UserSummary,
    ) -> Result<CohostResponse, MenoError> {
        let row = self.require(id).await?;
        require_creator(&row, actor)?;

        if request.cohost == actor.id {
            return Err(error::cannot_add_self_as_cohost());
        }

        let existing = self.repo.list_cohosts(id).await?;
        if existing.iter().any(|c| c.cohost_id == request.cohost) {
            return Err(error::already_cohost());
        }
        if existing.len() >= super::dto::MAX_COHOSTS {
            return Err(error::cohost_limit_reached(super::dto::MAX_COHOSTS));
        }

        let mut ids: Vec<Uuid> = existing.iter().map(|c| c.cohost_id).collect();
        ids.push(request.cohost);

        let updated = self.repo.set_cohosts(id, actor.id, &ids).await?;
        let added = updated
            .iter()
            .find(|cohost| cohost.cohost_id == request.cohost)
            .cloned()
            .ok_or_else(error::user_not_found)?;

        let user = self
            .repo
            .find_user(request.cohost)
            .await?
            .ok_or_else(error::user_not_found)?;

        Ok(cohost_response(
            &added,
            user_summary(user.id, user.full_name, user.avatar_id, user.avatar_url),
            updated.len(),
        ))
    }

    /// `DELETE /broadcasts/{id}/cohosts/{cohostId}` — remove one, as the creator.
    ///
    /// # Errors
    ///
    /// [`error::not_creator`] or [`error::not_joined`] when there is no such cohost —
    /// which is the honest answer for "that user is not a cohost of this broadcast".
    #[tracing::instrument(skip_all, fields(broadcast_id = %id, actor = %actor.id))]
    pub async fn remove_cohost(
        &self,
        id: Uuid,
        cohost_id: Uuid,
        actor: &UserSummary,
        remove_from_room: bool,
    ) -> Result<(), MenoError> {
        let row = self.require(id).await?;
        require_creator(&row, actor)?;

        let existing = self.repo.list_cohosts(id).await?;
        if !existing.iter().any(|cohost| cohost.cohost_id == cohost_id) {
            return Err(error::not_joined());
        }

        let ids: Vec<Uuid> = existing
            .iter()
            .map(|c| c.cohost_id)
            .filter(|id| *id != cohost_id)
            .collect();
        self.repo.set_cohosts(id, actor.id, &ids).await?;

        // Demoted to a listener in the room, and evicted if the caller asked: a
        // de-cohosted participant who keeps publishing is the failure
        // `remove_from_room` exists to prevent, and its default is the safe branch.
        self.repo
            .join(JoinRequest {
                broadcast_id: id,
                participant_id: cohost_id,
                role: ParticipantRole::Participant,
            })
            .await?;

        if remove_from_room && let Err(failure) = self.media.remove_participant(id, cohost_id).await
        {
            tracing::warn!(%failure, broadcast_id = %id, cohost_id = %cohost_id, "evicting a removed cohost failed");
        }

        Ok(())
    }

    /// `GET /broadcasts/{id}/participants` — the roster.
    ///
    /// # Errors
    ///
    /// [`error::not_found`] for the broadcast, or an invalid cursor as 400.
    #[tracing::instrument(skip_all, fields(broadcast_id = %id))]
    pub async fn participants(
        &self,
        id: Uuid,
        query: &ParticipantQuery,
    ) -> Result<CursorPage<ParticipantListItem>, MenoError> {
        self.require(id).await?;
        let cursor = decode_cursor(query.cursor())?;

        let rows = self
            .repo
            .list_participants(
                id,
                ParticipantFilter {
                    role: query.role,
                    present_only: false,
                    limit_plus_one: query.limit_plus_one(),
                    cursor,
                    order: query.effective_order(),
                },
            )
            .await?;

        let users = self
            .repo
            .find_users(&rows.iter().map(|p| p.participant_id).collect::<Vec<_>>())
            .await?;
        let by_id: std::collections::HashMap<Uuid, UserSummary> =
            users.into_iter().map(|user| (user.id, user)).collect();

        Ok(CursorPage::from_rows(
            rows.iter()
                .map(|participant| ParticipantListItem {
                    id: participant.participant_id,
                    full_name: by_id
                        .get(&participant.participant_id)
                        .map_or("Unknown".to_owned(), |user| user.full_name.clone()),
                    avatar_id: by_id
                        .get(&participant.participant_id)
                        .and_then(|user| user.avatar_id.clone()),
                    avatar_url: by_id
                        .get(&participant.participant_id)
                        .and_then(|user| user.avatar_url.clone()),
                    role: participant.role,
                    joined_at: participant.joined_at,
                    is_joined: participant.is_present(),
                })
                .collect::<Vec<_>>(),
            query.limit(),
            |item| Cursor::from_timestamp_id(item.joined_at, item.id),
        ))
    }

    // ── internals ─────────────────────────────────────────────────────────

    /// The row, or [`error::not_found`].
    async fn require(&self, id: Uuid) -> Result<Broadcast, MenoError> {
        self.repo.find_by_id(id).await?.ok_or_else(error::not_found)
    }

    /// The creator, as a [`UserSummary`].
    ///
    /// # Errors
    ///
    /// [`error::internal`]. Not [`error::user_not_found`]: the row has a foreign key to
    /// `users`, so a missing creator means the database is inconsistent, and a 404 would
    /// blame the client for it.
    async fn creator_of(&self, row: &Broadcast) -> Result<UserSummary, MenoError> {
        self.repo.find_user(row.creator_id).await?.ok_or_else(|| {
            error::internal(
                "load_creator",
                format!("broadcast {} has no creator row", row.id),
            )
        })
    }

    /// What `user_id` may do in `row`'s room: host, cohost, participant or nothing.
    async fn role_for(&self, row: &Broadcast, user_id: Uuid) -> Result<ParticipantRole, MenoError> {
        if row.creator_id == user_id {
            return Ok(ParticipantRole::Host);
        }
        Ok(self
            .repo
            .list_cohosts(row.id)
            .await?
            .iter()
            .find(|cohost| cohost.cohost_id == user_id)
            .map_or(ParticipantRole::Participant, |_| ParticipantRole::Cohost))
    }

    /// The caller's view of a broadcast, or `None` when they are not a live account.
    ///
    /// # Errors
    ///
    /// [`error::user_not_found`] when the caller id has no live `users` row. It cannot
    /// fall back to "not joined": a deleted account would otherwise be told it is simply
    /// not in the room, which is a different problem with the same fix.
    async fn viewer_for(&self, row: &Broadcast, user: &UserSummary) -> Result<Viewer, MenoError> {
        let role = self.role_for(row, user.id).await?;
        let participant: Option<BroadcastParticipant> =
            self.repo.find_participant(row.id, user.id).await?;

        Ok(Viewer {
            role,
            is_joined: participant.as_ref().is_some_and(|p| p.is_present()),
            // Bookmarks live in `broadcast_bookmarks`, which this port does not read
            // yet; reporting `false` would be a measurement, so it is reported as the
            // absence of a fact the module does not have. See the module docs on what
            // this port leaves out.
            is_bookmarked: false,
            live_participants_count: self.repo.live_participant_count(row.id).await?,
            time_remaining_seconds: None,
        })
    }

    /// Turn a row plus the caller's facts into a response.
    async fn render(
        &self,
        row: Broadcast,
        creator: Option<UserSummary>,
        viewer: Option<Viewer>,
    ) -> Result<BroadcastResponse, MenoError> {
        self.assemble(&row, creator, viewer).await
    }

    /// The shared half of every response: relations first, then the row.
    async fn assemble(
        &self,
        row: &Broadcast,
        creator: Option<UserSummary>,
        viewer: Option<Viewer>,
    ) -> Result<BroadcastResponse, MenoError> {
        let creator = match creator {
            Some(creator) => creator,
            None => self.creator_of(row).await?,
        };

        let cohosts = self
            .repo
            .list_cohosts(row.id)
            .await?
            .iter()
            .map(|cohost: &BroadcastCohost| cohost.cohost_id)
            .collect::<Vec<_>>();
        let cohost_users = self.repo.find_users(&cohosts).await?;

        BroadcastResponse::new(
            row,
            creator,
            cohost_users
                .into_iter()
                .map(|user| user_summary(user.id, user.full_name, user.avatar_id, user.avatar_url))
                .collect(),
            viewer.as_ref(),
        )
    }

    /// Resolve a cohost list, refusing a duplicate and an unknown account.
    async fn resolve_cohosts(
        &self,
        inviter: &Uuid,
        requested: Option<Vec<Uuid>>,
    ) -> Result<Vec<Uuid>, MenoError> {
        let Some(requested) = requested else {
            return Ok(Vec::new());
        };
        if requested.len() > super::dto::MAX_COHOSTS {
            return Err(error::cohost_limit_reached(super::dto::MAX_COHOSTS));
        }

        // One query for the whole list, not one per id (§9.4).
        let found = self.repo.find_users(&requested).await?;
        if found.len() != requested.len() {
            return Err(error::user_not_found());
        }

        if requested.contains(inviter) {
            return Err(error::cannot_add_self_as_cohost());
        }

        Ok(requested)
    }
}

/// Creator-only. Deliberately separate from the "does it exist" check so a reader sees
/// both rules named rather than one combined condition.
fn require_creator(broadcast: &Broadcast, actor: &UserSummary) -> Result<(), MenoError> {
    if broadcast.creator_id == actor.id {
        Ok(())
    } else {
        Err(error::not_creator())
    }
}

/// Refuse a time zone that is not shaped like an IANA identifier.
///
/// # Why this is a shape check and not a lookup
///
/// `std` has no zone database, and the workspace deliberately does not pull in a tzdb
/// crate: a wrong zone renders a scheduled broadcast at the wrong wall-clock time, and
/// the *cheap* failure — refusing a name that is obviously not a zone — is the one that
/// catches every client bug. A syntactically valid but unknown zone is stored as sent,
/// which is recorded here as a known limitation rather than dressed up: a deployment
/// that needs real resolution wants the tzdb, and that is a dependency decision, not a
/// validation one.
fn check_time_zone(zone: &str) -> Result<(), MenoError> {
    let plausible = zone.split('/').count() == 2
        && zone
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-'));

    if plausible {
        Ok(())
    } else {
        Err(error::invalid_time_zone(zone))
    }
}

/// Decode a `?cursor=` into the keyset boundary, once, at the edge.
fn decode_cursor(cursor: Option<&Cursor>) -> Result<Option<(OffsetDateTime, Uuid)>, MenoError> {
    cursor
        .map(Cursor::to_timestamp_id)
        .transpose()
        .map_err(Into::into)
}

/// A [`UserSummary`] built from a caller the auth middleware already verified.
///
/// The one place an [`AuthUser`](crate::middleware::auth::AuthUser) becomes the
/// broadcast module's own type. Avatars are `None` because the token's claims do not
/// carry them; the avatar columns are read by the repository for the users a response
/// actually lists, and inventing an empty one here would render a blank avatar for the
/// caller on their own broadcast.
#[must_use]
pub fn summary_of(user: &crate::middleware::auth::AuthUser) -> UserSummary {
    user_summary(user.id, user.full_name.clone(), None, None)
}
