//! Ending broadcasts that nobody came back to.
//!
//! # The problem this exists for
//!
//! A live broadcast is a room on the media server plus a row in `active`. When the
//! host's phone dies, the row stays live and the room stays open: nothing in the client
//! can close it, because the client is gone. Before this module the only thing that ever
//! ended a broadcast was the host pressing End, so an abandoned one stayed on air — and
//! in the catalogue — forever.
//!
//! # The rule, in one sentence
//!
//! **A live broadcast is healthy while its host, or one of its cohosts, is connected to
//! the room; the first moment it is not, a clock starts; ten minutes later it is ended.**
//!
//! Three consequences worth naming:
//!
//! - **A cohost keeps the broadcast alive.** The host answered the decision explicitly:
//!   a room with a cohost still in it is a room somebody is running. Ending it would
//!   throw out a broadcast that is being listened to.
//! - **A listener alone does not.** The stream's reason to exist is its host; a room
//!   with only listeners is a room that stopped producing audio ten minutes ago.
//! - **Presence comes from the media server, not the roster table.** See
//!   [`super::media::BroadcastMedia::room_participants`]: `left_at` is written by a
//!   client that leaves politely, and the client this rule is about never does.
//!
//! # Why the clock is a column
//!
//! `abandoned_since` is written the first time a sweep finds the broadcast unhealthy and
//! cleared the moment it is healthy again. It lives in the database rather than in the
//! job's memory because the process running the sweep can be restarted mid-grace, and a
//! countdown that resets on deploy would give every abandoned broadcast another ten
//! minutes — the failure mode this module exists to end.
//!
//! # Running it
//!
//! [`BroadcastReaper::sweep`] is one pass and is safe to run concurrently with itself:
//! `mark_ended` only matches a row that is still `active`, so two sweeps that both
//! decide to end a broadcast produce exactly one ending and exactly one announcement.
//! [`BroadcastReaper::run`] is the loop, started once per process in
//! `bootstrap::build_context`.

use std::sync::Arc;
use std::time::Duration;

use meno_core::Error as MenoError;
use uuid::Uuid;

use super::events::BroadcastEvents;
use super::media::BroadcastMedia;
use super::model::{EndReason, now};
use super::repository::{BroadcastRepo, LiveBroadcast};

/// How long a broadcast may be abandonable before it is ended.
///
/// Ten minutes: long enough for a phone that lost signal in a tunnel to come back, short
/// enough that a broadcast whose host force-quit is not discoverable in a catalogue for
/// the rest of the day.
pub const ABANDON_GRACE: Duration = Duration::from_secs(10 * 60);

/// How often a sweep runs.
///
/// A minute, not a second: the clock is stored with minute-scale meaning, and every tick
/// costs one media-server read per live broadcast.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

/// What one sweep did.
///
/// Returned rather than only logged so a test can assert that a broadcast was *not*
/// ended, which is the half of the rule that matters most.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SweepReport {
    /// How many live broadcasts were looked at.
    pub examined: usize,
    /// How many had their abandonment clock started.
    pub marked_abandoned: usize,
    /// How many were healthy again and had their clock cleared.
    pub cleared: usize,
    /// How many were ended by this sweep.
    pub ended: usize,
    /// How many could not be judged, because the media server would not answer.
    pub skipped: usize,
}

/// Ends live broadcasts that nobody is in.
#[derive(Clone)]
pub struct BroadcastReaper {
    repo: Arc<dyn BroadcastRepo>,
    media: Arc<dyn BroadcastMedia>,
    events: Arc<dyn BroadcastEvents>,
    grace: Duration,
}

impl std::fmt::Debug for BroadcastReaper {
    /// Names the collaborators' roles, not their contents: the media adapter holds
    /// credentials.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BroadcastReaper")
            .field("grace", &self.grace)
            .finish_non_exhaustive()
    }
}

/// What a sweep decided about one broadcast, before it is counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// The host, or a cohost, is in the room.
    Healthy,
    /// It was abandonable and is not any more.
    Cleared,
    /// Its clock is running, but not yet past the grace period.
    Abandoned,
    /// It was ended by this sweep.
    Ended,
    /// Somebody else ended it between the read and the write.
    AlreadyEnded,
}

impl BroadcastReaper {
    /// A reaper with the production grace period.
    #[must_use]
    pub fn new(
        repo: Arc<dyn BroadcastRepo>,
        media: Arc<dyn BroadcastMedia>,
        events: Arc<dyn BroadcastEvents>,
    ) -> Self {
        Self::with_grace(repo, media, events, ABANDON_GRACE)
    }

    /// A reaper with a grace period the caller chose, for the tests.
    #[must_use]
    pub fn with_grace(
        repo: Arc<dyn BroadcastRepo>,
        media: Arc<dyn BroadcastMedia>,
        events: Arc<dyn BroadcastEvents>,
        grace: Duration,
    ) -> Self {
        Self {
            repo,
            media,
            events,
            grace,
        }
    }

    /// One pass over every live broadcast.
    ///
    /// A failure to list the live broadcasts is returned, because a sweep that cannot
    /// see them is not a sweep. A failure on *one* broadcast is not: it is logged and
    /// counted as [`SweepReport::skipped`], and the next tick tries again — a broadcast
    /// must not be ended because the media server hiccuped, and the rest of the
    /// broadcasts must not wait for it.
    ///
    /// # Errors
    ///
    /// [`MenoError`] from [`BroadcastRepo::list_live`].
    pub async fn sweep(&self) -> Result<SweepReport, MenoError> {
        let live = self.repo.list_live().await?;
        let mut report = SweepReport {
            examined: live.len(),
            ..SweepReport::default()
        };

        for broadcast in live {
            match self.sweep_one(&broadcast).await {
                Ok(Outcome::Healthy) => {}
                Ok(Outcome::Cleared) => report.cleared += 1,
                Ok(Outcome::Abandoned) => report.marked_abandoned += 1,
                Ok(Outcome::Ended) => report.ended += 1,
                Ok(Outcome::AlreadyEnded) => {}
                Err(error) => {
                    report.skipped += 1;
                    tracing::warn!(
                        %error,
                        broadcast_id = %broadcast.id,
                        "could not judge a live broadcast this sweep; leaving it alone"
                    );
                }
            }
        }

        if report.ended > 0 || report.cleared > 0 || report.marked_abandoned > 0 {
            tracing::info!(
                examined = report.examined,
                ended = report.ended,
                abandoned = report.marked_abandoned,
                cleared = report.cleared,
                skipped = report.skipped,
                "broadcast sweep"
            );
        }

        Ok(report)
    }

    /// The loop, until the process exits.
    ///
    /// The first tick fires immediately, which is what makes a deploy shorten an
    /// already-running grace period rather than restart it.
    pub async fn run(self, interval: Duration) {
        let mut ticker = tokio::time::interval(interval);
        // A sweep that ran long must not be followed by a burst of catch-up sweeps: the
        // next one is the one that matters, and the clock is in the database anyway.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;

            match self.sweep().await {
                Ok(report) if report.examined == 0 => {}
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "the broadcast sweep could not run"),
            }
        }
    }

    /// Applies the rule to one broadcast.
    async fn sweep_one(&self, broadcast: &LiveBroadcast) -> Result<Outcome, MenoError> {
        let present = self.media.room_participants(broadcast.id).await?;
        let cohosts = self.repo.list_cohosts(broadcast.id).await?;

        if is_healthy(broadcast.creator_id, &cohosts, &present) {
            if broadcast.abandoned_since.is_some() {
                self.repo.set_abandoned_since(broadcast.id, None).await?;
                tracing::info!(broadcast_id = %broadcast.id, "the broadcast is healthy again");
                return Ok(Outcome::Cleared);
            }
            return Ok(Outcome::Healthy);
        }

        let Some(since) = broadcast.abandoned_since else {
            self.repo
                .set_abandoned_since(broadcast.id, Some(now()))
                .await?;
            tracing::info!(
                broadcast_id = %broadcast.id,
                grace_secs = self.grace.as_secs(),
                "nobody is running this broadcast; starting the abandonment clock"
            );
            return Ok(Outcome::Abandoned);
        };

        // A clock in the future is a clock that cannot be trusted — a replica with a
        // broken wall clock wrote it. Treating it as elapsed would end a broadcast that
        // has only just been abandoned, so it is left for the next sweep to judge.
        let elapsed = now().unix_timestamp() - since.unix_timestamp();
        if elapsed < 0 || elapsed < self.grace.as_secs() as i64 {
            return Ok(Outcome::Abandoned);
        }

        self.end(broadcast, &present, &cohosts).await
    }

    /// Ends an abandoned broadcast and tells everybody who was in it.
    async fn end(
        &self,
        broadcast: &LiveBroadcast,
        present: &[Uuid],
        cohosts: &[super::model::BroadcastCohost],
    ) -> Result<Outcome, MenoError> {
        let ended_at = now();

        // The guard, not an assertion: another sweep — or the host's own End — may have
        // ended this row between the read above and this write, and the answer to that
        // is "somebody else did it", never a second ending.
        let Some(row) = self
            .repo
            .mark_ended(broadcast.id, EndReason::HostDisconnected, ended_at)
            .await?
        else {
            return Ok(Outcome::AlreadyEnded);
        };

        // Best effort: the row is already ended, and refusing to tell anybody would be
        // worse than an empty room on the media server. LiveKit tears an empty room down
        // on its own timeout anyway.
        if let Err(error) = self.media.end_room(broadcast.id).await {
            tracing::warn!(%error, broadcast_id = %broadcast.id, "could not tear down the room");
        }

        // The same ending the explicit path writes: a roster left open makes the next
        // live count wrong on a broadcast nobody can be in any more.
        if let Err(error) = self.repo.close_participants(broadcast.id, ended_at).await {
            tracing::error!(%error, broadcast_id = %broadcast.id, "could not close participant rows");
        }

        let recipients = recipients_of(broadcast.creator_id, cohosts, present);
        tracing::info!(
            broadcast_id = %broadcast.id,
            recipients = recipients.len(),
            "ending an abandoned broadcast"
        );
        self.events
            .ended(row.id, EndReason::HostDisconnected, &recipients)
            .await;

        Ok(Outcome::Ended)
    }
}

/// Whether somebody who may run the broadcast is in the room.
///
/// The host, or a cohost: the two roles that can publish. Peers are what keep it alive,
/// which is why a listener in the room does not.
fn is_healthy(
    creator_id: Uuid,
    cohosts: &[super::model::BroadcastCohost],
    present: &[Uuid],
) -> bool {
    present.contains(&creator_id)
        || cohosts
            .iter()
            .any(|cohost| cohost.is_current() && present.contains(&cohost.cohost_id))
}

/// Everybody who has to be told the broadcast ended, once each.
///
/// The creator first, then each cohost, then everybody the room server reported. A
/// device that was both a cohost and connected appears once: two frames for one ending
/// is a client reacting twice.
fn recipients_of(
    creator_id: Uuid,
    cohosts: &[super::model::BroadcastCohost],
    present: &[Uuid],
) -> Vec<Uuid> {
    let mut recipients = Vec::with_capacity(cohosts.len() + present.len() + 1);
    recipients.push(creator_id);

    for id in cohosts
        .iter()
        .filter(|cohost| cohost.is_current())
        .map(|cohost| cohost.cohost_id)
        .chain(present.iter().copied())
    {
        if !recipients.contains(&id) {
            recipients.push(id);
        }
    }

    recipients
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::broadcast::dto::CreateBroadcastRequest;
    use crate::modules::broadcast::events::recording::RecordingEvents;
    use crate::modules::broadcast::media::RecordingMedia;
    use crate::modules::broadcast::model::{BroadcastCohost, UserSummary};
    use crate::modules::broadcast::repository::InMemoryBroadcastRepo;
    use crate::modules::broadcast::service::{BroadcastService, ServiceDeps};
    use crate::modules::broadcast::storage::BroadcastStorage;

    fn user(id: u128) -> UserSummary {
        UserSummary {
            id: Uuid::from_u128(id),
            full_name: format!("User {id}"),
            avatar_id: None,
            avatar_url: None,
        }
    }

    fn request() -> CreateBroadcastRequest {
        CreateBroadcastRequest {
            title: "Ada's show".to_owned(),
            description: None,
            image_id: None,
            image_url: None,
            time_zone: None,
            start_time: None,
            recording_enabled: None,
            cohosts: None,
        }
    }

    struct Fixture {
        repo: Arc<InMemoryBroadcastRepo>,
        media: Arc<RecordingMedia>,
        events: Arc<RecordingEvents>,
        service: BroadcastService,
        broadcast_id: Uuid,
    }

    /// A live broadcast whose host is in the room, ready to be abandoned.
    async fn live() -> Fixture {
        let repo = Arc::new(InMemoryBroadcastRepo::new());
        let media = Arc::new(RecordingMedia::new());
        let events = Arc::new(RecordingEvents::new());
        let service = BroadcastService::new(ServiceDeps {
            repo: repo.clone(),
            media: media.clone(),
            storage: BroadcastStorage::disabled(),
        });

        repo.insert_user(user(1));
        let created = service.create(request(), user(1)).await.expect("creating");
        service
            .go_live(created.id, &user(1))
            .await
            .expect("going live");
        media.set_room_participants(created.id, &[user(1).id]);

        Fixture {
            repo,
            media,
            events,
            service,
            broadcast_id: created.id,
        }
    }

    fn reaper(fixture: &Fixture) -> BroadcastReaper {
        BroadcastReaper::with_grace(
            fixture.repo.clone(),
            fixture.media.clone(),
            fixture.events.clone(),
            Duration::ZERO,
        )
    }

    #[tokio::test]
    async fn a_room_with_its_host_in_it_is_left_alone() {
        let fixture = live().await;

        let report = reaper(&fixture).sweep().await.expect("sweeping");

        assert_eq!(report.examined, 1);
        assert_eq!(report.ended, 0);
        assert_eq!(report.marked_abandoned, 0);
        assert!(fixture.events.is_empty());
        assert!(
            fixture
                .service
                .get(fixture.broadcast_id, None)
                .await
                .expect("reading")
                .state
                == crate::modules::broadcast::model::BroadcastState::Live
        );
    }

    #[tokio::test]
    async fn a_cohost_in_the_room_keeps_the_broadcast_on_air() {
        // The decision this test records: an absent host is not an abandonment while
        // somebody who can publish is still there.
        let repo = Arc::new(InMemoryBroadcastRepo::new());
        let media = Arc::new(RecordingMedia::new());
        let events = Arc::new(RecordingEvents::new());
        let service = BroadcastService::new(ServiceDeps {
            repo: repo.clone(),
            media: media.clone(),
            storage: BroadcastStorage::disabled(),
        });

        repo.insert_user(user(1));
        repo.insert_user(user(2));

        let created = service
            .create(
                CreateBroadcastRequest {
                    cohosts: Some(vec![user(2).id]),
                    ..request()
                },
                user(1),
            )
            .await
            .expect("creating");
        service
            .go_live(created.id, &user(1))
            .await
            .expect("going live");

        // The host is gone; the cohost is not.
        media.set_room_participants(created.id, &[user(2).id]);

        let fixture = Fixture {
            repo: repo.clone(),
            media: media.clone(),
            events: events.clone(),
            service,
            broadcast_id: created.id,
        };

        // Two sweeps: even with a zero grace period, a healthy room is never ended.
        let report = reaper(&fixture).sweep().await.expect("sweeping");
        assert_eq!(report.ended, 0);
        reaper(&fixture).sweep().await.expect("sweeping again");

        assert!(events.is_empty(), "a cohost was in the room");
        assert!(
            repo.find_by_id(created.id)
                .await
                .expect("reading")
                .is_some_and(|row| row.is_live())
        );
    }

    #[tokio::test]
    async fn nobody_in_the_room_ends_the_broadcast_after_the_grace_period() {
        let fixture = live().await;
        fixture.media.clear_room(fixture.broadcast_id);

        // First sweep: the clock starts.
        let first = reaper(&fixture).sweep().await.expect("sweeping");
        assert_eq!(first.marked_abandoned, 1);
        assert_eq!(first.ended, 0);
        assert!(fixture.events.is_empty());

        // Second sweep: the grace period is over.
        let second = reaper(&fixture).sweep().await.expect("sweeping");
        assert_eq!(second.ended, 1);

        let row = fixture
            .repo
            .find_by_id(fixture.broadcast_id)
            .await
            .expect("reading")
            .expect("the row survives");
        assert!(!row.is_live());
        assert_eq!(
            row.end_reason,
            Some(EndReason::HostDisconnected),
            "an abandoned broadcast is not a broadcast its host ended"
        );
        assert!(
            fixture
                .media
                .calls()
                .iter()
                .any(|call| call == &format!("end_room:{}", fixture.broadcast_id)),
            "the room is torn down, not left to the media server's timeout"
        );
    }

    #[tokio::test]
    async fn the_ending_is_announced_to_the_creator_cohosts_and_whoever_was_connected() {
        let repo = Arc::new(InMemoryBroadcastRepo::new());
        let media = Arc::new(RecordingMedia::new());
        let events = Arc::new(RecordingEvents::new());
        let service = BroadcastService::new(ServiceDeps {
            repo: repo.clone(),
            media: media.clone(),
            storage: BroadcastStorage::disabled(),
        });

        repo.insert_user(user(1));
        repo.insert_user(user(2));
        repo.insert_user(user(3));

        let created = service
            .create(
                CreateBroadcastRequest {
                    cohosts: Some(vec![user(2).id]),
                    ..request()
                },
                user(1),
            )
            .await
            .expect("creating");
        service
            .go_live(created.id, &user(1))
            .await
            .expect("going live");

        // The cohost dropped too; a listener is still listening.
        media.set_room_participants(created.id, &[user(3).id]);

        let fixture = Fixture {
            repo,
            media,
            events: events.clone(),
            service,
            broadcast_id: created.id,
        };

        reaper(&fixture).sweep().await.expect("sweeping");
        reaper(&fixture).sweep().await.expect("sweeping");

        let announced = events.announced();
        assert_eq!(announced.len(), 1);
        assert_eq!(announced[0].broadcast_id, created.id);
        assert_eq!(announced[0].reason, EndReason::HostDisconnected);
        assert_eq!(
            announced[0].recipients,
            [user(1).id, user(2).id, user(3).id],
            "the creator, the cohost, and the listener still connected"
        );
    }

    #[tokio::test]
    async fn a_broadcast_that_becomes_healthy_again_gets_its_clock_cleared() {
        let fixture = live().await;
        fixture.media.clear_room(fixture.broadcast_id);
        reaper(&fixture).sweep().await.expect("sweeping");

        let abandoned = fixture
            .repo
            .list_live()
            .await
            .expect("reading")
            .into_iter()
            .next()
            .expect("still live");
        assert!(abandoned.abandoned_since.is_some());

        // The host's phone found signal.
        fixture
            .media
            .set_room_participants(fixture.broadcast_id, &[user(1).id]);
        let report = reaper(&fixture).sweep().await.expect("sweeping");

        assert_eq!(report.cleared, 1);
        assert_eq!(report.ended, 0);

        let healthy = fixture
            .repo
            .list_live()
            .await
            .expect("reading")
            .into_iter()
            .next()
            .expect("still live");
        assert!(
            healthy.abandoned_since.is_none(),
            "a healthy broadcast must not carry a clock that ends it ten minutes later"
        );
    }

    #[tokio::test]
    async fn a_media_outage_never_ends_a_broadcast() {
        // §4.6: not being able to ask is not an answer. A sweep that ended broadcasts
        // because LiveKit was down would take every broadcast off air at once.
        let repo = Arc::new(InMemoryBroadcastRepo::new());
        let media = Arc::new(RecordingMedia::unavailable());
        let events = Arc::new(RecordingEvents::new());
        let service = BroadcastService::new(ServiceDeps {
            repo: repo.clone(),
            media: Arc::new(RecordingMedia::new()),
            storage: BroadcastStorage::disabled(),
        });

        repo.insert_user(user(1));
        let created = service.create(request(), user(1)).await.expect("creating");
        service
            .go_live(created.id, &user(1))
            .await
            .expect("going live");

        let fixture = Fixture {
            repo,
            media,
            events: events.clone(),
            service,
            broadcast_id: created.id,
        };

        let report = reaper(&fixture).sweep().await.expect("sweeping");
        assert_eq!(report.skipped, 1);
        assert_eq!(report.ended, 0);
        assert!(events.is_empty());
    }

    #[test]
    fn a_listener_alone_is_not_enough_to_keep_a_broadcast_alive() {
        let cohosts: Vec<BroadcastCohost> = Vec::new();
        assert!(!is_healthy(user(1).id, &cohosts, &[user(9).id]));
        assert!(!is_healthy(user(1).id, &cohosts, &[]));
        assert!(is_healthy(user(1).id, &cohosts, &[user(1).id]));
    }

    #[test]
    fn a_removed_cohost_does_not_keep_a_broadcast_alive() {
        let cohosts = vec![BroadcastCohost {
            broadcast_id: Uuid::from_u128(7),
            cohost_id: user(2).id,
            invited_by: user(1).id,
            invited_at: now(),
            removed_at: Some(now()),
        }];

        assert!(!is_healthy(user(1).id, &cohosts, &[user(2).id]));
        assert_eq!(
            recipients_of(user(1).id, &cohosts, &[user(2).id]),
            [user(1).id, user(2).id],
            "a removed cohost who is still in the room is told, and told once"
        );
    }
}
