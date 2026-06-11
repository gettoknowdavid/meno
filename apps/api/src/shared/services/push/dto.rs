use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct GoogleTokenResponse {
    pub access_token: String,
    pub expires_in: u64,
}

/// FCM v1 message envelope.
#[derive(Debug, Serialize)]
pub struct FcmEnvelope {
    pub message: FcmMessage,
}

#[derive(Debug, Serialize)]
pub struct FcmMessage {
    /// Device registration token (absent for topic/condition sends).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    pub notification: FcmNotification,

    /// Arbitrary string key-value pairs passed through to the client app.
    pub data: HashMap<String, String>,

    pub android: FcmAndroidConfig,

    pub apns: FcmApnsConfig,
}

#[derive(Debug, Serialize)]
pub struct FcmNotification {
    pub title: String,

    pub body: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FcmAndroidConfig {
    /// "high" ensures delivery even in Doze mode.
    pub priority: String,

    pub notification: FcmAndroidNotification,
}

#[derive(Debug, Serialize)]
pub struct FcmAndroidNotification {
    /// Android notification channel ID (defined in the Flutter app).
    pub channel_id: String,

    /// Intent action for deep-link handling.
    pub click_action: String,
}

#[derive(Debug, Serialize)]
pub struct FcmApnsConfig {
    pub headers: HashMap<String, String>,
    pub payload: FcmApnsPayload,
}

#[derive(Debug, Serialize)]
pub struct FcmApnsPayload {
    pub aps: FcmApnsAps,
}

#[derive(Debug, Serialize)]
pub struct FcmApnsAps {
    pub alert: FcmApnsAlert,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<i64>,

    pub sound: String,

    #[serde(rename = "content-available")]
    pub content_available: i32,
}

#[derive(Debug, Serialize)]
pub struct FcmApnsAlert {
    pub title: String,
    pub body: String,
}
