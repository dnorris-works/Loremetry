//! Per-user AI usage events and cost recording.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default)]
pub struct LlmUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Clone, Debug)]
pub struct UsageRecorder {
    pool: PgPool,
}

impl UsageRecorder {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn record_llm(
        &self,
        user_id: Uuid,
        provider: &str,
        model: &str,
        feature: &str,
        story_id: Option<&str>,
        usage: LlmUsage,
    ) -> Result<(), String> {
        let (input_price, output_price) = model_prices(&self.pool, provider, model).await?;
        let cost = (usage.input_tokens as f64 / 1000.0) * input_price
            + (usage.output_tokens as f64 / 1000.0) * output_price;
        let mut metadata = serde_json::json!({});
        if input_price == 0.0 && output_price == 0.0 && !model.is_empty() {
            metadata["pricing_missing"] = true.into();
        }
        self.insert(
            user_id,
            "llm",
            provider,
            model,
            feature,
            story_id,
            usage.input_tokens as i32,
            usage.output_tokens as i32,
            cost,
            metadata,
        )
        .await
    }

    pub async fn record_external(
        &self,
        user_id: Uuid,
        kind: &str,
        provider: &str,
        feature: &str,
        story_id: Option<&str>,
    ) -> Result<(), String> {
        self.insert(user_id, kind, provider, "", feature, story_id, 0, 0, 0.0, serde_json::json!({}))
            .await
    }

    async fn insert(
        &self,
        user_id: Uuid,
        kind: &str,
        provider: &str,
        model: &str,
        feature: &str,
        story_id: Option<&str>,
        input_tokens: i32,
        output_tokens: i32,
        cost_usd: f64,
        metadata: serde_json::Value,
    ) -> Result<(), String> {
        sqlx::query(
            "INSERT INTO ai_usage_events
                (user_id, kind, provider, model, feature, story_id, input_tokens, output_tokens, cost_usd, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(user_id)
        .bind(kind)
        .bind(provider)
        .bind(model)
        .bind(feature)
        .bind(story_id)
        .bind(input_tokens)
        .bind(output_tokens)
        .bind(cost_usd)
        .bind(metadata)
        .execute(&self.pool)
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

async fn model_prices(pool: &PgPool, provider: &str, model: &str) -> Result<(f64, f64), String> {
    if model.is_empty() {
        return Ok((0.0, 0.0));
    }
    let row: Option<(Option<f64>, Option<f64>)> = sqlx::query_as(
        "SELECT input_price, output_price FROM provider_models WHERE id = $1 AND provider = $2",
    )
    .bind(model)
    .bind(provider)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(match row {
        Some((i, o)) => (i.unwrap_or(0.0), o.unwrap_or(0.0)),
        None => (0.0, 0.0),
    })
}

#[derive(Debug, Serialize)]
pub struct UsageSummaryRow {
    pub user_id: Uuid,
    pub email: String,
    pub role: String,
    pub monthly_fee_cents: i32,
    pub total_cost_usd: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

pub async fn usage_summary(
    pool: &PgPool,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<UsageSummaryRow>, String> {
    let rows: Vec<(Uuid, String, String, i32, Option<f64>, Option<i64>, Option<i64>)> =
        sqlx::query_as(
            "SELECT u.id, u.email, u.role, u.monthly_fee_cents,
                    COALESCE(SUM(e.cost_usd), 0),
                    COALESCE(SUM(e.input_tokens), 0)::bigint,
                    COALESCE(SUM(e.output_tokens), 0)::bigint
             FROM users u
             LEFT JOIN ai_usage_events e ON e.user_id = u.id
                AND e.occurred_at >= $1 AND e.occurred_at < $2
             GROUP BY u.id, u.email, u.role, u.monthly_fee_cents
             ORDER BY u.email",
        )
        .bind(from)
        .bind(to)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(
            |(user_id, email, role, monthly_fee_cents, cost, inp, out)| UsageSummaryRow {
                user_id,
                email,
                role,
                monthly_fee_cents,
                total_cost_usd: cost.unwrap_or(0.0),
                input_tokens: inp.unwrap_or(0),
                output_tokens: out.unwrap_or(0),
            },
        )
        .collect())
}

#[derive(Debug, Serialize)]
pub struct UsageEventRow {
    pub id: i64,
    pub occurred_at: DateTime<Utc>,
    pub kind: String,
    pub provider: String,
    pub model: String,
    pub feature: String,
    pub story_id: Option<String>,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub cost_usd: f64,
}

pub async fn usage_events(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
) -> Result<Vec<UsageEventRow>, String> {
    let rows: Vec<(
        i64,
        DateTime<Utc>,
        String,
        String,
        String,
        String,
        Option<String>,
        i32,
        i32,
        f64,
    )> = sqlx::query_as(
        "SELECT id, occurred_at, kind, provider, model, feature, story_id,
                input_tokens, output_tokens, cost_usd
         FROM ai_usage_events
         WHERE user_id = $1
         ORDER BY occurred_at DESC
         LIMIT $2",
    )
    .bind(user_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .map(
            |(id, occurred_at, kind, provider, model, feature, story_id, input_tokens, output_tokens, cost_usd)| {
                UsageEventRow {
                    id,
                    occurred_at,
                    kind,
                    provider,
                    model,
                    feature,
                    story_id,
                    input_tokens,
                    output_tokens,
                    cost_usd,
                }
            },
        )
        .collect())
}
