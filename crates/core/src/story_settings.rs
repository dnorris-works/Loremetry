//! Story-level settings helpers (chapter summaries, archived reports).

use crate::analysis::{chapters, FolderRequest};
use crate::app_ctx::AppCtx;
use crate::db;

pub async fn get_story_artifact_state(app: AppCtx, story_id: String) -> Result<db::StoryArtifactStateResponse, String> {
    db::get_story_artifact_state(&app.db, &story_id).await
}

pub async fn refresh_chapter_summaries(app: AppCtx, request: FolderRequest) -> Result<String, String> {
    let result = chapters::generate_summaries(app, request).await;
    if result.success {
        Ok(result.report)
    } else {
        Err(result.error)
    }
}

pub async fn clear_chapter_summaries(app: AppCtx, story_id: String) -> Result<(), String> {
    if !crate::stories::story_exists(&app.db, &story_id).await {
        return Err("Story not found.".into());
    }
    db::delete_chapter_summaries(&app.db.pool, &story_id).await?;
    db::mark_artifacts_stale(&app.db.pool, &story_id).await?;
    db::archive_all_current_reports(&app.db.pool, &story_id, "summaries_cleared").await?;
    Ok(())
}

pub async fn get_archived_reports(app: AppCtx, story_id: String) -> Result<Vec<db::ArchivedReportRow>, String> {
    db::list_archived_reports(&app.db, &story_id).await
}
