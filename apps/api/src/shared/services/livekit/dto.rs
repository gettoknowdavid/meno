use crate::modules::broadcast::errors::BroadcastError;
use crate::modules::broadcast::model::ParticipantRole;
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq)]
pub enum LivekitRole {
    Host,
    Cohost,
    Participant,
}
impl Display for LivekitRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            LivekitRole::Host => "host".to_string(),
            LivekitRole::Cohost => "cohost".to_string(),
            LivekitRole::Participant => "participant".to_string(),
        };
        write!(f, "{}", str)
    }
}
impl TryFrom<ParticipantRole> for LivekitRole {
    type Error = BroadcastError;
    fn try_from(role: ParticipantRole) -> Result<Self, Self::Error> {
        match role {
            ParticipantRole::Host => Ok(LivekitRole::Host),
            ParticipantRole::Cohost => Ok(LivekitRole::Cohost),
            ParticipantRole::Participant => Ok(LivekitRole::Participant),
            ParticipantRole::None => Err(BroadcastError::NotParticipant),
        }
    }
}
