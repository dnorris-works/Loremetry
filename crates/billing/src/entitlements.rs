use crate::models::{AccessDecision, Tier};

/// The 5 free-tier report IDs.
pub const FREE_TIER_REPORTS: &[&str] = &[
    "zeigarnik_analysis",
    "chekhovs_gun",
    "show_dont_tell",
    "pov_discipline",
    "want_vs_need",
];

/// Series report IDs — always gated separately.
pub const SERIES_REPORTS: &[&str] = &[
    "cross_book_setup_payoff",
    "series_pacing_comparator",
    "recurring_motif_theme_series",
];

/// All known non-series report IDs.
pub const ALL_STANDARD_REPORTS: &[&str] = &[
    "show_dont_tell",
    "ai_isms",
    "story_beat_placement",
    "scene_sequel_balance",
    "pov_discipline",
    "continuity_check",
    "chekhovs_gun",
    "red_herring_vs_abandoned",
    "foreshadowing_twist_fairness",
    "macguffin_clarity",
    "timeline_flashback",
    "want_vs_need",
    "thematic_throughline",
    "mirror_foil_character",
    "zeigarnik_analysis",
    "dramatic_irony",
    "stakes_escalation",
];

/// Check whether a given tier grants access to a report.
pub fn can_run_report(tier: Tier, report_id: &str) -> AccessDecision {
    // Series reports are always denied for Free and Pro.
    if SERIES_REPORTS.contains(&report_id) {
        return AccessDecision::deny("series reports require a series entitlement");
    }

    // Unknown report IDs are denied.
    if !ALL_STANDARD_REPORTS.contains(&report_id) {
        return AccessDecision::deny("unknown report");
    }

    match tier {
        Tier::Free => {
            if FREE_TIER_REPORTS.contains(&report_id) {
                AccessDecision::allow()
            } else {
                AccessDecision::deny("upgrade required")
            }
        }
        Tier::Pro => AccessDecision::allow(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::resolve_tier_from_plan_label;

    #[test]
    fn free_tier_reports_count_is_five() {
        assert_eq!(FREE_TIER_REPORTS.len(), 5);
    }

    #[test]
    fn no_overlap_between_free_tier_and_series_reports() {
        for id in FREE_TIER_REPORTS {
            assert!(
                !SERIES_REPORTS.contains(id),
                "'{id}' appears in both FREE_TIER_REPORTS and SERIES_REPORTS"
            );
        }
    }

    #[test]
    fn resolve_tier_empty_label_returns_error() {
        assert_eq!(
            resolve_tier_from_plan_label(""),
            Err("no plan assigned")
        );
    }

    #[test]
    fn resolve_tier_free_label() {
        assert_eq!(
            resolve_tier_from_plan_label("free"),
            Ok(Tier::Free)
        );
    }

    #[test]
    fn resolve_tier_pro_label() {
        assert_eq!(
            resolve_tier_from_plan_label("pro"),
            Ok(Tier::Pro)
        );
    }

    #[test]
    fn resolve_tier_unknown_label_returns_error() {
        assert_eq!(
            resolve_tier_from_plan_label("unknown_value"),
            Err("unknown plan label")
        );
    }

    #[test]
    fn access_decision_allow_returns_correct_fields() {
        let decision = AccessDecision::allow();
        assert!(decision.allowed);
        assert_eq!(decision.reason, "allowed");
    }

    #[test]
    fn access_decision_deny_returns_correct_fields() {
        let decision = AccessDecision::deny("test");
        assert!(!decision.allowed);
        assert_eq!(decision.reason, "test");
    }
}
