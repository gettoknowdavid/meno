//! Where a broadcast's notification rows are produced.
//!
//! This is the write side of §2's notification flows: `live_broadcast_started` fans out
//! over the creator's followers, `broadcast_ended` goes to the room's recipients, and
//! `added_as_cohost` tells the cohost directly. The inbox itself belongs to
//! [`crate::modules::notifications`]; this module only says *when* a sentence is owed.
//!
//! # Why a port
//!
//! The service suite runs with no Postgres and no notification module, and every
//! lifecycle flow still has to be assertable there. [`SilentNotices`] is the deployment
//! with notifications absent; the production adapter in `state.rs` holds the
//! notification service and the follow graph.
//!
//! None of these fail the flow. A broadcast that went live and whose announcement
//! could not be stored is a missing inbox row, not a broadcast that did not start.

use async_trait::async_trait;
use uuid::Uuid;

use super::dto::BroadcastResponse;
use super::model::UserSummary;

/// Announces the broadcasts that produce a notification row.
#[async_trait]
pub trait BroadcastNotices: Send + Sync + 'static {
    /// The creator went live: notify everyone following them.
    async fn started(&self, broadcast: &BroadcastResponse);

    /// The broadcast ended: tell the accounts `recipients` names.
    async fn ended(&self, broadcast_id: Uuid, title: &str, recipients: &[Uuid]);

    /// `cohost` was added to `broadcast_id` by `actor`.
    async fn cohost_added(
        &self,
        broadcast_id: Uuid,
        title: &str,
        actor: &UserSummary,
        cohost: Uuid,
    );
}

/// A deployment with no notification module.
#[derive(Debug, Clone, Copy, Default)]
pub struct SilentNotices;

#[async_trait]
impl BroadcastNotices for SilentNotices {
    async fn started(&self, _broadcast: &BroadcastResponse) {}

    async fn ended(&self, _broadcast_id: Uuid, _title: &str, _recipients: &[Uuid]) {}

    async fn cohost_added(
        &self,
        _broadcast_id: Uuid,
        _title: &str,
        _actor: &UserSummary,
        _cohost: Uuid,
    ) {
    }
}
