// Feature: billing-free-tier — Property-based tests for entitlement gate logic

use loremetry_billing::entitlements::{
    can_run_report, ALL_STANDARD_REPORTS, FREE_TIER_REPORTS, SERIES_REPORTS,
};
use loremetry_billing::models::Tier;
use proptest::prelude::*;

// Feature: billing-free-tier, Property 1: Free tier allows exactly the designated reports
// **Validates: Requirements 3.2**
proptest! {
    #[test]
    fn free_tier_allows_designated_reports(index in 0..FREE_TIER_REPORTS.len()) {
        let report_id = FREE_TIER_REPORTS[index];
        let decision = can_run_report(Tier::Free, report_id);
        prop_assert!(
            decision.allowed,
            "Expected Free tier to allow report '{}', but got denied: {}",
            report_id,
            decision.reason
        );
    }
}

/// Compute the set of non-free standard reports (ALL_STANDARD_REPORTS minus FREE_TIER_REPORTS).
fn non_free_reports() -> Vec<&'static str> {
    ALL_STANDARD_REPORTS
        .iter()
        .copied()
        .filter(|id| !FREE_TIER_REPORTS.contains(id))
        .collect()
}

// Feature: billing-free-tier, Property 2: Free tier denies non-free, non-series reports
// **Validates: Requirements 3.3**
proptest! {
    #[test]
    fn free_tier_denies_non_free_standard_reports(index in 0usize..100) {
        let reports = non_free_reports();
        let report_id = reports[index % reports.len()];

        let decision = can_run_report(Tier::Free, report_id);
        prop_assert!(!decision.allowed, "Expected denied for report '{}', got allowed", report_id);
        prop_assert_eq!(decision.reason, "upgrade required",
            "Expected reason 'upgrade required' for report '{}', got '{}'", report_id, decision.reason);
    }
}

// Feature: billing-free-tier, Property 3: Pro tier allows all standard (non-series) reports
// **Validates: Requirements 3.4**
proptest! {
    #[test]
    fn pro_tier_allows_all_standard_reports(index in 0..ALL_STANDARD_REPORTS.len()) {
        let report_id = ALL_STANDARD_REPORTS[index];
        let decision = can_run_report(Tier::Pro, report_id);
        prop_assert!(
            decision.allowed,
            "Expected Pro tier to allow report '{}', but got denied: {}",
            report_id,
            decision.reason
        );
    }
}

// Feature: billing-free-tier, Property 4: Series reports are always denied regardless of tier
// **Validates: Requirements 3.5**
proptest! {
    #[test]
    fn series_reports_always_denied(
        tier_idx in 0usize..2,
        report_idx in 0usize..100,
    ) {
        let tier = match tier_idx {
            0 => Tier::Free,
            _ => Tier::Pro,
        };
        let report_id = SERIES_REPORTS[report_idx % SERIES_REPORTS.len()];

        let decision = can_run_report(tier, report_id);
        prop_assert!(!decision.allowed,
            "Expected denied for series report '{}' with tier {:?}, got allowed",
            report_id, tier);
        prop_assert!(decision.reason.contains("series"),
            "Expected reason to mention 'series' for report '{}', got '{}'",
            report_id, decision.reason);
    }
}

// Feature: billing-free-tier, Property 5: Unknown report IDs are always denied
// **Validates: Requirements 3.6**
proptest! {
    #[test]
    fn unknown_report_ids_always_denied(
        arbitrary_id in "\\PC+",
        tier_idx in 0usize..2,
    ) {
        prop_assume!(!ALL_STANDARD_REPORTS.contains(&arbitrary_id.as_str()));
        prop_assume!(!SERIES_REPORTS.contains(&arbitrary_id.as_str()));

        let tier = match tier_idx {
            0 => Tier::Free,
            _ => Tier::Pro,
        };

        let decision = can_run_report(tier, &arbitrary_id);
        prop_assert!(!decision.allowed,
            "Expected denied for unknown report '{}' with tier {:?}, got allowed",
            arbitrary_id, tier);
        prop_assert_eq!(decision.reason, "unknown report",
            "Expected reason 'unknown report' for '{}', got '{}'",
            arbitrary_id, decision.reason);
    }
}
