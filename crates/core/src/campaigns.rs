// campaigns.rs — Ad campaign tracking (Marketing mode)

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use crate::app_ctx::AppCtx;

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdPlatformAccount {
    pub id: i64,
    pub platform: String,
    pub account_id: String,
    pub pixel_id: String,
    pub tracking_notes: String,
    pub payment_notes: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdLandingPage {
    pub id: i64,
    #[serde(rename = "story_folder")]
    pub story_id: String,
    pub name: String,
    pub url: String,
    pub conversion_rate: Option<f64>,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdCampaign {
    pub id: i64,
    #[serde(rename = "story_folder")]
    pub story_id: String,
    pub name: String,
    pub platform: String,
    pub platform_account_id: Option<i64>,
    pub objective: String,
    pub status: String,
    pub budget: Option<f64>,
    pub budget_period: String,
    pub start_date: String,
    pub end_date: String,
    pub target_audience: String,
    pub landing_page_id: Option<i64>,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub total_spend: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdCreative {
    pub id: i64,
    pub campaign_id: i64,
    pub name: String,
    pub creative_type: String,
    pub version: String,
    pub platform_format: String,
    pub status: String,
    pub asset_path: String,
    pub body_text: String,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdPerformanceSnapshot {
    pub id: i64,
    pub campaign_id: i64,
    pub creative_id: Option<i64>,
    pub snapshot_date: String,
    pub impressions: i64,
    pub clicks: i64,
    pub conversions: i64,
    pub ctr: f64,
    pub cpc: f64,
    pub cpa: f64,
    pub spend: f64,
    pub notes: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdSpendEntry {
    pub id: i64,
    pub campaign_id: i64,
    pub platform: String,
    pub amount: f64,
    pub spent_at: String,
    pub notes: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdAudienceNote {
    pub id: i64,
    pub campaign_id: i64,
    pub label: String,
    pub demographics: String,
    pub interests: String,
    pub lookalike_notes: String,
    pub outcome: String,
    pub notes: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AdCampaignDetail {
    pub campaign: AdCampaign,
    pub creatives: Vec<AdCreative>,
    pub snapshots: Vec<AdPerformanceSnapshot>,
    pub spend_entries: Vec<AdSpendEntry>,
    pub audience_notes: Vec<AdAudienceNote>,
    pub landing_page: Option<AdLandingPage>,
}

#[derive(Serialize)]
pub struct CampaignResult {
    pub success: bool,
    pub error: String,
}

#[derive(Serialize)]
pub struct CampaignListResult {
    pub success: bool,
    pub campaigns: Vec<AdCampaign>,
    pub error: String,
}

#[derive(Serialize)]
pub struct CampaignDetailResult {
    pub success: bool,
    pub detail: Option<AdCampaignDetail>,
    pub error: String,
}

#[derive(Serialize)]
pub struct PlatformAccountsResult {
    pub success: bool,
    pub accounts: Vec<AdPlatformAccount>,
    pub error: String,
}

#[derive(Serialize)]
pub struct LandingPagesResult {
    pub success: bool,
    pub pages: Vec<AdLandingPage>,
    pub error: String,
}

#[derive(Serialize)]
pub struct CreativesResult {
    pub success: bool,
    pub creatives: Vec<AdCreative>,
    pub error: String,
}

#[derive(Serialize)]
pub struct SnapshotsResult {
    pub success: bool,
    pub snapshots: Vec<AdPerformanceSnapshot>,
    pub error: String,
}

#[derive(Serialize)]
pub struct SpendEntriesResult {
    pub success: bool,
    pub entries: Vec<AdSpendEntry>,
    pub error: String,
}

#[derive(Serialize)]
pub struct AudienceNotesResult {
    pub success: bool,
    pub notes: Vec<AdAudienceNote>,
    pub error: String,
}

#[derive(Serialize)]
pub struct IdResult {
    pub success: bool,
    pub id: i64,
    pub error: String,
}

// ── Request types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateCampaignRequest {
    pub story_folder: String,
    pub name: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub objective: String,
}

#[derive(Deserialize)]
pub struct UpdateCampaignRequest {
    pub id: i64,
    pub name: String,
    pub platform: String,
    pub platform_account_id: Option<i64>,
    pub objective: String,
    pub status: String,
    pub budget: Option<f64>,
    pub budget_period: String,
    pub start_date: String,
    pub end_date: String,
    pub target_audience: String,
    pub landing_page_id: Option<i64>,
    pub notes: String,
}

#[derive(Deserialize)]
pub struct PlatformAccountInput {
    pub platform: String,
    #[serde(default)]
    pub account_id: String,
    #[serde(default)]
    pub pixel_id: String,
    #[serde(default)]
    pub tracking_notes: String,
    #[serde(default)]
    pub payment_notes: String,
}

#[derive(Deserialize)]
pub struct UpdatePlatformAccountRequest {
    pub id: i64,
    pub platform: String,
    pub account_id: String,
    pub pixel_id: String,
    pub tracking_notes: String,
    pub payment_notes: String,
}

#[derive(Deserialize)]
pub struct LandingPageInput {
    pub story_folder: String,
    pub name: String,
    #[serde(default)]
    pub url: String,
    pub conversion_rate: Option<f64>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Deserialize)]
pub struct UpdateLandingPageRequest {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub conversion_rate: Option<f64>,
    pub notes: String,
}

#[derive(Deserialize)]
pub struct CreativeInput {
    pub campaign_id: i64,
    pub name: String,
    #[serde(default)]
    pub creative_type: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub platform_format: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub asset_path: String,
    #[serde(default)]
    pub body_text: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Deserialize)]
pub struct UpdateCreativeRequest {
    pub id: i64,
    pub name: String,
    pub creative_type: String,
    pub version: String,
    pub platform_format: String,
    pub status: String,
    pub asset_path: String,
    pub body_text: String,
    pub notes: String,
}

#[derive(Deserialize)]
pub struct SnapshotInput {
    pub campaign_id: i64,
    pub creative_id: Option<i64>,
    pub snapshot_date: String,
    #[serde(default)]
    pub impressions: i64,
    #[serde(default)]
    pub clicks: i64,
    #[serde(default)]
    pub conversions: i64,
    #[serde(default)]
    pub ctr: f64,
    #[serde(default)]
    pub cpc: f64,
    #[serde(default)]
    pub cpa: f64,
    #[serde(default)]
    pub spend: f64,
    #[serde(default)]
    pub notes: String,
}

#[derive(Deserialize)]
pub struct SpendInput {
    pub campaign_id: i64,
    #[serde(default)]
    pub platform: String,
    pub amount: f64,
    pub spent_at: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Deserialize)]
pub struct AudienceNoteInput {
    pub campaign_id: i64,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub demographics: String,
    #[serde(default)]
    pub interests: String,
    #[serde(default)]
    pub lookalike_notes: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Deserialize)]
pub struct UpdateAudienceNoteRequest {
    pub id: i64,
    pub label: String,
    pub demographics: String,
    pub interests: String,
    pub lookalike_notes: String,
    pub outcome: String,
    pub notes: String,
}

// ── Row loaders ───────────────────────────────────────────────────────────────

fn campaign_from_row(row: &sqlx::postgres::PgRow) -> AdCampaign {
    AdCampaign {
        id: row.get("id"),
        story_id: row.get("story_id"),
        name: row.get("name"),
        platform: row.get("platform"),
        platform_account_id: row.get("platform_account_id"),
        objective: row.get("objective"),
        status: row.get("status"),
        budget: row.get("budget"),
        budget_period: row.get("budget_period"),
        start_date: row.get("start_date"),
        end_date: row.get("end_date"),
        target_audience: row.get("target_audience"),
        landing_page_id: row.get("landing_page_id"),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        total_spend: 0.0,
    }
}

async fn campaign_spend_total(pool: &PgPool, campaign_id: i64) -> f64 {
    sqlx::query_scalar::<_, f64>(
        "SELECT COALESCE(SUM(amount), 0) FROM ad_spend_entries WHERE campaign_id = $1",
    )
    .bind(campaign_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0.0)
}

fn landing_page_from_row(row: &sqlx::postgres::PgRow) -> AdLandingPage {
    AdLandingPage {
        id: row.get("id"),
        story_id: row.get("story_id"),
        name: row.get("name"),
        url: row.get("url"),
        conversion_rate: row.get("conversion_rate"),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

async fn load_landing_page(pool: &PgPool, id: i64) -> Option<AdLandingPage> {
    sqlx::query(
        "SELECT id, story_id, name, url, conversion_rate, notes, created_at, updated_at \
         FROM ad_landing_pages WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(|row| landing_page_from_row(&row))
}

fn creative_from_row(row: &sqlx::postgres::PgRow) -> AdCreative {
    AdCreative {
        id: row.get("id"),
        campaign_id: row.get("campaign_id"),
        name: row.get("name"),
        creative_type: row.get("creative_type"),
        version: row.get("version"),
        platform_format: row.get("platform_format"),
        status: row.get("status"),
        asset_path: row.get("asset_path"),
        body_text: row.get("body_text"),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn snapshot_from_row(row: &sqlx::postgres::PgRow) -> AdPerformanceSnapshot {
    AdPerformanceSnapshot {
        id: row.get("id"),
        campaign_id: row.get("campaign_id"),
        creative_id: row.get("creative_id"),
        snapshot_date: row.get("snapshot_date"),
        impressions: row.get("impressions"),
        clicks: row.get("clicks"),
        conversions: row.get("conversions"),
        ctr: row.get("ctr"),
        cpc: row.get("cpc"),
        cpa: row.get("cpa"),
        spend: row.get("spend"),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
    }
}

fn spend_entry_from_row(row: &sqlx::postgres::PgRow) -> AdSpendEntry {
    AdSpendEntry {
        id: row.get("id"),
        campaign_id: row.get("campaign_id"),
        platform: row.get("platform"),
        amount: row.get("amount"),
        spent_at: row.get("spent_at"),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
    }
}

fn audience_note_from_row(row: &sqlx::postgres::PgRow) -> AdAudienceNote {
    AdAudienceNote {
        id: row.get("id"),
        campaign_id: row.get("campaign_id"),
        label: row.get("label"),
        demographics: row.get("demographics"),
        interests: row.get("interests"),
        lookalike_notes: row.get("lookalike_notes"),
        outcome: row.get("outcome"),
        notes: row.get("notes"),
        created_at: row.get("created_at"),
    }
}

fn platform_account_from_row(row: &sqlx::postgres::PgRow) -> AdPlatformAccount {
    AdPlatformAccount {
        id: row.get("id"),
        platform: row.get("platform"),
        account_id: row.get("account_id"),
        pixel_id: row.get("pixel_id"),
        tracking_notes: row.get("tracking_notes"),
        payment_notes: row.get("payment_notes"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

async fn load_creatives_inner(pool: &PgPool, campaign_id: i64) -> Vec<AdCreative> {
    sqlx::query(
        "SELECT id, campaign_id, name, creative_type, version, platform_format, status, \
         asset_path, body_text, notes, created_at, updated_at \
         FROM ad_creatives WHERE campaign_id = $1 ORDER BY created_at",
    )
    .bind(campaign_id)
    .fetch_all(pool)
    .await
    .map(|rows| rows.iter().map(creative_from_row).collect())
    .unwrap_or_default()
}

async fn load_snapshots_inner(pool: &PgPool, campaign_id: i64) -> Vec<AdPerformanceSnapshot> {
    sqlx::query(
        "SELECT id, campaign_id, creative_id, snapshot_date, impressions, clicks, conversions, \
         ctr, cpc, cpa, spend, notes, created_at \
         FROM ad_performance_snapshots WHERE campaign_id = $1 ORDER BY snapshot_date DESC",
    )
    .bind(campaign_id)
    .fetch_all(pool)
    .await
    .map(|rows| rows.iter().map(snapshot_from_row).collect())
    .unwrap_or_default()
}

async fn load_spend_inner(pool: &PgPool, campaign_id: i64) -> Vec<AdSpendEntry> {
    sqlx::query(
        "SELECT id, campaign_id, platform, amount, spent_at, notes, created_at \
         FROM ad_spend_entries WHERE campaign_id = $1 ORDER BY spent_at DESC",
    )
    .bind(campaign_id)
    .fetch_all(pool)
    .await
    .map(|rows| rows.iter().map(spend_entry_from_row).collect())
    .unwrap_or_default()
}

async fn load_audience_inner(pool: &PgPool, campaign_id: i64) -> Vec<AdAudienceNote> {
    sqlx::query(
        "SELECT id, campaign_id, label, demographics, interests, lookalike_notes, outcome, notes, created_at \
         FROM ad_audience_notes WHERE campaign_id = $1 ORDER BY created_at DESC",
    )
    .bind(campaign_id)
    .fetch_all(pool)
    .await
    .map(|rows| rows.iter().map(audience_note_from_row).collect())
    .unwrap_or_default()
}

// ── Campaign commands ─────────────────────────────────────────────────────────

pub async fn list_campaigns(app: AppCtx, story_id: String) -> CampaignListResult {
    let pool = &app.db.pool;
    let rows = match sqlx::query(
        "SELECT id, story_id, name, platform, platform_account_id, objective, status, \
         budget, budget_period, start_date, end_date, target_audience, landing_page_id, \
         notes, created_at, updated_at \
         FROM ad_campaigns WHERE story_id = $1 ORDER BY updated_at DESC",
    )
    .bind(&story_id)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return CampaignListResult {
                success: false,
                campaigns: Vec::new(),
                error: e.to_string(),
            };
        }
    };

    let mut campaigns: Vec<AdCampaign> = rows.iter().map(campaign_from_row).collect();
    for c in &mut campaigns {
        c.total_spend = campaign_spend_total(pool, c.id).await;
    }

    CampaignListResult {
        success: true,
        campaigns,
        error: String::new(),
    }
}

pub async fn create_campaign(app: AppCtx, request: CreateCampaignRequest) -> IdResult {
    let name = request.name.trim();
    if name.is_empty() {
        return IdResult {
            success: false,
            id: 0,
            error: "Campaign name is required.".to_string(),
        };
    }
    let pool = &app.db.pool;
    let ts = now();
    match sqlx::query_scalar::<_, i64>(
        "INSERT INTO ad_campaigns (story_id, name, platform, objective, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(&request.story_folder)
    .bind(name)
    .bind(&request.platform)
    .bind(&request.objective)
    .bind(&ts)
    .bind(&ts)
    .fetch_one(pool)
    .await
    {
        Ok(id) => IdResult {
            success: true,
            id,
            error: String::new(),
        },
        Err(e) => IdResult {
            success: false,
            id: 0,
            error: e.to_string(),
        },
    }
}

pub async fn update_campaign(app: AppCtx, request: UpdateCampaignRequest) -> CampaignResult {
    let pool = &app.db.pool;
    match sqlx::query(
        "UPDATE ad_campaigns SET name = $1, platform = $2, platform_account_id = $3, \
         objective = $4, status = $5, budget = $6, budget_period = $7, start_date = $8, \
         end_date = $9, target_audience = $10, landing_page_id = $11, notes = $12, \
         updated_at = $13 WHERE id = $14",
    )
    .bind(request.name.trim())
    .bind(&request.platform)
    .bind(request.platform_account_id)
    .bind(&request.objective)
    .bind(&request.status)
    .bind(request.budget)
    .bind(&request.budget_period)
    .bind(&request.start_date)
    .bind(&request.end_date)
    .bind(&request.target_audience)
    .bind(request.landing_page_id)
    .bind(&request.notes)
    .bind(now())
    .bind(request.id)
    .execute(pool)
    .await
    {
        Ok(_) => CampaignResult {
            success: true,
            error: String::new(),
        },
        Err(e) => CampaignResult {
            success: false,
            error: e.to_string(),
        },
    }
}

pub async fn delete_campaign(app: AppCtx, id: i64) -> CampaignResult {
    let pool = &app.db.pool;
    match sqlx::query("DELETE FROM ad_campaigns WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
    {
        Ok(_) => CampaignResult {
            success: true,
            error: String::new(),
        },
        Err(e) => CampaignResult {
            success: false,
            error: e.to_string(),
        },
    }
}

pub async fn get_campaign_detail(app: AppCtx, id: i64) -> CampaignDetailResult {
    let pool = &app.db.pool;
    let row = match sqlx::query(
        "SELECT id, story_id, name, platform, platform_account_id, objective, status, \
         budget, budget_period, start_date, end_date, target_audience, landing_page_id, \
         notes, created_at, updated_at FROM ad_campaigns WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return CampaignDetailResult {
                success: false,
                detail: None,
                error: e.to_string(),
            };
        }
    };

    let mut campaign = match row {
        Some(r) => campaign_from_row(&r),
        None => {
            return CampaignDetailResult {
                success: false,
                detail: None,
                error: "Campaign not found".to_string(),
            };
        }
    };
    campaign.total_spend = campaign_spend_total(pool, id).await;

    let creatives = load_creatives_inner(pool, id).await;
    let snapshots = load_snapshots_inner(pool, id).await;
    let spend_entries = load_spend_inner(pool, id).await;
    let audience_notes = load_audience_inner(pool, id).await;
    let landing_page = match campaign.landing_page_id {
        Some(lp_id) => load_landing_page(pool, lp_id).await,
        None => None,
    };

    CampaignDetailResult {
        success: true,
        detail: Some(AdCampaignDetail {
            campaign,
            creatives,
            snapshots,
            spend_entries,
            audience_notes,
            landing_page,
        }),
        error: String::new(),
    }
}

// ── Creatives ─────────────────────────────────────────────────────────────────

pub async fn list_creatives(app: AppCtx, campaign_id: i64) -> CreativesResult {
    let pool = &app.db.pool;
    CreativesResult {
        success: true,
        creatives: load_creatives_inner(pool, campaign_id).await,
        error: String::new(),
    }
}

pub async fn create_creative(app: AppCtx, request: CreativeInput) -> IdResult {
    let pool = &app.db.pool;
    let ts = now();
    match sqlx::query_scalar::<_, i64>(
        "INSERT INTO ad_creatives (campaign_id, name, creative_type, version, platform_format, \
         status, asset_path, body_text, notes, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
    )
    .bind(request.campaign_id)
    .bind(&request.name)
    .bind(&request.creative_type)
    .bind(&request.version)
    .bind(&request.platform_format)
    .bind(&request.status)
    .bind(&request.asset_path)
    .bind(&request.body_text)
    .bind(&request.notes)
    .bind(&ts)
    .bind(&ts)
    .fetch_one(pool)
    .await
    {
        Ok(id) => IdResult {
            success: true,
            id,
            error: String::new(),
        },
        Err(e) => IdResult {
            success: false,
            id: 0,
            error: e.to_string(),
        },
    }
}

pub async fn update_creative(app: AppCtx, request: UpdateCreativeRequest) -> CampaignResult {
    let pool = &app.db.pool;
    match sqlx::query(
        "UPDATE ad_creatives SET name = $1, creative_type = $2, version = $3, \
         platform_format = $4, status = $5, asset_path = $6, body_text = $7, notes = $8, \
         updated_at = $9 WHERE id = $10",
    )
    .bind(&request.name)
    .bind(&request.creative_type)
    .bind(&request.version)
    .bind(&request.platform_format)
    .bind(&request.status)
    .bind(&request.asset_path)
    .bind(&request.body_text)
    .bind(&request.notes)
    .bind(now())
    .bind(request.id)
    .execute(pool)
    .await
    {
        Ok(_) => CampaignResult {
            success: true,
            error: String::new(),
        },
        Err(e) => CampaignResult {
            success: false,
            error: e.to_string(),
        },
    }
}

pub async fn delete_creative(app: AppCtx, id: i64) -> CampaignResult {
    let pool = &app.db.pool;
    match sqlx::query("DELETE FROM ad_creatives WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
    {
        Ok(_) => CampaignResult {
            success: true,
            error: String::new(),
        },
        Err(e) => CampaignResult {
            success: false,
            error: e.to_string(),
        },
    }
}

// ── Performance snapshots ─────────────────────────────────────────────────────

pub async fn list_performance_snapshots(app: AppCtx, campaign_id: i64) -> SnapshotsResult {
    let pool = &app.db.pool;
    SnapshotsResult {
        success: true,
        snapshots: load_snapshots_inner(pool, campaign_id).await,
        error: String::new(),
    }
}

pub async fn add_performance_snapshot(app: AppCtx, request: SnapshotInput) -> IdResult {
    let pool = &app.db.pool;
    match sqlx::query_scalar::<_, i64>(
        "INSERT INTO ad_performance_snapshots (campaign_id, creative_id, snapshot_date, \
         impressions, clicks, conversions, ctr, cpc, cpa, spend, notes, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING id",
    )
    .bind(request.campaign_id)
    .bind(request.creative_id)
    .bind(&request.snapshot_date)
    .bind(request.impressions)
    .bind(request.clicks)
    .bind(request.conversions)
    .bind(request.ctr)
    .bind(request.cpc)
    .bind(request.cpa)
    .bind(request.spend)
    .bind(&request.notes)
    .bind(now())
    .fetch_one(pool)
    .await
    {
        Ok(id) => IdResult {
            success: true,
            id,
            error: String::new(),
        },
        Err(e) => IdResult {
            success: false,
            id: 0,
            error: e.to_string(),
        },
    }
}

pub async fn delete_performance_snapshot(app: AppCtx, id: i64) -> CampaignResult {
    let pool = &app.db.pool;
    match sqlx::query("DELETE FROM ad_performance_snapshots WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
    {
        Ok(_) => CampaignResult {
            success: true,
            error: String::new(),
        },
        Err(e) => CampaignResult {
            success: false,
            error: e.to_string(),
        },
    }
}

// ── Spend entries ─────────────────────────────────────────────────────────────

pub async fn list_spend_entries(app: AppCtx, campaign_id: i64) -> SpendEntriesResult {
    let pool = &app.db.pool;
    SpendEntriesResult {
        success: true,
        entries: load_spend_inner(pool, campaign_id).await,
        error: String::new(),
    }
}

pub async fn add_spend_entry(app: AppCtx, request: SpendInput) -> IdResult {
    let pool = &app.db.pool;
    match sqlx::query_scalar::<_, i64>(
        "INSERT INTO ad_spend_entries (campaign_id, platform, amount, spent_at, notes, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(request.campaign_id)
    .bind(&request.platform)
    .bind(request.amount)
    .bind(&request.spent_at)
    .bind(&request.notes)
    .bind(now())
    .fetch_one(pool)
    .await
    {
        Ok(id) => IdResult {
            success: true,
            id,
            error: String::new(),
        },
        Err(e) => IdResult {
            success: false,
            id: 0,
            error: e.to_string(),
        },
    }
}

pub async fn delete_spend_entry(app: AppCtx, id: i64) -> CampaignResult {
    let pool = &app.db.pool;
    match sqlx::query("DELETE FROM ad_spend_entries WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
    {
        Ok(_) => CampaignResult {
            success: true,
            error: String::new(),
        },
        Err(e) => CampaignResult {
            success: false,
            error: e.to_string(),
        },
    }
}

// ── Landing pages ─────────────────────────────────────────────────────────────

pub async fn list_landing_pages(app: AppCtx, story_id: String) -> LandingPagesResult {
    let pool = &app.db.pool;
    match sqlx::query(
        "SELECT id, story_id, name, url, conversion_rate, notes, created_at, updated_at \
         FROM ad_landing_pages WHERE story_id = $1 ORDER BY name",
    )
    .bind(&story_id)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => LandingPagesResult {
            success: true,
            pages: rows.iter().map(landing_page_from_row).collect(),
            error: String::new(),
        },
        Err(e) => LandingPagesResult {
            success: false,
            pages: Vec::new(),
            error: e.to_string(),
        },
    }
}

pub async fn create_landing_page(app: AppCtx, request: LandingPageInput) -> IdResult {
    let pool = &app.db.pool;
    let ts = now();
    match sqlx::query_scalar::<_, i64>(
        "INSERT INTO ad_landing_pages (story_id, name, url, conversion_rate, notes, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(&request.story_folder)
    .bind(&request.name)
    .bind(&request.url)
    .bind(request.conversion_rate)
    .bind(&request.notes)
    .bind(&ts)
    .bind(&ts)
    .fetch_one(pool)
    .await
    {
        Ok(id) => IdResult {
            success: true,
            id,
            error: String::new(),
        },
        Err(e) => IdResult {
            success: false,
            id: 0,
            error: e.to_string(),
        },
    }
}

pub async fn update_landing_page(app: AppCtx, request: UpdateLandingPageRequest) -> CampaignResult {
    let pool = &app.db.pool;
    match sqlx::query(
        "UPDATE ad_landing_pages SET name = $1, url = $2, conversion_rate = $3, notes = $4, \
         updated_at = $5 WHERE id = $6",
    )
    .bind(&request.name)
    .bind(&request.url)
    .bind(request.conversion_rate)
    .bind(&request.notes)
    .bind(now())
    .bind(request.id)
    .execute(pool)
    .await
    {
        Ok(_) => CampaignResult {
            success: true,
            error: String::new(),
        },
        Err(e) => CampaignResult {
            success: false,
            error: e.to_string(),
        },
    }
}

pub async fn delete_landing_page(app: AppCtx, id: i64) -> CampaignResult {
    let pool = &app.db.pool;
    let _ = sqlx::query("UPDATE ad_campaigns SET landing_page_id = NULL WHERE landing_page_id = $1")
        .bind(id)
        .execute(pool)
        .await;
    match sqlx::query("DELETE FROM ad_landing_pages WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
    {
        Ok(_) => CampaignResult {
            success: true,
            error: String::new(),
        },
        Err(e) => CampaignResult {
            success: false,
            error: e.to_string(),
        },
    }
}

// ── Audience notes ────────────────────────────────────────────────────────────

pub async fn list_audience_notes(app: AppCtx, campaign_id: i64) -> AudienceNotesResult {
    let pool = &app.db.pool;
    AudienceNotesResult {
        success: true,
        notes: load_audience_inner(pool, campaign_id).await,
        error: String::new(),
    }
}

pub async fn add_audience_note(app: AppCtx, request: AudienceNoteInput) -> IdResult {
    let pool = &app.db.pool;
    match sqlx::query_scalar::<_, i64>(
        "INSERT INTO ad_audience_notes (campaign_id, label, demographics, interests, \
         lookalike_notes, outcome, notes, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(request.campaign_id)
    .bind(&request.label)
    .bind(&request.demographics)
    .bind(&request.interests)
    .bind(&request.lookalike_notes)
    .bind(&request.outcome)
    .bind(&request.notes)
    .bind(now())
    .fetch_one(pool)
    .await
    {
        Ok(id) => IdResult {
            success: true,
            id,
            error: String::new(),
        },
        Err(e) => IdResult {
            success: false,
            id: 0,
            error: e.to_string(),
        },
    }
}

pub async fn update_audience_note(app: AppCtx, request: UpdateAudienceNoteRequest) -> CampaignResult {
    let pool = &app.db.pool;
    match sqlx::query(
        "UPDATE ad_audience_notes SET label = $1, demographics = $2, interests = $3, \
         lookalike_notes = $4, outcome = $5, notes = $6 WHERE id = $7",
    )
    .bind(&request.label)
    .bind(&request.demographics)
    .bind(&request.interests)
    .bind(&request.lookalike_notes)
    .bind(&request.outcome)
    .bind(&request.notes)
    .bind(request.id)
    .execute(pool)
    .await
    {
        Ok(_) => CampaignResult {
            success: true,
            error: String::new(),
        },
        Err(e) => CampaignResult {
            success: false,
            error: e.to_string(),
        },
    }
}

pub async fn delete_audience_note(app: AppCtx, id: i64) -> CampaignResult {
    let pool = &app.db.pool;
    match sqlx::query("DELETE FROM ad_audience_notes WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
    {
        Ok(_) => CampaignResult {
            success: true,
            error: String::new(),
        },
        Err(e) => CampaignResult {
            success: false,
            error: e.to_string(),
        },
    }
}

// ── Platform accounts ─────────────────────────────────────────────────────────

pub async fn list_platform_accounts(app: AppCtx) -> PlatformAccountsResult {
    let pool = &app.db.pool;
    match sqlx::query(
        "SELECT id, platform, account_id, pixel_id, tracking_notes, payment_notes, \
         created_at, updated_at FROM ad_platform_accounts ORDER BY platform, account_id",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => PlatformAccountsResult {
            success: true,
            accounts: rows.iter().map(platform_account_from_row).collect(),
            error: String::new(),
        },
        Err(e) => PlatformAccountsResult {
            success: false,
            accounts: Vec::new(),
            error: e.to_string(),
        },
    }
}

pub async fn create_platform_account(app: AppCtx, request: PlatformAccountInput) -> IdResult {
    let pool = &app.db.pool;
    let ts = now();
    match sqlx::query_scalar::<_, i64>(
        "INSERT INTO ad_platform_accounts (platform, account_id, pixel_id, tracking_notes, \
         payment_notes, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(&request.platform)
    .bind(&request.account_id)
    .bind(&request.pixel_id)
    .bind(&request.tracking_notes)
    .bind(&request.payment_notes)
    .bind(&ts)
    .bind(&ts)
    .fetch_one(pool)
    .await
    {
        Ok(id) => IdResult {
            success: true,
            id,
            error: String::new(),
        },
        Err(e) => IdResult {
            success: false,
            id: 0,
            error: e.to_string(),
        },
    }
}

pub async fn update_platform_account(
    app: AppCtx,
    request: UpdatePlatformAccountRequest,
) -> CampaignResult {
    let pool = &app.db.pool;
    match sqlx::query(
        "UPDATE ad_platform_accounts SET platform = $1, account_id = $2, pixel_id = $3, \
         tracking_notes = $4, payment_notes = $5, updated_at = $6 WHERE id = $7",
    )
    .bind(&request.platform)
    .bind(&request.account_id)
    .bind(&request.pixel_id)
    .bind(&request.tracking_notes)
    .bind(&request.payment_notes)
    .bind(now())
    .bind(request.id)
    .execute(pool)
    .await
    {
        Ok(_) => CampaignResult {
            success: true,
            error: String::new(),
        },
        Err(e) => CampaignResult {
            success: false,
            error: e.to_string(),
        },
    }
}

pub async fn delete_platform_account(app: AppCtx, id: i64) -> CampaignResult {
    let pool = &app.db.pool;
    let _ = sqlx::query(
        "UPDATE ad_campaigns SET platform_account_id = NULL WHERE platform_account_id = $1",
    )
    .bind(id)
    .execute(pool)
    .await;
    match sqlx::query("DELETE FROM ad_platform_accounts WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
    {
        Ok(_) => CampaignResult {
            success: true,
            error: String::new(),
        },
        Err(e) => CampaignResult {
            success: false,
            error: e.to_string(),
        },
    }
}
