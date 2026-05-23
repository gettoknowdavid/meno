use dashmap::DashMap;
use serde::Serialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct WsPayload {
    pub event: String,
    pub data: serde_json::Value,
}

#[derive(Clone)]
pub struct ConnectionSender {
    conn_id: usize,
    sender: mpsc::Sender<Arc<WsPayload>>,
}

#[derive(Clone)]
pub struct WsHub {
    clients: DashMap<Uuid, Vec<ConnectionSender>>,
    conn_seq: Arc<AtomicUsize>,
}

impl WsHub {
    pub fn new() -> Self {
        Self {
            clients: DashMap::new(),
            conn_seq: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn register(&self, user_id: Uuid, sender: mpsc::Sender<Arc<WsPayload>>) -> usize {
        let conn_id = self.conn_seq.fetch_add(1, Ordering::Relaxed);
        let conn_sender = ConnectionSender { conn_id, sender };
        self.clients.entry(user_id).or_default().push(conn_sender);
        conn_id
    }

    pub fn unregister(&self, user_id: Uuid, conn_id: usize) {
        if let Some(mut entry) = self.clients.get_mut(&user_id) {
            entry.retain(|e| e.conn_id != conn_id);
        }

        if self
            .clients
            .get(&user_id)
            .map(|v| v.is_empty())
            .unwrap_or(true)
        {
            self.clients.remove(&user_id);
        }
    }

    pub async fn send_to_user(&self, user_id: Uuid, payload: WsPayload) {
        let arc_payload = Arc::new(payload);
        if let Some(senders) = self.clients.get(&user_id) {
            for sender in senders.iter() {
                let _ = sender.sender.send(Arc::clone(&arc_payload)).await;
            }
        }
    }

    pub async fn send_to_users(&self, user_ids: &[Uuid], payload: WsPayload) {
        let arc = Arc::new(payload);
        for &user_id in user_ids {
            self.send_to_user(user_id, (*arc).clone()).await;
        }
    }

    pub fn connection_count(&self, user_id: Uuid) -> usize {
        self.clients.get(&user_id).map(|v| v.len()).unwrap_or(0)
    }

    pub fn is_online(&self, user_id: Uuid) -> bool {
        self.clients.contains_key(&user_id)
    }
}
