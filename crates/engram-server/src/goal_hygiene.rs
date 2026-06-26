//! Goal hygiene — audit active goals, 72h stale autopause, session_end reporting.
//!
//! Prevents manifold sprawl from abandoned `goal_decompose` trees and overnight sprints
//! that never received `goal_update_status(completed)` at session_end.

use crate::store::{
    goal_block_text, goal_current_status, goal_status_is_active, primary_goal_marker_target,
    resolve_active_primary_goal, StoreHandle,
};
use engram_core::types::HolographicBlock;
use serde::Serialize;

/// Default stale threshold — matches `/api/hygiene` `stale_goal` rule.
pub const STALE_GOAL_SECS: u64 = 72 * 3600;

/// Warn when more than this many goals remain `active` after hygiene.
pub const ACTIVE_GOAL_WARN_THRESHOLD: usize = 5;

#[derive(Debug, Clone, Serialize)]
pub struct GoalHygieneReport {
    pub active_count: usize,
    pub active_goals: Vec<String>,
    pub demoted_count: usize,
    pub completed_count: usize,
    pub stale_candidates: Vec<String>,
    pub autopaused: Vec<String>,
    pub warnings: Vec<String>,
    pub autopause_enabled: bool,
    pub stale_threshold_hours: u64,
}

impl GoalHygieneReport {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({}))
    }
}

pub fn autopause_enabled() -> bool {
    !matches!(
        std::env::var("ENGRAM_GOAL_AUTOPAUSE").as_deref(),
        Ok("0") | Ok("false") | Ok("off") | Ok("no")
    )
}

pub fn stale_threshold_secs() -> u64 {
    std::env::var("ENGRAM_GOAL_STALE_HOURS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|h| h.saturating_mul(3600))
        .unwrap_or(STALE_GOAL_SECS)
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Best-effort last-touch epoch for staleness.
/// Access index wins when present (agent recall/goal_status); else block timestamps (never touched).
pub fn goal_last_touch_epoch(store: &StoreHandle, concept: &str, block: &HolographicBlock) -> u64 {
    if let Some(a) = store.access_index.last_accessed(concept) {
        return a;
    }
    let mut ts = 0u64;
    if block.last_accessed_timestamp > 0 {
        ts = ts.max(block.last_accessed_timestamp);
    }
    if block.energetics.ts > 0 {
        ts = ts.max(block.energetics.ts);
    }
    ts
}

pub fn is_goal_stale(
    store: &StoreHandle,
    concept: &str,
    block: &HolographicBlock,
    now: u64,
) -> bool {
    let last = goal_last_touch_epoch(store, concept, block);
    if last == 0 {
        return false;
    }
    now.saturating_sub(last) > stale_threshold_secs()
}

/// Scan all `goal:*` blocks and tally statuses; list active goals.
pub fn audit_active_goals(store: &StoreHandle) -> GoalHygieneReport {
    let goal_concepts = store.list_goal_concepts();
    let mut active_goals: Vec<String> = Vec::new();
    let mut demoted_count = 0usize;
    let mut completed_count = 0usize;
    let now = now_epoch();
    let primary_exempt = resolve_active_primary_goal(store).or_else(|| {
        store
            .fetch_block_high_priority("primary_goal")
            .and_then(|b| primary_goal_marker_target(&b))
    });
    let mut stale_candidates: Vec<String> = Vec::new();

    for concept in goal_concepts {
        let Some(block) = store.fetch_block_high_priority(&concept) else {
            continue;
        };
        let text = goal_block_text(&block);
        let status = goal_current_status(&text).unwrap_or_else(|| "unknown".to_string());
        match status.as_str() {
            "active" => {
                if primary_exempt.as_deref() != Some(concept.as_str())
                    && is_goal_stale(store, &concept, &block, now)
                {
                    stale_candidates.push(concept.clone());
                }
                active_goals.push(concept);
            }
            "demoted" => demoted_count += 1,
            "completed" => completed_count += 1,
            _ => {}
        }
    }

    active_goals.sort();

    let mut warnings: Vec<String> = Vec::new();
    if active_goals.len() > ACTIVE_GOAL_WARN_THRESHOLD {
        warnings.push(format!(
            "{} active goals exceed threshold {} — complete or demote at session_end",
            active_goals.len(),
            ACTIVE_GOAL_WARN_THRESHOLD
        ));
    }

    GoalHygieneReport {
        active_count: active_goals.len(),
        active_goals,
        demoted_count,
        completed_count,
        stale_candidates: stale_candidates.clone(),
        autopaused: Vec::new(),
        warnings,
        autopause_enabled: autopause_enabled(),
        stale_threshold_hours: stale_threshold_secs() / 3600,
    }
}

/// Demote active goals with no touch in `STALE_GOAL_SECS` (except primary).
pub fn autopause_stale_active_goals(store: &mut StoreHandle) -> Vec<String> {
    if !autopause_enabled() {
        return Vec::new();
    }

    let now = now_epoch();
    let primary_exempt = resolve_active_primary_goal(store).or_else(|| {
        store
            .fetch_block_high_priority("primary_goal")
            .and_then(|b| primary_goal_marker_target(&b))
    });
    let goal_concepts = store.list_goal_concepts();
    let mut autopaused: Vec<String> = Vec::new();

    for concept in goal_concepts {
        if primary_exempt.as_deref() == Some(concept.as_str()) {
            continue;
        }
        let Some(block) = store.fetch_block_high_priority(&concept) else {
            continue;
        };
        let text = goal_block_text(&block);
        if !goal_status_is_active(&text) {
            continue;
        }
        if !is_goal_stale(store, &concept, &block, now) {
            continue;
        }

        let note = format!(
            "Autopause: no access in {}h+ (goal_hygiene session_end)",
            stale_threshold_secs() / 3600
        );
        if store
            .apply_goal_status_change(&concept, "demoted", &note)
            .is_ok()
        {
            autopaused.push(concept);
        }
    }

    autopaused
}

/// Run at `session_end`: autopause stale actives, then audit and return report.
pub fn run_session_end_hygiene(store: &mut StoreHandle) -> GoalHygieneReport {
    let autopaused = autopause_stale_active_goals(store);
    let mut report = audit_active_goals(store);
    report.autopaused = autopaused;
    if !report.autopaused.is_empty() {
        report.warnings.push(format!(
            "Autopaused {} stale active goal(s)",
            report.autopaused.len()
        ));
        store.invalidate_continuation_bundle_cache();
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::StoreHandle;

    fn test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "engram_goal_hygiene_{}_{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn stale_goal_detected_when_last_touch_old() {
        let dir = test_dir("stale");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "goal:stale_test",
                "GOAL\n\n**status:** active\n**goal_statement:** old\n",
            )
            .unwrap();
        let now = now_epoch();
        store
            .access_index
            .set_last_accessed_for_test("goal:stale_test", now.saturating_sub(80 * 3600));
        let block = store.fetch_block_high_priority("goal:stale_test").unwrap();
        assert!(is_goal_stale(&store, "goal:stale_test", &block, now));
    }

    #[test]
    fn primary_goal_never_autopaused() {
        let dir = test_dir("primary_exempt");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** goal:engram_mvp_v1\n**set_at:** test\n",
            )
            .unwrap();
        store
            .remember(
                "goal:engram_mvp_v1",
                "GOAL\n\n**status:** active\n**goal_statement:** north star\n",
            )
            .unwrap();
        let now = now_epoch();
        store
            .access_index
            .set_last_accessed_for_test("goal:engram_mvp_v1", now.saturating_sub(90 * 3600));
        std::env::set_var("ENGRAM_GOAL_AUTOPAUSE", "1");
        let paused = autopause_stale_active_goals(&mut store);
        assert!(!paused.contains(&"goal:engram_mvp_v1".to_string()));
        let text = goal_block_text(
            &store
                .fetch_block_high_priority("goal:engram_mvp_v1")
                .unwrap(),
        );
        assert!(goal_status_is_active(&text));
    }

    #[test]
    fn autopause_demotes_stale_non_primary() {
        let dir = test_dir("autopause");
        let mut store = StoreHandle::new(&dir.to_string_lossy());
        store
            .remember(
                "primary_goal",
                "PRIMARY GOAL\n\n**goal:** goal:engram_mvp_v1\n**set_at:** test\n",
            )
            .unwrap();
        store
            .remember(
                "goal:engram_mvp_v1",
                "GOAL\n\n**status:** active\n**goal_statement:** north star\n",
            )
            .unwrap();
        store
            .remember(
                "goal:old_sprint",
                "GOAL\n\n**status:** active\n**goal_statement:** abandoned sprint\n",
            )
            .unwrap();
        let now = now_epoch();
        store.access_index.touch("goal:engram_mvp_v1");
        store
            .access_index
            .set_last_accessed_for_test("goal:old_sprint", now.saturating_sub(100 * 3600));
        std::env::set_var("ENGRAM_GOAL_AUTOPAUSE", "1");
        let paused = autopause_stale_active_goals(&mut store);
        assert_eq!(paused, vec!["goal:old_sprint".to_string()]);
        let text = goal_block_text(&store.fetch_block_high_priority("goal:old_sprint").unwrap());
        assert_eq!(goal_current_status(&text).as_deref(), Some("demoted"));
    }
}
