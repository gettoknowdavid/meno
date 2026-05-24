use crate::modules::broadcast::errors::BroadcastError;
use crate::modules::broadcast::model::ParticipantRole;

#[derive(Debug, Clone, PartialEq)]
pub enum LivekitRole {
    Host,
    Cohost,
    Participant,
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
