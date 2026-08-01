//! Loremetry core library — analysis, DB, market intel.

pub mod analysis;
pub mod batch_prompt;
pub mod craft_report_groups;
pub mod app_ctx;
pub mod cancel;
pub mod canopy;
pub mod campaigns;
pub mod commands;
pub mod competition_analyzer;
pub mod config;
pub mod dataforseo;
pub mod db;
pub mod assets;
pub mod asset_export;
pub mod docx;
pub mod documents;
pub mod genre_taxonomy;
pub mod models;
pub mod platform_secrets;
pub mod prompts;
pub mod secrets;
pub mod series;
pub mod stories;
pub mod story_settings;
pub mod users;
pub mod usage;
pub mod winningcat;
pub mod jobs;
pub mod manuscript_fingerprint;

pub use app_ctx::{AppCtx, LogEvent};
pub use cancel::{cancel_operation, is_cancelled, notify as cancel_notify, reset as reset_cancel};
pub use config::Config;
