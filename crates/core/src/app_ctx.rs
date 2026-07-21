//! Shared application context — replaces Tauri AppCtx / managed State.

use std::sync::Arc;
use tokio::sync::broadcast;

use crate::db::Db;

#[derive(Clone, Debug)]
pub struct LogEvent {
    pub channel: String,
    pub message: String,
}

#[derive(Clone)]
pub struct AppCtx {
    pub db: Arc<Db>,
    log_tx: broadcast::Sender<LogEvent>,
}

impl AppCtx {
    pub fn new(db: Db) -> Self {
        let (log_tx, _) = broadcast::channel(512);
        Self {
            db: Arc::new(db),
            log_tx,
        }
    }

    pub fn subscribe_logs(&self) -> broadcast::Receiver<LogEvent> {
        self.log_tx.subscribe()
    }

    pub fn emit(&self, channel: &str, msg: &str) {
        let _ = self.log_tx.send(LogEvent {
            channel: channel.to_string(),
            message: msg.to_string(),
        });
    }

    pub fn emit_genre(&self, msg: &str) {
        self.emit("genre:log", msg);
    }

    pub fn emit_cdp(&self, msg: &str) {
        self.emit("cdp:log", msg);
    }
}
