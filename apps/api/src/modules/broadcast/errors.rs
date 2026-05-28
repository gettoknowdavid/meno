use crate::modules::broadcast::dto::MAX_COHOSTS;
use crate::shared::errors::{error_response, validation_error_response};
use axum::http::StatusCode;
use validator::ValidationErrors;

#[derive(thiserror::Error, Debug)]
pub enum BroadcastError {
    // 400 - BAD_REQUEST
    #[error("Broadcast is not currently live")]
    NotLive,

    #[error("Broadcast is already live")]
    AlreadyLive,

    #[error("You are not a participant in this broadcast")]
    NotParticipant,

    #[error("A join request is already in progress")]
    JoinInProgress,

    #[error("Invalid time zone: {0}")]
    InvalidTimeZone(String),

    #[error("start_time must be in the future")]
    StartTimeInPast,

    #[error("Recording not available for this broadcast")]
    RecordingNotAvailable,

    #[error("Cannot publish a broadcast that is still live")]
    BroadcastStillLive,

    #[error("No or invalid broadcast ID")]
    InvalidId,

    // 403 - FORBIDDEN
    #[error("Only the broadcast creator can perform this action")]
    NotCreator,

    #[error("You cannot join a broadcast you created")]
    CreatorCannotJoin,

    #[error("Only the creator or admin can end this broadcast")]
    CannotEnd,

    #[error("Cannot add yourself as a cohost")]
    CannotAddSelfAsCohost,

    #[error("Cohost limit exceeded. Maximum {0} cohosts allowed")]
    CohostLimitExceeded(usize),

    #[error("One or more users were not found")]
    OneOrMoreUsersNotFound,

    #[error("Cannot modify a broadcast that is currently live")]
    CannotModifyLiveBroadcast,

    #[error("This invitation is not addressed to you")]
    InvitationNotYours,

    // 404 - NOT_FOUND
    #[error("Broadcast not found")]
    NotFound,

    #[error("User not found")]
    UserNotFound,

    #[error("Cohost invitation not found")]
    InvitationNotFound,

    // 409 - CONFLICT
    #[error("User is already a co-host of this broadcast")]
    AlreadyCohost,

    #[error("This user already has a pending co-host invitation")]
    DuplicateInvitation,

    // 429 - TOO_MANY_REQUESTS
    #[error("Too many reconnection attempts. Wait 30 seconds")]
    TooManyReconnects,

    // 503 SERVICE_UNAVAILABLE
    #[error("Could not connect to media server. Please try again")]
    LiveKitUnavailable,

    // 500 INTERNAL
    #[error(transparent)]
    LiveKitAccess(#[from] livekit_api::access_token::AccessTokenError),

    #[error(transparent)]
    LiveKit(#[from] livekit_api::services::ServiceError),

    #[error(transparent)]
    Redis(#[from] fred::error::Error),

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),

    #[error("Validation error")]
    ValidationError(#[from] ValidationErrors),
}
impl axum::response::IntoResponse for BroadcastError {
    fn into_response(self) -> axum::response::Response {
        match &self {
            BroadcastError::NotLive => error_response(
                StatusCode::BAD_REQUEST,
                "BROADCAST_NOT_LIVE",
                &self.to_string(),
            ),
            BroadcastError::AlreadyLive => error_response(
                StatusCode::BAD_REQUEST,
                "BROADCAST_ALREADY_LIVE",
                &self.to_string(),
            ),
            BroadcastError::NotParticipant => error_response(
                StatusCode::BAD_REQUEST,
                "NOT_PARTICIPANT",
                &self.to_string(),
            ),
            BroadcastError::JoinInProgress => error_response(
                StatusCode::BAD_REQUEST,
                "JOIN_IN_PROGRESS",
                &self.to_string(),
            ),
            BroadcastError::InvalidTimeZone(_) => error_response(
                StatusCode::BAD_REQUEST,
                "INVALID_TIME_ZONE",
                &self.to_string(),
            ),
            BroadcastError::StartTimeInPast => error_response(
                StatusCode::BAD_REQUEST,
                "START_TIME_IN_PAST",
                &self.to_string(),
            ),
            BroadcastError::RecordingNotAvailable => error_response(
                StatusCode::BAD_REQUEST,
                "RECORDING_NOT_AVAILABLE",
                &self.to_string(),
            ),
            BroadcastError::BroadcastStillLive => error_response(
                StatusCode::BAD_REQUEST,
                "BROADCAST_STILL_LIVE",
                &self.to_string(),
            ),
            BroadcastError::InvalidId => error_response(
                StatusCode::BAD_REQUEST,
                "INVALID_BROADCAST_ID",
                &self.to_string(),
            ),
            BroadcastError::NotCreator => error_response(
                StatusCode::FORBIDDEN,
                "NOT_BROADCAST_CREATOR",
                &self.to_string(),
            ),
            BroadcastError::CreatorCannotJoin => error_response(
                StatusCode::FORBIDDEN,
                "CREATOR_CANNOT_JOIN",
                &self.to_string(),
            ),
            BroadcastError::CannotEnd => error_response(
                StatusCode::FORBIDDEN,
                "CANNOT_END_BROADCAST",
                &self.to_string(),
            ),
            BroadcastError::CannotAddSelfAsCohost => error_response(
                StatusCode::FORBIDDEN,
                "CANNOT_ADD_SELF_AS_COHOST",
                &self.to_string(),
            ),
            BroadcastError::CannotModifyLiveBroadcast => error_response(
                StatusCode::FORBIDDEN,
                "CANNOT_MODIFY_LIVE_BROADCAST",
                &self.to_string(),
            ),
            BroadcastError::CohostLimitExceeded(_) => error_response(
                StatusCode::FORBIDDEN,
                "COHOST_LIMIT_REACHED",
                &format!("Cohost limit exceeded. Max {} cohosts allowed", MAX_COHOSTS),
            ),
            BroadcastError::InvitationNotYours => error_response(
                StatusCode::FORBIDDEN,
                "INVITATION_NOT_YOURS",
                &self.to_string(),
            ),
            BroadcastError::NotFound => error_response(
                StatusCode::NOT_FOUND,
                "BROADCAST_NOT_FOUND",
                &self.to_string(),
            ),
            BroadcastError::UserNotFound => {
                error_response(StatusCode::NOT_FOUND, "USER_NOT_FOUND", &self.to_string())
            }
            BroadcastError::OneOrMoreUsersNotFound => error_response(
                StatusCode::NOT_FOUND,
                "ONE_OR_MORE_USERS_NOT_FOUND",
                &self.to_string(),
            ),
            BroadcastError::InvitationNotFound => error_response(
                StatusCode::NOT_FOUND,
                "INVITATION_NOT_FOUND",
                &self.to_string(),
            ),
            BroadcastError::AlreadyCohost => error_response(
                StatusCode::CONFLICT,
                "ALREADY_BROADCAST_COHOST",
                &self.to_string(),
            ),
            BroadcastError::DuplicateInvitation => error_response(
                StatusCode::CONFLICT,
                "DUPLICATE_INVITATION",
                &self.to_string(),
            ),
            BroadcastError::TooManyReconnects => error_response(
                StatusCode::TOO_MANY_REQUESTS,
                "TOO_MANY_REQUESTS",
                &self.to_string(),
            ),
            BroadcastError::LiveKitUnavailable => error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                "BROADCAST_SERVICE_UNAVAILABLE",
                &self.to_string(),
            ),
            BroadcastError::LiveKitAccess(_)
            | BroadcastError::LiveKit(_)
            | BroadcastError::Redis(_)
            | BroadcastError::Database(_)
            | BroadcastError::Internal(_) => {
                tracing::error!("{:?}", self);
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred",
                )
            }
            BroadcastError::ValidationError(errs) => validation_error_response(errs.clone()),
        }
    }
}
