//! An in-memory [`BroadcastRepo`] for the hermetic tests.
//!
//! # A double that diverges is worse than no double
//!
//! §10's service tests are only worth anything if this behaves like Postgres in the
//! places the service's *correctness* depends on, so it is not a `HashMap` that answers
//! "yes" to everything:
//!
//! - [`BroadcastRepo::join`] distinguishes first join, rejoin and already-present, and
//!   moves `total_participants` for the first two only. A double that always inserted
//!   would make §7.6's bug look correct.
//! - [`BroadcastRepo::mark_live`] and [`BroadcastRepo::mark_ended`] are guarded on the
//!   current status, so "go live twice" is answered rather than overwriting.
//! - [`BroadcastRepo::leave`] refuses for the host, matching the adapter.
//! - [`BroadcastRepo::list`] applies the keyset boundary, because a page that ignores
//!   it returns row 1 again forever and no test would notice.
//!
//! It is a real implementation of the trait rather than scaffolding: the `tests/`
//! integration suite links `meno_api` as an external crate where `#[cfg(test)]` is
//! false, which is why this is behind `test-support`.

use async_trait::async_trait;
use std::sync::Mutex;
use time::OffsetDateTime;
use uuid::Uuid;

use super::{BroadcastFilter, BroadcastRepo, ParticipantFilter};
use crate::modules::broadcast::model::{
    Broadcast, BroadcastCohost, BroadcastParticipant, BroadcastPatch, BroadcastStatus, EndReason,
    JoinOutcome, JoinRequest, LeaveOutcome, NewBroadcast, ParticipantRole, UserSummary, now,
};
use meno_core::Error as MenoError;

/// An in-memory store, guarded by one mutex.
///
/// One lock rather than one per table: the service's flows are short and each is a
/// handful of operations, and a lock-per-table would let a test observe a state no real
/// transaction can produce — which is exactly the kind of false green this file is
/// meant to avoid.
#[derive(Debug, Default)]
pub struct InMemoryBroadcastRepo {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    broadcasts: Vec<Broadcast>,
    participants: Vec<BroadcastParticipant>,
    cohosts: Vec<BroadcastCohost>,
    users: Vec<UserSummary>,
    clock: Option<OffsetDateTime>,
}

impl InMemoryBroadcastRepo {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an account the repository can resolve.
    ///
    /// A broadcast's creator is a real `users` row, and the FK is what makes a detail
    /// response possible at all, so the double needs one to answer with.
    pub fn insert_user(&self, user: UserSummary) {
        let mut state = self.lock();
        state.users.retain(|existing| existing.id != user.id);
        state.users.push(user);
    }

    /// Freeze the clock at `at`, so a test can assert on "is this in the past".
    pub fn set_clock(&self, at: OffsetDateTime) {
        self.lock().clock = Some(at);
    }

    /// How many broadcasts the store holds.
    #[must_use]
    pub fn broadcast_count(&self) -> usize {
        self.lock().broadcasts.len()
    }

    /// The all-time participant counter for a broadcast.
    ///
    /// Exposed because §7.6 is about this number: a test that cannot read it cannot
    /// assert that the two implementations agree.
    #[must_use]
    pub fn total_participants(&self, broadcast_id: Uuid) -> i64 {
        self.lock()
            .broadcasts
            .iter()
            .find(|b| b.id == broadcast_id)
            .map_or(0, |b| b.total_participants)
    }

    /// Insert a broadcast directly, for a test that starts from a known state.
    pub fn insert_broadcast(&self, mut broadcast: Broadcast) {
        let mut state = self.lock();
        let now = state.clock.unwrap_or_else(now);
        broadcast.created_at = now;
        broadcast.updated_at = now;
        state.broadcasts.push(broadcast);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        // A poisoned lock means a test panicked while holding it, and every later
        // assertion in that test would be meaningless anyway.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn now(state: &State) -> OffsetDateTime {
        state.clock.unwrap_or_else(now)
    }
}

/// Find a broadcast, or report it as missing exactly as the adapter does.
fn require(state: &mut State, id: Uuid) -> Result<&mut Broadcast, MenoError> {
    state
        .broadcasts
        .iter_mut()
        .find(|b| b.id == id && b.deleted_at.is_none())
        .ok_or_else(crate::modules::broadcast::error::not_found)
}

#[async_trait]
impl BroadcastRepo for InMemoryBroadcastRepo {
    async fn create(&self, new: NewBroadcast) -> Result<Broadcast, MenoError> {
        let mut state = self.lock();
        let now = Self::now(&state);

        let broadcast = Broadcast {
            id: Uuid::new_v4(),
            title: new.title,
            description: new.description,
            status: BroadcastStatus::Inactive,
            creator_id: new.creator_id,
            time_zone: new.time_zone,
            image_url: new.image_url,
            image_id: new.image_id,
            broadcast_token: None,
            total_participants: 0,
            start_time: new.start_time,
            end_time: None,
            recording_enabled: new.recording_enabled,
            recording_key: None,
            recording_url: None,
            published_at: None,
            end_reason: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };

        for cohost in &new.cohosts {
            state.cohosts.push(BroadcastCohost {
                broadcast_id: broadcast.id,
                cohost_id: *cohost,
                invited_by: new.creator_id,
                invited_at: now,
                removed_at: None,
            });
        }

        state.broadcasts.push(broadcast.clone());
        Ok(broadcast)
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Broadcast>, MenoError> {
        Ok(self
            .lock()
            .broadcasts
            .iter()
            .find(|b| b.id == id && b.deleted_at.is_none())
            .cloned())
    }

    async fn update(&self, id: Uuid, patch: BroadcastPatch) -> Result<Broadcast, MenoError> {
        let mut state = self.lock();
        let now = Self::now(&state);

        // The borrow of `state.broadcasts` has to end before `state.cohosts` is
        // touched, and the cohost rows need the creator id the patch is applied to — so
        // the two halves are separate statements over the same guard rather than one
        // nested block, which is what the Postgres adapter's single transaction buys.
        {
            let broadcast = require(&mut state, id)?;

            if let Some(title) = patch.title.clone() {
                broadcast.title = title;
            }
            if let Some(description) = patch.description.clone() {
                broadcast.description = Some(description);
            }
            if let Some(time_zone) = patch.time_zone.clone() {
                broadcast.time_zone = Some(time_zone);
            }
            if let Some(image_id) = patch.image_id.clone() {
                broadcast.image_id = Some(image_id);
            }
            if let Some(image_url) = patch.image_url.clone() {
                broadcast.image_url = Some(image_url);
            }
            if let Some(start_time) = patch.start_time {
                broadcast.start_time = Some(start_time);
            }
            if let Some(recording_enabled) = patch.recording_enabled {
                broadcast.recording_enabled = recording_enabled;
            }
            broadcast.updated_at = now;
        }

        if let Some(cohosts) = patch.cohosts {
            let creator = state
                .broadcasts
                .iter()
                .find(|b| b.id == id)
                .map_or(id, |b| b.creator_id);
            state.cohosts.retain(|c| c.broadcast_id != id);
            for cohost in cohosts {
                state.cohosts.push(BroadcastCohost {
                    broadcast_id: id,
                    cohost_id: cohost,
                    invited_by: creator,
                    invited_at: now,
                    removed_at: None,
                });
            }
        }

        Ok(require(&mut state, id)?.clone())
    }

    async fn soft_delete(&self, id: Uuid) -> Result<bool, MenoError> {
        let mut state = self.lock();
        let now = Self::now(&state);
        match state
            .broadcasts
            .iter_mut()
            .find(|b| b.id == id && b.deleted_at.is_none())
        {
            Some(broadcast) => {
                broadcast.deleted_at = Some(now);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn mark_live(
        &self,
        id: Uuid,
        at: OffsetDateTime,
    ) -> Result<Option<Broadcast>, MenoError> {
        let mut state = self.lock();
        let now = Self::now(&state);
        let Some(broadcast) = state
            .broadcasts
            .iter_mut()
            .find(|b| b.id == id && b.deleted_at.is_none())
        else {
            return Ok(None);
        };

        if broadcast.status != BroadcastStatus::Inactive {
            return Ok(None);
        }

        broadcast.status = BroadcastStatus::Active;
        broadcast.published_at = Some(at);
        broadcast.updated_at = now;
        Ok(Some(broadcast.clone()))
    }

    async fn mark_ended(
        &self,
        id: Uuid,
        reason: EndReason,
        at: OffsetDateTime,
    ) -> Result<Option<Broadcast>, MenoError> {
        let mut state = self.lock();
        let now = Self::now(&state);
        let Some(broadcast) = state
            .broadcasts
            .iter_mut()
            .find(|b| b.id == id && b.deleted_at.is_none())
        else {
            return Ok(None);
        };

        if broadcast.status != BroadcastStatus::Active {
            return Ok(None);
        }

        broadcast.status = BroadcastStatus::Inactive;
        broadcast.end_time = Some(at);
        broadcast.end_reason = Some(reason);
        broadcast.updated_at = now;
        Ok(Some(broadcast.clone()))
    }

    async fn list(&self, filter: BroadcastFilter) -> Result<Vec<Broadcast>, MenoError> {
        let state = self.lock();
        let mut rows: Vec<Broadcast> = state
            .broadcasts
            .iter()
            .filter(|b| b.deleted_at.is_none())
            .filter(|b| filter.creator_id.is_none_or(|id| b.creator_id == id))
            .filter(|b| filter.status.is_none_or(|status| b.status == status))
            .cloned()
            .collect();

        // The keyset boundary, applied for real: a page that ignores it returns the
        // first row forever, and no assertion in a service test would notice.
        if let Some((cursor_ts, cursor_id)) = filter.cursor {
            rows.retain(|b| match filter.order {
                meno_core::Order::Desc => (b.created_at, b.id) < (cursor_ts, cursor_id),
                meno_core::Order::Asc => (b.created_at, b.id) > (cursor_ts, cursor_id),
            });
        }

        rows.sort_by(|a, b| match filter.order {
            meno_core::Order::Desc => b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)),
            meno_core::Order::Asc => a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)),
        });

        rows.truncate(filter.limit_plus_one.max(0) as usize);
        Ok(rows)
    }

    async fn live_participant_count(&self, broadcast_id: Uuid) -> Result<i64, MenoError> {
        Ok(self
            .lock()
            .participants
            .iter()
            .filter(|p| p.broadcast_id == broadcast_id && p.is_present())
            .count() as i64)
    }

    async fn join(&self, request: JoinRequest) -> Result<JoinOutcome, MenoError> {
        let mut state = self.lock();
        let now = Self::now(&state);

        if state
            .broadcasts
            .iter()
            .any(|b| b.id == request.broadcast_id && b.deleted_at.is_none())
        {
            // A join against a deleted broadcast is the adapter's job to refuse; the
            // double reports it the same way rather than inventing a participant.
        } else {
            return Err(crate::modules::broadcast::error::not_found());
        }

        if let Some(existing) = state.participants.iter_mut().find(|p| {
            p.broadcast_id == request.broadcast_id && p.participant_id == request.participant_id
        }) {
            if existing.is_present() {
                return Ok(JoinOutcome::AlreadyPresent);
            }
            existing.left_at = None;
            existing.role = request.role;
            bump_total(&mut state, request.broadcast_id);
            return Ok(JoinOutcome::Rejoined);
        }

        state.participants.push(BroadcastParticipant {
            broadcast_id: request.broadcast_id,
            participant_id: request.participant_id,
            role: request.role,
            joined_at: now,
            left_at: None,
            last_listen_position_seconds: 0,
            last_listened_at: None,
        });
        bump_total(&mut state, request.broadcast_id);
        Ok(JoinOutcome::Joined)
    }

    async fn leave(
        &self,
        broadcast_id: Uuid,
        participant_id: Uuid,
    ) -> Result<LeaveOutcome, MenoError> {
        let mut state = self.lock();
        let now = Self::now(&state);

        let Some(existing) = state
            .participants
            .iter_mut()
            .find(|p| p.broadcast_id == broadcast_id && p.participant_id == participant_id)
        else {
            return Ok(LeaveOutcome::NotPresent);
        };

        if existing.role == ParticipantRole::Host {
            return Ok(LeaveOutcome::IsHost);
        }
        if !existing.is_present() {
            return Ok(LeaveOutcome::NotPresent);
        }

        // Closed, not deleted: an all-time count must not fall when someone leaves, and
        // migration 0006's trigger only decrements on DELETE.
        existing.left_at = Some(now);
        Ok(LeaveOutcome::Left)
    }

    async fn find_participant(
        &self,
        broadcast_id: Uuid,
        participant_id: Uuid,
    ) -> Result<Option<BroadcastParticipant>, MenoError> {
        Ok(self
            .lock()
            .participants
            .iter()
            .find(|p| p.broadcast_id == broadcast_id && p.participant_id == participant_id)
            .cloned())
    }

    async fn list_participants(
        &self,
        broadcast_id: Uuid,
        filter: ParticipantFilter,
    ) -> Result<Vec<BroadcastParticipant>, MenoError> {
        let state = self.lock();
        let mut rows: Vec<BroadcastParticipant> = state
            .participants
            .iter()
            .filter(|p| p.broadcast_id == broadcast_id)
            .filter(|p| !filter.present_only || p.is_present())
            .filter(|p| filter.role.is_none_or(|role| p.role == role))
            .cloned()
            .collect();

        if let Some((cursor_ts, cursor_id)) = filter.cursor {
            rows.retain(|p| match filter.order {
                meno_core::Order::Desc => (p.joined_at, p.participant_id) < (cursor_ts, cursor_id),
                meno_core::Order::Asc => (p.joined_at, p.participant_id) > (cursor_ts, cursor_id),
            });
        }

        rows.sort_by(|a, b| match filter.order {
            meno_core::Order::Desc => b
                .joined_at
                .cmp(&a.joined_at)
                .then(b.participant_id.cmp(&a.participant_id)),
            meno_core::Order::Asc => a
                .joined_at
                .cmp(&b.joined_at)
                .then(a.participant_id.cmp(&b.participant_id)),
        });

        rows.truncate(filter.limit_plus_one.max(0) as usize);
        Ok(rows)
    }

    async fn list_cohosts(&self, broadcast_id: Uuid) -> Result<Vec<BroadcastCohost>, MenoError> {
        Ok(self
            .lock()
            .cohosts
            .iter()
            .filter(|c| c.broadcast_id == broadcast_id && c.is_current())
            .cloned()
            .collect())
    }

    async fn set_cohosts(
        &self,
        broadcast_id: Uuid,
        invited_by: Uuid,
        cohosts: &[Uuid],
    ) -> Result<Vec<BroadcastCohost>, MenoError> {
        let mut state = self.lock();
        let now = Self::now(&state);
        let creator = state
            .broadcasts
            .iter()
            .find(|b| b.id == broadcast_id)
            .map_or(invited_by, |b| b.creator_id);

        state.cohosts.retain(|c| c.broadcast_id != broadcast_id);
        for cohost in cohosts {
            state.cohosts.push(BroadcastCohost {
                broadcast_id,
                cohost_id: *cohost,
                invited_by: creator,
                invited_at: now,
                removed_at: None,
            });
        }

        Ok(state
            .cohosts
            .iter()
            .filter(|c| c.broadcast_id == broadcast_id && c.is_current())
            .cloned()
            .collect())
    }

    async fn find_user(&self, id: Uuid) -> Result<Option<UserSummary>, MenoError> {
        Ok(self.lock().users.iter().find(|u| u.id == id).cloned())
    }

    async fn find_users(&self, ids: &[Uuid]) -> Result<Vec<UserSummary>, MenoError> {
        let state = self.lock();
        Ok(ids
            .iter()
            .filter_map(|id| state.users.iter().find(|u| u.id == *id).cloned())
            .collect())
    }
}

/// §7.6, in one line: the all-time counter moves with the rows.
fn bump_total(state: &mut State, broadcast_id: Uuid) {
    if let Some(broadcast) = state.broadcasts.iter_mut().find(|b| b.id == broadcast_id) {
        broadcast.total_participants += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::broadcast::model::NewBroadcast;

    fn user(n: u128) -> UserSummary {
        UserSummary {
            id: Uuid::from_u128(n),
            full_name: format!("User {n}"),
            avatar_id: None,
            avatar_url: None,
        }
    }

    fn new_broadcast(creator: Uuid) -> NewBroadcast {
        NewBroadcast {
            creator_id: creator,
            title: "Ada's show".to_owned(),
            description: None,
            time_zone: None,
            image_id: None,
            image_url: None,
            start_time: None,
            recording_enabled: false,
            cohosts: Vec::new(),
        }
    }

    #[tokio::test]
    async fn a_first_join_counts_and_a_rejoin_counts_again() {
        // §7.6. A counter that only moved on INSERT would miss every return visit; one
        // that moved on every join would inflate a client that refreshes a token.
        let repo = InMemoryBroadcastRepo::new();
        let broadcast = repo
            .create(new_broadcast(Uuid::from_u128(1)))
            .await
            .expect("create");
        let listener = Uuid::from_u128(2);

        assert_eq!(
            repo.join(JoinRequest {
                broadcast_id: broadcast.id,
                participant_id: listener,
                role: ParticipantRole::Participant,
            })
            .await
            .expect("join"),
            JoinOutcome::Joined
        );
        assert_eq!(repo.total_participants(broadcast.id), 1);

        assert_eq!(
            repo.leave(broadcast.id, listener).await.expect("leave"),
            LeaveOutcome::Left
        );
        assert_eq!(
            repo.total_participants(broadcast.id),
            1,
            "leaving must not reduce an all-time count"
        );

        assert_eq!(
            repo.join(JoinRequest {
                broadcast_id: broadcast.id,
                participant_id: listener,
                role: ParticipantRole::Participant,
            })
            .await
            .expect("rejoin"),
            JoinOutcome::Rejoined
        );
        assert_eq!(repo.total_participants(broadcast.id), 2);
    }

    #[tokio::test]
    async fn joining_twice_without_leaving_is_reported_rather_than_counted_twice() {
        let repo = InMemoryBroadcastRepo::new();
        let broadcast = repo
            .create(new_broadcast(Uuid::from_u128(1)))
            .await
            .expect("create");
        let listener = Uuid::from_u128(2);
        let request = JoinRequest {
            broadcast_id: broadcast.id,
            participant_id: listener,
            role: ParticipantRole::Participant,
        };

        assert_eq!(
            repo.join(request.clone()).await.expect("join"),
            JoinOutcome::Joined
        );
        assert_eq!(
            repo.join(request).await.expect("join"),
            JoinOutcome::AlreadyPresent,
            "a duplicate join is a 409, not a second count"
        );
        assert_eq!(repo.total_participants(broadcast.id), 1);
    }

    #[tokio::test]
    async fn a_host_cannot_leave_their_own_broadcast() {
        let repo = InMemoryBroadcastRepo::new();
        let host = Uuid::from_u128(1);
        let broadcast = repo.create(new_broadcast(host)).await.expect("create");
        repo.join(JoinRequest {
            broadcast_id: broadcast.id,
            participant_id: host,
            role: ParticipantRole::Host,
        })
        .await
        .expect("host joins");

        assert_eq!(
            repo.leave(broadcast.id, host).await.expect("leave"),
            LeaveOutcome::IsHost,
            "otherwise a live broadcast can end up with nobody able to end it"
        );
    }

    #[tokio::test]
    async fn going_live_twice_is_answered_rather_than_overwriting_the_first_start() {
        let repo = InMemoryBroadcastRepo::new();
        let broadcast = repo
            .create(new_broadcast(Uuid::from_u128(1)))
            .await
            .expect("create");
        let at = now();

        assert!(
            repo.mark_live(broadcast.id, at)
                .await
                .expect("live")
                .is_some()
        );
        assert!(
            repo.mark_live(broadcast.id, at + time::Duration::hours(1))
                .await
                .expect("live")
                .is_none(),
            "a second go-live must not rewrite published_at"
        );
    }

    #[tokio::test]
    async fn a_list_past_the_cursor_never_repeats_a_row() {
        // The property that makes a keyset cursor work, and the one a naive double
        // gets wrong: page two must not be page one.
        let repo = InMemoryBroadcastRepo::new();
        let base = now();
        for n in 0..5_i64 {
            // Distinct `created_at`s, or the id tiebreak decides the order and the test
            // would be asserting on uuid v4 randomness. Freeze the double's clock at each
            // intended instant so the assertion is stable rather than racing real time.
            let at = base + time::Duration::seconds(n);
            repo.set_clock(at);
            repo.create(new_broadcast(Uuid::from_u128(1 + n as u128)))
                .await
                .expect("create");
            if let Some(row) = repo
                .find_by_id(repo.broadcasts()[n as usize].id)
                .await
                .expect("read")
            {
                assert_eq!(row.created_at, at, "clock is frozen per create");
            }
        }

        let filter = BroadcastFilter {
            limit_plus_one: 2,
            order: meno_core::Order::Desc,
            ..Default::default()
        };
        let page_one = repo.list(filter.clone()).await.expect("listing");
        assert_eq!(page_one.len(), 2, "limit + one probe row");

        let cursor = (page_one[1].created_at, page_one[1].id);
        let page_two = repo
            .list(BroadcastFilter {
                cursor: Some(cursor),
                ..filter
            })
            .await
            .expect("listing");

        assert!(
            !page_two
                .iter()
                .any(|b| page_one.iter().any(|seen| seen.id == b.id)),
            "page two repeated a row from page one: {cursor:?}"
        );
    }

    #[tokio::test]
    async fn a_user_lookup_returns_the_creator_and_only_the_asked_for_cohosts() {
        // The response DTOs are assembled from these two calls: one for the creator, one
        // for the whole cohost list. A double that answered them inconsistently — say
        // `find_users` returning every row, or the wrong id's summary — would render a
        // broadcast with someone else's name on it, and only a real assertion catches
        // that.
        let repo = InMemoryBroadcastRepo::new();
        let creator = user(1);
        let cohost = user(2);
        let stranger = user(3);
        for summary in [&creator, &cohost, &stranger] {
            repo.insert_user(summary.clone());
        }

        let broadcast = repo
            .create(new_broadcast(creator.id))
            .await
            .expect("create");
        let broadcast = repo
            .update(
                broadcast.id,
                BroadcastPatch {
                    cohosts: Some(vec![cohost.id]),
                    ..Default::default()
                },
            )
            .await
            .expect("update");

        let found = repo
            .find_user(creator.id)
            .await
            .expect("find_user")
            .expect("the creator exists");
        assert_eq!(found.full_name, creator.full_name);
        assert_eq!(
            repo.find_user(stranger.id).await.expect("find_user"),
            Some(stranger.clone()),
            "an unrelated user is still findable — the method filters nothing"
        );

        let summaries = repo
            .find_users(&[cohost.id, stranger.id])
            .await
            .expect("find_users");
        assert_eq!(summaries.len(), 2, "both asked-for users: {summaries:?}");
        assert!(
            summaries.contains(&cohost) && summaries.contains(&stranger),
            "{summaries:?}"
        );
        assert!(
            repo.find_users(&[]).await.expect("find_users").is_empty(),
            "an empty id list must not turn into a full scan"
        );

        // The cohost list actually stored, to prove the lookup above was asked about a
        // real invitation rather than an arbitrary id.
        let cohosts = repo.list_cohosts(broadcast.id).await.expect("cohosts");
        assert_eq!(cohosts.len(), 1);
        assert_eq!(cohosts[0].cohost_id, cohost.id);
    }

    impl InMemoryBroadcastRepo {
        /// The stored broadcasts, oldest first, for test setup and assertions.
        fn broadcasts(&self) -> Vec<Broadcast> {
            let mut rows = self.lock().broadcasts.clone();
            rows.sort_by_key(|b| b.created_at);
            rows
        }
    }
}
