use crate::models::Tier;

/// Map a `plan_label` database value to a Tier.
///
/// Returns `Ok(Tier)` for known labels, `Err` with a message for unknown/empty.
pub fn resolve_tier_from_plan_label(plan_label: &str) -> Result<Tier, &'static str> {
    match plan_label.trim() {
        "" => Err("no plan assigned"),
        "free" => Ok(Tier::Free),
        "pro" => Ok(Tier::Pro),
        _ => Err("unknown plan label"),
    }
}
