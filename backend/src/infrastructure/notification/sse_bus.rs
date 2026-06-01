use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio::sync::mpsc::{self, UnboundedSender};
use uuid::Uuid;

use crate::application::ports::NotificationBus;
use crate::application::shared::AppError;

/// In-process SSE broadcaster. Holds one mpsc sender per live SSE connection.
/// Multiple tabs from the same user each get their own receiver.
pub struct SseBroadcaster {
    senders: DashMap<Uuid, Vec<UnboundedSender<String>>>,
}

impl SseBroadcaster {
    pub fn new() -> Self {
        Self { senders: DashMap::new() }
    }

    /// Register a new SSE connection for the user. Returns the receiver end of
    /// the channel; the caller streams it to the HTTP response.
    pub fn subscribe(&self, user_id: Uuid) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.senders.entry(user_id).or_default().push(tx);
        rx
    }

    /// Fan out a JSON string to all live connections for the user.
    /// Automatically removes closed connections.
    pub fn publish(&self, user_id: Uuid, data: String) {
        if let Some(mut entry) = self.senders.get_mut(&user_id) {
            entry.retain(|tx| tx.send(data.clone()).is_ok());
            if entry.is_empty() {
                drop(entry);
                self.senders.remove(&user_id);
            }
        }
    }
}

// ─── NotificationBus implementation ──────────────────────────────────────────

pub struct SseNotificationBus {
    broadcaster: Arc<SseBroadcaster>,
}

impl SseNotificationBus {
    pub fn new(broadcaster: Arc<SseBroadcaster>) -> Self {
        Self { broadcaster }
    }
}

#[async_trait]
impl NotificationBus for SseNotificationBus {
    async fn publish(&self, user_id: Uuid, payload: serde_json::Value) -> Result<(), AppError> {
        self.broadcaster.publish(user_id, payload.to_string());
        Ok(())
    }
}
