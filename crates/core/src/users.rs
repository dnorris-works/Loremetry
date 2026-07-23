//! Application users (synced from Clerk).

use sqlx::PgPool;
use uuid::Uuid;

/// Map Clerk role claim to DB `users.role` (`admin` | `subscriber`).
pub fn role_from_clerk_claim(claim: &str) -> &'static str {
    if claim.eq_ignore_ascii_case("admin") {
        "admin"
    } else {
        "subscriber"
    }
}

/// Upsert a user row from Clerk identity; returns `(id, role)`.
pub async fn upsert_clerk_user(
    pool: &PgPool,
    clerk_id: &str,
    email: &str,
    clerk_role: &str,
) -> Result<(Uuid, String), String> {
    let role = role_from_clerk_claim(clerk_role);
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
            email = EXCLUDED.email,
            role = EXCLUDED.role
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

#[cfg(test)]
mod tests {
    use super::role_from_clerk_claim;

    #[test]
    fn admin_role_claim() {
        assert_eq!(role_from_clerk_claim("admin"), "admin");
        assert_eq!(role_from_clerk_claim("Admin"), "admin");
        assert_eq!(role_from_clerk_claim("subscriber"), "subscriber");
        assert_eq!(role_from_clerk_claim("user"), "subscriber");
    }
}
