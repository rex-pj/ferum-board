use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use tokio::sync::mpsc::{self, UnboundedSender};
use uuid::Uuid;

use ferum_application::ports::{NotificationBus, NotificationSubscriber};
use ferum_application::shared::AppError;

/// In-process SSE broadcaster. Holds one mpsc sender per live SSE connection.
/// Multiple tabs from the same user each get their own receiver.
pub struct SseBroadcaster {
    senders: Arc<DashMap<Uuid, Vec<UnboundedSender<String>>>>,
}

impl SseBroadcaster {
    pub fn new() -> Self {
        let senders: Arc<DashMap<Uuid, Vec<UnboundedSender<String>>>> = Arc::new(DashMap::new());

        // Background task: evict dead senders every 60s so that repeated page
        // navigation (each of which opens a new SSE connection) does not
        // accumulate closed senders and Tokio tasks indefinitely.
        let weak = Arc::downgrade(&senders);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let Some(map) = weak.upgrade() else { break };
                map.retain(|_, senders| {
                    senders.retain(|tx| !tx.is_closed());
                    !senders.is_empty()
                });
            }
        });

        Self { senders }
    }

    /// Register a new SSE connection for the user. Returns the receiver end of
    /// the channel; the caller streams it to the HTTP response.
    pub fn subscribe(&self, user_id: Uuid) -> mpsc::UnboundedReceiver<String> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut entry = self.senders.entry(user_id).or_default();
        // Evict dead senders from previous closed connections before registering the new one.
        entry.retain(|s| !s.is_closed());
        entry.push(tx);
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

    pub fn active_connection_count(&self) -> usize {
        self.senders.iter().map(|e| e.value().len()).sum()
    }
}

// ─── NotificationSubscriber implementation ────────────────────────────────────

impl NotificationSubscriber for SseBroadcaster {
    fn subscribe(&self, user_id: Uuid) -> mpsc::UnboundedReceiver<String> {
        self.subscribe(user_id)
    }

    fn active_connection_count(&self) -> usize {
        self.active_connection_count()
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
