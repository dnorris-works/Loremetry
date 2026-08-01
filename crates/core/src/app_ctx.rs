//! Shared application context — replaces Tauri AppCtx / managed State.

use std::sync::Arc;
use tokio::sync::broadcast;

use crate::db::Db;
use crate::usage::UsageRecorder;

#[derive(Clone, Debug)]
pub struct LogEvent {
    pub channel: String,
    pub message: String,
}

#[derive(Clone)]
pub struct AppCtx {
    pub db: Arc<Db>,
    pub usage: Arc<UsageRecorder>,
    log_tx: broadcast::Sender<LogEvent>,
    /// When set (web + Clerk), overrides bootstrap for `user_id()`.
    effective_user_id: Option<uuid::Uuid>,
    /// When set, log lines are also persisted to `lore.job_events`.
    active_job_id: Option<uuid::Uuid>,
}

impl AppCtx {
    pub fn new(db: Db) -> Self {
        let usage = Arc::new(UsageRecorder::new(db.pool.clone()));
        let (log_tx, _) = broadcast::channel(512);
        Self {
            db: Arc::new(db),
            usage,
            log_tx,
            effective_user_id: None,
            active_job_id: None,
        }
    }

    pub fn with_user_id(&self, user_id: uuid::Uuid) -> Self {
        let mut ctx = self.clone();
        ctx.effective_user_id = Some(user_id);
        ctx
    }

    pub fn with_job_id(&self, job_id: uuid::Uuid) -> Self {
        let mut ctx = self.clone();
        ctx.active_job_id = Some(job_id);
        ctx
    }

    /// Authenticated user id, or bootstrap admin when Clerk is not in use.
    pub fn user_id(&self) -> uuid::Uuid {
        self.effective_user_id
            .unwrap_or(self.db.bootstrap_user_id)
    }

    pub fn subscribe_logs(&self) -> broadcast::Receiver<LogEvent> {
        self.log_tx.subscribe()
    }

    pub fn emit(&self, channel: &str, msg: &str) {
        let channel_s = channel.to_string();
        let message = msg.to_string();
        let _ = self.log_tx.send(LogEvent {
            channel: channel_s.clone(),
            message: message.clone(),
        });
        if let Some(job_id) = self.active_job_id {
            let pool = self.db.pool.clone();
            tokio::spawn(async move {
                crate::jobs::insert_event(&pool, job_id, &channel_s, &message).await;
            });
        }
    }

    pub fn emit_genre(&self, msg: &str) {
        self.emit("genre:log", msg);
    }

    pub fn emit_cdp(&self, msg: &str) {
        self.emit("cdp:log", msg);
    }
}
