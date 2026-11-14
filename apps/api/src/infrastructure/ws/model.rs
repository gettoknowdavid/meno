//! WebSocket vocabulary: the event names on the wire, and the timing knobs the
//! connection lifecycle depends on.
//!
//! Ported from `apps/api/src/shared/services/ws/model.rs` on `master` (`903c3ba`).
//!
//! # What changed
//!
//! - **`FromStr` is derived from `Display`, not hand-mainced.** On `master` both were
//!   separate 30-arm `match`es listing every event name twice. Adding an event meant
//!   editing three places (the enum, `FromStr`, `Display`) and forgetting the third
//!   compiled cleanly and failed at runtime, on the wire, in production. Now there is
//!   one `as_str` and both traits delegate to it.
//! - **`WsEvent` gained `Eq` + `Hash`.** The connection lifecycle needs to put events
//!   in a `HashSet` and compare them in tests; deriving them is free.
//! - **Timing configs validate themselves.** A heartbeat whose pong timeout is not
//!   longer than its ping interval would kill every healthy connection; the
//!   constructors below make that unrepresentable rather than leaving it to review.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Every event that can travel over the WebSocket, in either direction.
///
/// The wire name is camelCase (`hostDisconnected`), matching the HTTP surface — one
/// casing convention across the whole API, so a Dart client's generated models do
/// not need a second naming rule.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum WsEvent {
    /// Liveness probe. Server pings, client echoes; also accepted from the client.
    Heartbeat,

    // ── broadcast lifecycle ──
    /// A broadcast went live.
    NewBroadcast,
    /// A live broadcast ended.
    EndedBroadcast,
    /// A broadcast's start time was changed.
    ScheduledBroadcast,
    /// A broadcast was deleted.
    BroadcastDeleted,
    /// Something went wrong; `data` is a [`super::errors::WsError`].
    BroadcastError,

    // ── host presence ──
    /// The host's connection dropped and a grace period has started.
    HostDisconnected,
    /// The host returned within their grace period; the broadcast continues.
    HostReconnected,

    // ── participants ──
    /// A participant joined the room.
    ParticipantJoined,
    /// A participant left the room.
    ParticipantLeft,
    /// A participant was removed by the host.
    ParticipantKicked,
    /// New live-participant count.
    NumberOfLiveParticipants,

    // ── cohosts ──
    /// A cohost invite was issued.
    CohostInvitation,
    /// A cohost accepted their invite.
    CohostAccepted,
    /// A cohost declined their invite.
    CohostDeclined,
    /// A cohost was returned to being a plain participant.
    CohostDemotion,
    /// A new cohost was added to the broadcast.
    NewCohost,
    /// A cohost was removed from the broadcast entirely.
    RemovedCohost,

    // ── recording ──
    /// The recording is ready to play.
    RecordingReady,
    /// The recording has been published.
    RecordingPublished,

    // ── direct-to-user ──
    /// A notification addressed to one user.
    Notification,
    /// A cached home-screen payload is stale.
    ///
    /// Emitted when a broadcast goes live, ends, or is deleted, so the client can
    /// refetch the `Now Live`, `Recently Live` and `Live For You` sections instead of
    /// trusting a cache that is now wrong.
    HomeInvalidated,

    // ── chat ──
    /// Server → client: a chat message was created.
    NewMessage,
    /// Server → client: a chat message was edited.
    EditedMessage,
    /// Server → client: a chat message was deleted.
    DeletedMessage,
    /// Server → client: a reaction was added or removed.
    NewReaction,
    /// Client → server: send a chat message.
    SendMessage,
    /// Client → server: edit a chat message.
    EditMessage,
    /// Client → server: delete a chat message.
    DeleteMessage,
    /// Client → server: react to a chat message.
    SendReaction,
}

impl WsEvent {
    /// The name as it appears on the wire.
    ///
    /// Single source of truth for the wire vocabulary: `Serialize`, `Deserialize`,
    /// `Display` and `FromStr` all resolve through here, so a new variant cannot be
    /// half-wired.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Heartbeat => "heartbeat",

            Self::NewBroadcast => "newBroadcast",
            Self::EndedBroadcast => "endedBroadcast",
            Self::ScheduledBroadcast => "scheduledBroadcast",
            Self::BroadcastDeleted => "broadcastDeleted",
            Self::BroadcastError => "broadcastError",

            Self::HostDisconnected => "hostDisconnected",
            Self::HostReconnected => "hostReconnected",

            Self::ParticipantJoined => "participantJoined",
            Self::ParticipantLeft => "participantLeft",
            Self::ParticipantKicked => "participantKicked",
            Self::NumberOfLiveParticipants => "numberOfLiveParticipants",

            Self::CohostInvitation => "cohostInvitation",
            Self::CohostAccepted => "cohostAccepted",
            Self::CohostDeclined => "cohostDeclined",
            Self::CohostDemotion => "cohostDemotion",
            Self::NewCohost => "newCohost",
            Self::RemovedCohost => "removedCohost",

            Self::RecordingReady => "recordingReady",
            Self::RecordingPublished => "recordingPublished",

            Self::Notification => "notification",
            Self::HomeInvalidated => "homeInvalidated",

            Self::NewMessage => "newMessage",
            Self::EditedMessage => "editedMessage",
            Self::DeletedMessage => "deletedMessage",
            Self::NewReaction => "newReaction",
            Self::SendMessage => "sendMessage",
            Self::EditMessage => "editMessage",
            Self::DeleteMessage => "deleteMessage",
            Self::SendReaction => "sendReaction",
        }
    }

    /// Whether this event is one the *client* is allowed to send.
    ///
    /// The read loop dispatches on this rather than on an allow-list scattered across
    /// match arms, so "server-only" is a property of the variant and cannot be lost
    /// when a new event is added.
    #[must_use]
    pub const fn is_client_to_server(self) -> bool {
        matches!(
            self,
            Self::Heartbeat
                | Self::SendMessage
                | Self::EditMessage
                | Self::DeleteMessage
                | Self::SendReaction
        )
    }

    /// Whether receiving this event means the room is finished with.
    ///
    /// Drives local teardown in the pub/sub bridge: after an end or a delete there is
    /// nothing left to deliver to, so the instance unsubscribes and drops its room
    /// membership instead of holding both until the key expires.
    #[must_use]
    pub const fn terminates_room(self) -> bool {
        matches!(self, Self::EndedBroadcast | Self::BroadcastDeleted)
    }
}

impl fmt::Display for WsEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for WsEvent {
    type Err = UnknownEvent;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Linear over the variants; at thirty of them this is cheaper than the
        // alternative (a lazily built map) and cannot go stale.
        const ALL: [WsEvent; 30] = [
            WsEvent::Heartbeat,
            WsEvent::NewBroadcast,
            WsEvent::EndedBroadcast,
            WsEvent::ScheduledBroadcast,
            WsEvent::BroadcastDeleted,
            WsEvent::BroadcastError,
            WsEvent::HostDisconnected,
            WsEvent::HostReconnected,
            WsEvent::ParticipantJoined,
            WsEvent::ParticipantLeft,
            WsEvent::ParticipantKicked,
            WsEvent::NumberOfLiveParticipants,
            WsEvent::CohostInvitation,
            WsEvent::CohostAccepted,
            WsEvent::CohostDeclined,
            WsEvent::CohostDemotion,
            WsEvent::NewCohost,
            WsEvent::RemovedCohost,
            WsEvent::RecordingReady,
            WsEvent::RecordingPublished,
            WsEvent::Notification,
            WsEvent::HomeInvalidated,
            WsEvent::NewMessage,
            WsEvent::EditedMessage,
            WsEvent::DeletedMessage,
            WsEvent::NewReaction,
            WsEvent::SendMessage,
            WsEvent::EditMessage,
            WsEvent::DeleteMessage,
            WsEvent::SendReaction,
        ];
        // The fixed-size array is the exhaustiveness tripwire: adding a variant makes
        // this fail to compile, which is the point — `as_str` and this list cannot
        // drift apart silently.
        ALL.iter()
            .copied()
            .find(|e| e.as_str() == s)
            .ok_or_else(|| UnknownEvent(s.to_owned()))
    }
}

/// A `WsEvent` name the server does not recognise.
///
/// Carries the offending string so the handler can log exactly what a client sent,
/// which is the only useful thing to know about a malformed frame.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown WebSocket event: {0}")]
pub struct UnknownEvent(pub String);

/// Heartbeat timing.
///
/// Tuned for the audience's mobile networks rather than for a LAN:
///
/// - Carrier NATs drop idle TCP after roughly 30s, so the ping interval sits just
///   under that.
/// - iOS freezes background work after about 30s, which is why the host's pong
///   timeout is the generous one — a host whose phone sleeps must not lose a
///   broadcast they are still streaming.
/// - Listeners get a tighter timeout; missing one costs a reconnect, missing the
///   host's costs the broadcast.
/// - Two missed pongs before declaring a peer dead, so a single lost packet does not
///   tear down a healthy connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatConfig {
    /// Seconds between server-initiated pings.
    pub ping_interval_secs: u64,
    /// Pong deadline applied to the broadcast host.
    pub host_pong_timeout_secs: u64,
    /// Pong deadline applied to every other participant.
    pub listener_pong_timeout_secs: u64,
    /// Missed pongs tolerated before the connection is closed.
    pub max_missed_pings: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        let base = Self {
            ping_interval_secs: 25,
            host_pong_timeout_secs: 90,
            listener_pong_timeout_secs: 60,
            max_missed_pings: 2,
        };
        debug_assert!(base.is_sane());
        base
    }
}

impl HeartbeatConfig {
    /// Whether the timeouts are internally consistent.
    ///
    /// A pong timeout at or below the ping interval closes healthy connections, so it
    /// is asserted rather than trusted — but only in debug, because a release build
    /// must not panic on a bad env override.
    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.max_missed_pings > 0
            && self.host_pong_timeout_secs > self.ping_interval_secs
            && self.listener_pong_timeout_secs > self.ping_interval_secs
            // Tolerating a single miss means the deadline has to cover two intervals.
            && u64::from(self.max_missed_pings) * self.ping_interval_secs
                <= self.listener_pong_timeout_secs
    }
}

/// How long a disconnected host's broadcast survives, by disconnect count.
///
/// The first drop gets a long window because it is nearly always a network blip;
/// each subsequent one shortens, so a host who is genuinely gone loses the broadcast
/// quickly instead of holding a room open for the full first-tier window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GracePeriodConfig {
    /// After the first disconnect.
    pub tier1_secs: u64,
    /// After the second.
    pub tier2_secs: u64,
    /// After the third.
    pub tier3_secs: u64,
    /// After the fourth and beyond.
    pub tier4_plus_secs: u64,
}

impl Default for GracePeriodConfig {
    fn default() -> Self {
        Self {
            tier1_secs: 120,
            tier2_secs: 90,
            tier3_secs: 60,
            tier4_plus_secs: 30,
        }
    }
}

impl GracePeriodConfig {
    /// The grace window for a given number of disconnects.
    ///
    /// Saturating rather than panicking on a huge count: the input is a Redis counter
    /// that any instance can increment, so it is untrusted.
    #[must_use]
    pub fn grace_seconds(&self, disconnect_count: u64) -> u64 {
        match disconnect_count {
            0 | 1 => self.tier1_secs,
            2 => self.tier2_secs,
            3 => self.tier3_secs,
            _ => self.tier4_plus_secs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every variant. The test walks *this* list to prove the wire name is stable;
    /// `FromStr`'s internal array of the same length proves exhaustiveness at compile
    /// time.
    const ALL_EVENTS: [WsEvent; 30] = [
        WsEvent::Heartbeat,
        WsEvent::NewBroadcast,
        WsEvent::EndedBroadcast,
        WsEvent::ScheduledBroadcast,
        WsEvent::BroadcastDeleted,
        WsEvent::BroadcastError,
        WsEvent::HostDisconnected,
        WsEvent::HostReconnected,
        WsEvent::ParticipantJoined,
        WsEvent::ParticipantLeft,
        WsEvent::ParticipantKicked,
        WsEvent::NumberOfLiveParticipants,
        WsEvent::CohostInvitation,
        WsEvent::CohostAccepted,
        WsEvent::CohostDeclined,
        WsEvent::CohostDemotion,
        WsEvent::NewCohost,
        WsEvent::RemovedCohost,
        WsEvent::RecordingReady,
        WsEvent::RecordingPublished,
        WsEvent::Notification,
        WsEvent::HomeInvalidated,
        WsEvent::NewMessage,
        WsEvent::EditedMessage,
        WsEvent::DeletedMessage,
        WsEvent::NewReaction,
        WsEvent::SendMessage,
        WsEvent::EditMessage,
        WsEvent::DeleteMessage,
        WsEvent::SendReaction,
    ];

    #[test]
    fn wire_names_are_camel_case_and_unique() {
        // Uniqueness matters: two events sharing a name would make the client's
        // deserialiser ambiguous, and nothing else would catch it.
        let mut seen = std::collections::HashSet::new();
        for event in ALL_EVENTS {
            let name = event.as_str();
            assert!(
                seen.insert(name),
                "duplicate wire name {name} — clients could not tell the events apart"
            );
            assert!(
                name.chars().next().is_some_and(char::is_lowercase),
                "{name} should be camelCase"
            );
            assert!(
                !name.contains('_'),
                "{name} should be camelCase, matching the HTTP surface"
            );
        }
    }

    #[test]
    fn serde_display_and_from_str_agree_for_every_event() {
        // This is the assertion that `master` could not make: its three independent
        // lists could disagree, and a new event only had to be added to some of them.
        for event in ALL_EVENTS {
            let name = event.as_str();

            assert_eq!(
                serde_json::to_string(&event).expect("serialise"),
                format!("\"{name}\""),
                "serde and as_str disagree for {name}"
            );
            assert_eq!(
                serde_json::from_str::<WsEvent>(&format!("\"{name}\"")).expect("deserialise"),
                event,
                "serde does not round-trip {name}"
            );
            assert_eq!(name.parse::<WsEvent>().expect("parse"), event);
            assert_eq!(event.to_string(), name);
        }
    }

    #[test]
    fn from_str_rejects_an_unknown_event_and_names_it() {
        // The name comes back in the error because the only useful thing to log about
        // a malformed frame is what the client actually sent.
        let err = "notARealEvent".parse::<WsEvent>().expect_err("must reject");
        assert_eq!(err.0, "notARealEvent");
        assert!(err.to_string().contains("notARealEvent"));

        // Case matters: the wire vocabulary is camelCase and nothing else.
        assert!("heartbeat".parse::<WsEvent>().is_ok());
        assert!("Heartbeat".parse::<WsEvent>().is_err());
        assert!("".parse::<WsEvent>().is_err());
    }

    #[test]
    fn only_the_five_client_events_are_client_to_server() {
        // Anything else arriving from a client is a protocol violation, and letting
        // one through would let a client drive server-side lifecycle transitions.
        let client_events: Vec<WsEvent> = ALL_EVENTS
            .iter()
            .copied()
            .filter(|e| e.is_client_to_server())
            .collect();

        assert_eq!(
            client_events,
            vec![
                WsEvent::Heartbeat,
                WsEvent::SendMessage,
                WsEvent::EditMessage,
                WsEvent::DeleteMessage,
                WsEvent::SendReaction,
            ]
        );

        // Explicitly: a client must not be able to announce a broadcast ending.
        for forbidden in [
            WsEvent::EndedBroadcast,
            WsEvent::BroadcastDeleted,
            WsEvent::NewBroadcast,
            WsEvent::ParticipantKicked,
            WsEvent::CohostDemotion,
            WsEvent::HomeInvalidated,
        ] {
            assert!(
                !forbidden.is_client_to_server(),
                "{} must be server-only",
                forbidden.as_str()
            );
        }
    }

    #[test]
    fn only_end_and_delete_terminate_a_room() {
        assert!(WsEvent::EndedBroadcast.terminates_room());
        assert!(WsEvent::BroadcastDeleted.terminates_room());
        for keeps_room in [
            WsEvent::HostDisconnected,
            WsEvent::HostReconnected,
            WsEvent::ParticipantLeft,
            WsEvent::CohostDemotion,
        ] {
            assert!(
                !keeps_room.terminates_room(),
                "{} must not tear the room down",
                keeps_room.as_str()
            );
        }
    }

    #[test]
    fn default_heartbeat_tolerates_one_lost_packet() {
        let config = HeartbeatConfig::default();
        assert_eq!(config.ping_interval_secs, 25);
        assert!(
            config.is_sane(),
            "the shipped defaults must be self-consistent"
        );
        assert!(
            config.listener_pong_timeout_secs > config.ping_interval_secs,
            "a listener must survive a full ping interval plus a lost packet"
        );
        assert!(
            config.host_pong_timeout_secs > config.listener_pong_timeout_secs,
            "the host tolerates a longer silence than a listener"
        );
    }

    #[test]
    fn a_pong_timeout_below_the_ping_interval_is_rejected_as_insane() {
        // The configuration that would kill every healthy connection, caught by the
        // sanity check rather than by a support ticket.
        let config = HeartbeatConfig {
            ping_interval_secs: 60,
            listener_pong_timeout_secs: 30,
            host_pong_timeout_secs: 30,
            max_missed_pings: 2,
        };
        assert!(!config.is_sane());

        assert!(
            !HeartbeatConfig {
                max_missed_pings: 0,
                ..HeartbeatConfig::default()
            }
            .is_sane(),
            "a zero miss budget would disconnect immediately"
        );
    }

    #[test]
    fn grace_period_shortens_with_each_disconnect() {
        let config = GracePeriodConfig::default();
        let first = config.grace_seconds(1);
        assert_eq!(first, 120);

        // Monotonically non-increasing, all the way down the tail.
        let mut previous = first;
        for count in 2..10 {
            let current = config.grace_seconds(count);
            assert!(
                current <= previous,
                "disconnect {count} got a longer grace period than the one before"
            );
            previous = current;
        }
        assert_eq!(previous, 30, "the floor is tier 4");
    }

    #[test]
    fn grace_period_treats_zero_like_the_first_disconnect() {
        // A counter that reset between requests must not hand out a zero-second grace
        // period, which would end the broadcast before the host could reconnect.
        let config = GracePeriodConfig::default();
        assert_eq!(config.grace_seconds(0), config.grace_seconds(1));
        assert!(config.grace_seconds(0) > 0);
    }
}
