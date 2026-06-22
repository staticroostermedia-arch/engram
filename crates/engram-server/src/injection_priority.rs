//! Pure ranking + completeness scoring for context injection (wake / continuation / harness).
//!
//! Keeps NVMe-bypass delivery logic testable without StoreHandle I/O.

/// One artifact candidate for wake/continuation injection.
#[derive(Debug, Clone, PartialEq)]
pub struct InjectionArtifact {
    pub concept: String,
    pub crs: f32,
    pub hot: bool,
    /// Lower = more recent within the batch (0 = freshest).
    pub recency_rank: u32,
    /// Recall momentum / similarity when sourced from momentum_recall (0..1).
    pub momentum_score: f32,
    pub source: String,
    pub is_scar: bool,
    pub is_handoff: bool,
    pub is_primary_anchor: bool,
}

/// Composite rank for right-time delivery: CRS + hot + recency + momentum + anchor/scar boosts.
pub fn injection_rank_score(a: &InjectionArtifact) -> f32 {
    let crs_w = a.crs.clamp(0.0, 1.0) * 0.35;
    let hot_w = if a.hot { 0.18 } else { 0.0 };
    let recency_w = 0.12 * (1.0 / (1.0 + a.recency_rank as f32));
    let momentum_w = a.momentum_score.clamp(0.0, 1.0) * 0.15;
    let anchor_w = if a.is_scar {
        0.36
    } else if a.is_handoff {
        0.22
    } else if a.is_primary_anchor {
        0.18
    } else {
        0.0
    };
    (crs_w + hot_w + recency_w + momentum_w + anchor_w).clamp(0.0, 1.5)
}

/// Sort artifacts by composite injection rank (highest first).
#[allow(dead_code)] // Public API for harness/tests; store uses `injection_rank_score` inline.
pub fn prioritize_artifacts(mut artifacts: Vec<InjectionArtifact>) -> Vec<InjectionArtifact> {
    artifacts.sort_by(|a, b| {
        injection_rank_score(b)
            .partial_cmp(&injection_rank_score(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    artifacts
}

/// Observable completeness of a wake injection surface (0..1).
#[derive(Debug, Clone, PartialEq)]
pub struct InjectionCompleteness {
    pub score: f32,
    pub slots_filled: u8,
    pub slots_total: u8,
    pub missing: Vec<&'static str>,
}

/// Score whether the agent received the minimum continuity slots on wake.
pub fn compute_injection_completeness(
    has_primary: bool,
    has_handoff: bool,
    has_trace_head: bool,
    open_scars: usize,
    hot_tile_count: usize,
    presentation_nodes: usize,
    recall_mode: &str,
    bvh_ready: bool,
    gpu_hot_resident: bool,
) -> InjectionCompleteness {
    let slots: [(&str, bool); 8] = [
        ("primary_goal", has_primary),
        ("session_handoff", has_handoff),
        ("trace_chain_head", has_trace_head),
        ("open_scars_surfaced", open_scars > 0 || !has_handoff),
        ("hot_tiles", hot_tile_count > 0),
        ("presentation_stratum", presentation_nodes > 0),
        (
            "nvme_recall_path",
            recall_mode == "full_bvh_gpu" || recall_mode == "full_bvh" || bvh_ready,
        ),
        ("gpu_hot_resident", gpu_hot_resident || !bvh_ready),
    ];

    let filled = slots.iter().filter(|(_, ok)| *ok).count() as u8;
    let total = slots.len() as u8;
    let missing: Vec<&'static str> = slots
        .iter()
        .filter(|(_, ok)| !*ok)
        .map(|(name, _)| *name)
        .collect();
    let score = filled as f32 / total as f32;

    InjectionCompleteness {
        score,
        slots_filled: filled,
        slots_total: total,
        missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(concept: &str, crs: f32, hot: bool, recency: u32) -> InjectionArtifact {
        InjectionArtifact {
            concept: concept.to_string(),
            crs,
            hot,
            recency_rank: recency,
            momentum_score: 0.0,
            source: "test".to_string(),
            is_scar: false,
            is_handoff: false,
            is_primary_anchor: false,
        }
    }

    #[test]
    fn handoff_outranks_low_crs_episodic() {
        let mut handoff = artifact("helper:session_handoff_latest", 0.94, true, 1);
        handoff.is_handoff = true;
        let episodic = artifact("trace:old", 0.5, false, 50);
        let ranked = prioritize_artifacts(vec![episodic.clone(), handoff.clone()]);
        assert_eq!(ranked[0].concept, "helper:session_handoff_latest");
        assert!(injection_rank_score(&ranked[0]) > injection_rank_score(&episodic));
    }

    #[test]
    fn scar_gets_repulsion_boost() {
        let mut scar = artifact("scar:dead_approach", 0.6, false, 5);
        scar.is_scar = true;
        let tile = artifact("tile:spec", 0.88, true, 2);
        let ranked = prioritize_artifacts(vec![tile.clone(), scar.clone()]);
        assert_eq!(ranked[0].concept, "scar:dead_approach");
    }

    #[test]
    fn completeness_full_when_all_slots_present() {
        let c = compute_injection_completeness(true, true, true, 0, 3, 5, "full_bvh_gpu", true, true);
        assert!(c.score >= 0.85, "score={}", c.score);
        assert!(c.missing.is_empty() || c.missing == ["open_scars_surfaced"]);
    }

    #[test]
    fn completeness_flags_missing_handoff() {
        let c = compute_injection_completeness(true, false, false, 0, 0, 0, "sampled_bounded", false, false);
        assert!(c.score < 0.6);
        assert!(c.missing.contains(&"session_handoff"));
    }
}