use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// WebSocket event types - both client and server events
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum WsEvent {
    Heartbeat,

    NewBroadcast,
    EndedBroadcast,
    ScheduledBroadcast,
    BroadcastDeleted,
    BroadcastError,

    HostDisconnected,
    HostReconnected,

    ParticipantJoined,
    ParticipantLeft,
    ParticipantKicked,

    NumberOfLiveParticipants,

    CohostInvitation,
    CohostAccepted,
    CohostDeclined,
    /// When a cohost is returned to being a participant
    CohostDemotion,

    /// A new cohost has been added
    NewCohost,

    /// When a cohost is removed from the broadcast entirely
    RemovedCohost,

    RecordingReady,
    RecordingPublished,

    Notification,

    /// This event is emitted when a broadcast goes live, ends, or is deleted.
    /// It is a lightweight way to notify the FE of a change in the broadcast list, so it can
    /// refetch the `Now Live`, `Recently Live` and `Live For You` sections of the home page.
    HomeInvalidated,

    NewMessage,
    EditedMessage,
    DeletedMessage,
    NewReaction,
    SendMessage,
    EditMessage,
    DeleteMessage,
    SendReaction,
}
impl FromStr for WsEvent {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "heartbeat" => Ok(WsEvent::Heartbeat),
            "newBroadcast" => Ok(WsEvent::NewBroadcast),
            "endedBroadcast" => Ok(WsEvent::EndedBroadcast),
            "scheduledBroadcast" => Ok(WsEvent::ScheduledBroadcast),
            "broadcastDeleted" => Ok(WsEvent::BroadcastDeleted),
            "broadcastError" => Ok(WsEvent::BroadcastError),
            "hostDisconnected" => Ok(WsEvent::HostDisconnected),
            "hostReconnected" => Ok(WsEvent::HostReconnected),
            "participantJoined" => Ok(WsEvent::ParticipantJoined),
            "participantLeft" => Ok(WsEvent::ParticipantLeft),
            "participantKicked" => Ok(WsEvent::ParticipantKicked),
            "numberOfLiveParticipants" => Ok(WsEvent::NumberOfLiveParticipants),
            "cohostInvitation" => Ok(WsEvent::CohostInvitation),
            "cohostAccepted" => Ok(WsEvent::CohostAccepted),
            "cohostDeclined" => Ok(WsEvent::CohostDeclined),
            "newCohost" => Ok(WsEvent::NewCohost),
            "removedCohost" => Ok(WsEvent::RemovedCohost),
            "cohostDemotion" => Ok(WsEvent::CohostDemotion),
            "recordingReady" => Ok(WsEvent::RecordingReady),
            "recordingPublished" => Ok(WsEvent::RecordingPublished),
            "notification" => Ok(WsEvent::Notification),
            "homeInvalidated" => Ok(WsEvent::HomeInvalidated),
            "newMessage" => Ok(WsEvent::NewMessage),
            "editedMessage" => Ok(WsEvent::EditedMessage),
            "deletedMessage" => Ok(WsEvent::DeletedMessage),
            "newReaction" => Ok(WsEvent::NewReaction),
            "sendMessage" => Ok(WsEvent::SendMessage),
            "editMessage" => Ok(WsEvent::EditMessage),
            "deleteMessage" => Ok(WsEvent::DeleteMessage),
            "sendReaction" => Ok(WsEvent::SendReaction),
            _ => Err(format!("Unknown WebSocket event: {s}")),
        }
    }
}
impl fmt::Display for WsEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            WsEvent::Heartbeat => "heartbeat",
            WsEvent::NewBroadcast => "newBroadcast",
            WsEvent::EndedBroadcast => "endedBroadcast",
            WsEvent::ScheduledBroadcast => "scheduledBroadcast",
            WsEvent::BroadcastDeleted => "broadcastDeleted",
            WsEvent::BroadcastError => "broadcastError",
            WsEvent::HostDisconnected => "hostDisconnected",
            WsEvent::HostReconnected => "hostReconnected",
            WsEvent::ParticipantJoined => "participantJoined",
            WsEvent::ParticipantLeft => "participantLeft",
            WsEvent::ParticipantKicked => "participantKicked",
            WsEvent::NumberOfLiveParticipants => "numberOfLiveParticipants",
            WsEvent::CohostInvitation => "cohostInvitation",
            WsEvent::CohostAccepted => "cohostAccepted",
            WsEvent::CohostDeclined => "cohostDeclined",
            WsEvent::CohostDemotion => "cohostDemotion",
            WsEvent::NewCohost => "newCohost",
            WsEvent::RemovedCohost => "removedCohost",
            WsEvent::RecordingReady => "recordingReady",
            WsEvent::RecordingPublished => "recordingPublished",
            WsEvent::Notification => "notification",
            WsEvent::HomeInvalidated => "homeInvalidated",
            WsEvent::NewMessage => "newMessage",
            WsEvent::EditedMessage => "editedMessage",
            WsEvent::DeletedMessage => "deletedMessage",
            WsEvent::NewReaction => "newReaction",
            WsEvent::SendMessage => "sendMessage",
            WsEvent::EditMessage => "editMessage",
            WsEvent::DeleteMessage => "deleteMessage",
            WsEvent::SendReaction => "sendReaction",
        };
        write!(f, "{s}")
    }
}

/// Below is an AI-generated explanation, but it here to help understand the reason for using
/// this to solve the unstable network issue
///
/// Heartbeat configuration tuned for Nigerian mobile networks
///
/// Rationale:
/// - Nigerian mobile NATs drop idle TCP after ~30s → ping every 25s
/// - iOS background budget: up to 30s before processing is frozen → 60s pong timeout for hosts
/// - Listeners get 20s timeout (more aggressive, less critical)
/// - 2 missed pongs before declaring dead (handles one brief packet loss event)
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    pub ping_interval_secs: u64,
    pub host_pong_timeout_secs: u64,
    pub listener_pong_timeout_secs: u64,
    pub max_missed_pings: u32,
}
impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            ping_interval_secs: 25,
            host_pong_timeout_secs: 60,
            listener_pong_timeout_secs: 20,
            max_missed_pings: 2,
        }
    }
}

/// Tiered grace period configuration
/// First disconnect: 120s (generous enough for a network blip)
/// Subsequent: 90s, 60s, 30s
#[derive(Debug, Clone)]
pub struct GracePeriodConfig {
    pub tier1_secs: u64,
    pub tier2_secs: u64,
    pub tier3_secs: u64,
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
    #[must_use]
    pub fn get_grace_seconds(&self, disconnect_count: u64) -> u64 {
        match disconnect_count {
            1 => self.tier1_secs,
            2 => self.tier2_secs,
            3 => self.tier3_secs,
            _ => self.tier4_plus_secs,
        }
    }
}
