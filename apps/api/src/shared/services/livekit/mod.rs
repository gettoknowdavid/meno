pub mod dto;

use crate::config::MenoConfig;
use crate::modules::broadcast::dto::UserSummary;
use crate::shared::constants::LIVEKIT_ACCESS_TOKEN_TTL;
use crate::shared::services::livekit::dto::{LivekitParticipantInfo, LivekitRole};
use livekit_api::access_token::{AccessToken, AccessTokenError, VideoGrants};
use livekit_api::services::ServiceError;
use livekit_api::services::room::{CreateRoomOptions, RoomClient, UpdateParticipantOptions};
use livekit_protocol::ParticipantPermission;
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct LivekitService {
    pub api_key: String,
    pub api_secret: String,
    pub host: String,
    pub room: Arc<RoomClient>,
}
impl LivekitService {
    pub fn new(config: &MenoConfig, room: Arc<RoomClient>) -> Self {
        let api_key = config.livekit_api_key.clone();
        let api_secret = config.livekit_api_secret.clone();
        let host = config.livekit_host.clone();
        Self {
            api_key,
            api_secret,
            host,
            room,
        }
    }

    pub async fn mint_token(
        &self,
        user_id: Uuid,
        user_name: &str,
        broadcast_id: Uuid,
        role: LivekitRole,
    ) -> Result<String, AccessTokenError> {
        let room_name = broadcast_id.to_string();

        let identity = user_id.to_string();

        let grant = match role {
            LivekitRole::Host => VideoGrants {
                room: room_name.clone(),
                room_join: true,
                can_subscribe: true,
                can_publish: true,
                room_admin: true,
                ..Default::default()
            },
            LivekitRole::Cohost => VideoGrants {
                room: room_name.clone(),
                room_join: true,
                can_subscribe: true,
                can_publish: true,
                room_admin: false,
                ..Default::default()
            },
            LivekitRole::Participant => VideoGrants {
                room: room_name.clone(),
                room_join: true,
                can_subscribe: true,
                can_publish: false,
                room_admin: false,
                ..Default::default()
            },
        };

        AccessToken::with_api_key(&self.api_key, &self.api_secret)
            .with_identity(&identity)
            .with_name(&user_name)
            .with_grants(grant)
            .with_ttl(Duration::from_secs(LIVEKIT_ACCESS_TOKEN_TTL as u64))
            .to_jwt()
    }

    pub async fn mint_host_token(
        &self,
        host: &UserSummary,
        broadcast_id: Uuid,
    ) -> Result<String, AccessTokenError> {
        self.mint_token(host.id, &host.full_name, broadcast_id, LivekitRole::Host)
            .await
    }

    pub async fn mint_cohost_token(
        &self,
        host: &UserSummary,
        broadcast_id: Uuid,
    ) -> Result<String, AccessTokenError> {
        self.mint_token(host.id, &host.full_name, broadcast_id, LivekitRole::Cohost)
            .await
    }

    pub async fn mint_participant_token(
        &self,
        host: &UserSummary,
        broadcast_id: Uuid,
    ) -> Result<String, AccessTokenError> {
        self.mint_token(
            host.id,
            &host.full_name,
            broadcast_id,
            LivekitRole::Participant,
        )
        .await
    }

    pub async fn create_room(&self, broadcast_id: Uuid) -> Result<(), ServiceError> {
        let room_name = broadcast_id.to_string();
        let options = CreateRoomOptions {
            max_participants: 10000,
            empty_timeout: 300,
            metadata: room_name.clone(),
            ..Default::default()
        };
        self.room.create_room(&room_name, options).await?;
        Ok(())
    }

    pub async fn delete_room(&self, broadcast_id: Uuid) -> Result<(), ServiceError> {
        self.room.delete_room(&broadcast_id.to_string()).await?;
        Ok(())
    }

    pub async fn list_participants(
        &self,
        broadcast_id: Uuid,
    ) -> Result<Vec<LivekitParticipantInfo>, ServiceError> {
        let room_name = broadcast_id.to_string();
        let participants = self.room.list_participants(&room_name).await?;
        let mut result = Vec::with_capacity(participants.len());
        for participant in participants {
            if let Ok(id) = Uuid::parse_str(&participant.identity) {
                result.push(LivekitParticipantInfo {
                    id,
                    joined_at: OffsetDateTime::from_unix_timestamp(participant.joined_at)
                        .unwrap_or(OffsetDateTime::now_utc()),
                })
            }
        }
        Ok(result)
    }

    pub async fn remove_participant(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), ServiceError> {
        let room = broadcast_id.to_string();
        let identifier = user_id.to_string();
        self.room.remove_participant(&room, &identifier).await?;
        Ok(())
    }

    pub async fn update_permission(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
        can_publish: bool,
    ) -> Result<(), ServiceError> {
        let room = broadcast_id.to_string();
        let identifier = user_id.to_string();
        let options = UpdateParticipantOptions {
            permission: Some(ParticipantPermission {
                can_subscribe: true,
                can_publish: false,
                can_publish_data: can_publish,
                ..Default::default()
            }),
            ..Default::default()
        };
        self.room
            .update_participant(&room, &identifier, options)
            .await?;
        Ok(())
    }

    pub async fn mute_participant(
        &self,
        broadcast_id: Uuid,
        user_id: Uuid,
        track_sid: &str,
        muted: bool,
    ) -> Result<(), ServiceError> {
        let room = broadcast_id.to_string();
        let identifier = user_id.to_string();
        self.room
            .mute_published_track(&room, &identifier, track_sid, muted)
            .await?;
        Ok(())
    }
}
