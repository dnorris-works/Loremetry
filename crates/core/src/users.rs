//! Application users (synced from Clerk).

use sqlx::PgPool;
use uuid::Uuid;

/// Upsert a user row from Clerk identity; returns `(id, role)` (`subscriber` only).
pub async fn upsert_clerk_user(
    pool: &PgPool,
    clerk_id: &str,
    email: &str,
) -> Result<(Uuid, String), String> {
    let role = "subscriber";
    if clerk_id.trim().is_empty() {
        return Err("missing clerk user id".into());
    }
    let email = {
        let e = email.trim();
        if e.is_empty() {
            format!("{clerk_id}@clerk.local")
        } else {
            e.to_string()
        }
    };

    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (clerk_id, email, role, plan_label, monthly_fee_cents)
         VALUES ($1, $2, $3, '', 0)
         ON CONFLICT (clerk_id) DO UPDATE SET
            email = EXCLUDED.email
         RETURNING id",
    )
    .bind(clerk_id)
    .bind(&email)
    .bind(role)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok((id, role.to_string()))
}

pub async fn email_by_id(pool: &PgPool, id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

pub async fn theme_preference_by_id(pool: &PgPool, id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT theme_preference FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

pub async fn plan_label_by_id(pool: &PgPool, id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT plan_label FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

pub async fn set_theme_preference(pool: &PgPool, id: Uuid, theme: &str) -> Result<(), String> {
    if theme != "light" && theme != "dark" {
        return Err("theme must be light or dark".into());
    }
    sqlx::query("UPDATE users SET theme_preference = $2 WHERE id = $1")
        .bind(id)
        .bind(theme)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
