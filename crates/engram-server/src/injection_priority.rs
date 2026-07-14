//! Pure ranking + completeness scoring for context injection (wake / continuation / harness).
//!
//! Keeps NVMe-bypass delivery logic testable without StoreHandle I/O.

use std::collections::HashMap;

/// Block count above which BVH + GPU hot path is expected for full recall.
pub const LARGE_MANIFOLD_THRESHOLD: usize = 10_000;

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
    /// RoMem relation edge volatility α ∈ [0,1]. 0 = unset (no damping).
    /// High α damps injection rank so static structure surfaces first (RSI Cycle 23).
    pub edge_volatility: f32,
}

/// Master switch for RoMem α speed-gate (Cycles 20–25).
/// Env `ENGRAM_ALPHA_SPEED_GATE`: unset / `1` / `true` / `yes` / `on` → enabled (default).
/// `0` / `false` / `no` / `off` → disabled globally for surfaces that honor the gate.
pub fn alpha_speed_gate_enabled() -> bool {
    match std::env::var("ENGRAM_ALPHA_SPEED_GATE") {
        Ok(v) => {
            let t = v.trim().to_ascii_lowercase();
            !(t.is_empty() || t == "0" || t == "false" || t == "no" || t == "off")
        }
        Err(_) => true,
    }
}

/// Per-call `alpha_weighted` override, else master switch default.
pub fn resolve_alpha_weighted(explicit: Option<bool>) -> bool {
    explicit.unwrap_or_else(alpha_speed_gate_enabled)
}

/// Scale injection weight by edge volatility α (aligned with presentation `score_alpha_scale`).
/// `0` / unset → 1.0 (no damping). Static α≈0.12 → ~0.96; dynamic α≈0.85 → ~0.77.
/// When master gate is off, always returns 1.0.
pub fn edge_volatility_scale(volatility: f32) -> f32 {
    if !alpha_speed_gate_enabled() {
        return 1.0;
    }
    if volatility <= 0.0 {
        return 1.0;
    }
    let vol = volatility.clamp(0.01, 1.0);
    1.0 / (1.0 + 0.35 * vol)
}

/// Continuity anchors never receive α damping (wake + momentum paths).
pub fn protect_alpha_damp(concept: &str) -> bool {
    concept == "primary_goal"
        || concept.starts_with("scar:")
        || concept.starts_with("helper:session_handoff")
        || concept.starts_with("compression_handoff_")
}

/// CRS×α joint reweight for Dirichlet recall (RSI Cycle 27).
/// Env `ENGRAM_CRS_ALPHA_JOINT`: unset/1/true → on; 0/false/off → off.
/// When on and α gate on, multiplies score by `edge_volatility_scale(edge_vol)`.
pub fn crs_alpha_joint_enabled() -> bool {
    if !alpha_speed_gate_enabled() {
        return false;
    }
    match std::env::var("ENGRAM_CRS_ALPHA_JOINT") {
        Ok(v) => {
            let t = v.trim().to_ascii_lowercase();
            !(t == "0" || t == "false" || t == "no" || t == "off")
        }
        Err(_) => true,
    }
}

/// Apply CRS×α joint scale to a Dirichlet recall score.
pub fn apply_crs_alpha_joint(score: f32, edge_volatility: f32, concept: &str) -> f32 {
    if !crs_alpha_joint_enabled() || protect_alpha_damp(concept) {
        return score;
    }
    score * edge_volatility_scale(edge_volatility)
}

/// 80/20 q/p momentum blend, optionally re-weighted by RoMem edge α (RSI Cycle 24).
pub fn momentum_alpha_score(
    q_score: f32,
    p_score: f32,
    edge_volatility: f32,
    apply_alpha: bool,
    concept: &str,
) -> f32 {
    let base = (0.80 * q_score + 0.20 * p_score).clamp(-1.0, 1.0);
    if !apply_alpha || protect_alpha_damp(concept) {
        return base;
    }
    (base * edge_volatility_scale(edge_volatility)).clamp(-1.0, 1.0)
}

/// Composite rank for right-time delivery: CRS + hot + recency + momentum + anchor/scar boosts.
/// Non-anchor artifacts are damped by `edge_volatility_scale` (α speed-gate).
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
    let base = crs_w + hot_w + recency_w + momentum_w + anchor_w;
    // Load-bearing continuity slots are never α-damped; honor master gate.
    let scaled = if a.is_scar || a.is_handoff || a.is_primary_anchor || !alpha_speed_gate_enabled()
    {
        base
    } else {
        base * edge_volatility_scale(a.edge_volatility)
    };
    scaled.clamp(0.0, 1.5)
}

/// Build recency rank map from access-index tuples (0 = freshest).
pub fn recency_rank_map(recent: &[(String, u64)]) -> HashMap<String, u32> {
    let mut map = HashMap::new();
    for (i, (concept, _)) in recent.iter().enumerate() {
        map.entry(concept.clone()).or_insert(i as u32);
    }
    map
}

/// Build an artifact for ranking from concept metadata.
/// Set `edge_volatility` on the result when RoMem α is known (0 = unset / no damping).
pub fn artifact_for_concept(
    concept: &str,
    crs: f32,
    hot: bool,
    recency_rank: &HashMap<String, u32>,
    momentum_score: f32,
    source: &str,
    handoff_concept: &str,
) -> InjectionArtifact {
    InjectionArtifact {
        concept: concept.to_string(),
        crs,
        hot,
        recency_rank: recency_rank.get(concept).copied().unwrap_or(999),
        momentum_score,
        source: source.to_string(),
        is_scar: concept.starts_with("scar:"),
        is_handoff: concept == handoff_concept || concept.starts_with("compression_handoff_"),
        is_primary_anchor: concept == "primary_goal",
        edge_volatility: 0.0,
    }
}

/// Sort artifacts by composite injection rank (highest first).
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

/// True when recall is on a BVH-backed NVMe path (not cpu_linear / sampled_bounded warmup).
pub fn nvme_recall_path_ready(recall_mode: &str) -> bool {
    matches!(recall_mode, "full_bvh_gpu" | "full_bvh")
}

/// True when GPU hot residency is satisfied for the current recall mode.
pub fn gpu_hot_slot_ready(
    recall_mode: &str,
    gpu_hot_resident: bool,
    leg_block_count: usize,
) -> bool {
    match recall_mode {
        "full_bvh_gpu" => gpu_hot_resident,
        "sampled_bounded" if leg_block_count > LARGE_MANIFOLD_THRESHOLD => false,
        _ => true, // small store or non-GPU recall — slot N/A, counts filled
    }
}

/// Inputs for `compute_injection_completeness` (bundled to satisfy clippy::too_many_arguments).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InjectionCompletenessInput {
    pub has_primary: bool,
    pub has_handoff: bool,
    pub has_trace_head: bool,
    pub open_scars: usize,
    pub hot_tile_count: usize,
    pub presentation_nodes: usize,
    pub recall_mode: &'static str,
    pub gpu_hot_resident: bool,
    pub leg_block_count: usize,
}

/// Score whether the agent received the minimum continuity slots on wake.
pub fn compute_injection_completeness(input: InjectionCompletenessInput) -> InjectionCompleteness {
    let InjectionCompletenessInput {
        has_primary,
        has_handoff,
        has_trace_head,
        open_scars,
        hot_tile_count,
        presentation_nodes,
        recall_mode,
        gpu_hot_resident,
        leg_block_count,
    } = input;

    let slots: [(&str, bool); 8] = [
        ("primary_goal", has_primary),
        ("session_handoff", has_handoff),
        ("trace_chain_head", has_trace_head),
        ("open_scars_surfaced", open_scars > 0 || !has_handoff),
        ("hot_tiles", hot_tile_count > 0),
        ("presentation_stratum", presentation_nodes > 0),
        ("nvme_recall_path", nvme_recall_path_ready(recall_mode)),
        (
            "gpu_hot_resident",
            gpu_hot_slot_ready(recall_mode, gpu_hot_resident, leg_block_count),
        ),
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
            edge_volatility: 0.0,
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
    fn alpha_speed_gate_env_defaults_on() {
        std::env::remove_var("ENGRAM_ALPHA_SPEED_GATE");
        assert!(alpha_speed_gate_enabled());
        assert!(resolve_alpha_weighted(None));
        assert!(!resolve_alpha_weighted(Some(false)));
        assert!(resolve_alpha_weighted(Some(true)));
        std::env::set_var("ENGRAM_ALPHA_SPEED_GATE", "0");
        assert!(!alpha_speed_gate_enabled());
        assert!(!resolve_alpha_weighted(None));
        // Explicit tool override still wins when master off
        assert!(resolve_alpha_weighted(Some(true)));
        std::env::set_var("ENGRAM_ALPHA_SPEED_GATE", "true");
        assert!(alpha_speed_gate_enabled());
        std::env::remove_var("ENGRAM_ALPHA_SPEED_GATE");
    }

    #[test]
    fn edge_volatility_scale_prefers_static() {
        std::env::remove_var("ENGRAM_ALPHA_SPEED_GATE");
        assert!((edge_volatility_scale(0.0) - 1.0).abs() < 1e-6);
        assert!(edge_volatility_scale(0.12) > edge_volatility_scale(0.85));
        assert!((edge_volatility_scale(0.12) - 1.0 / (1.0 + 0.35 * 0.12)).abs() < 1e-5);
        std::env::set_var("ENGRAM_ALPHA_SPEED_GATE", "off");
        assert!((edge_volatility_scale(0.85) - 1.0).abs() < 1e-6);
        std::env::remove_var("ENGRAM_ALPHA_SPEED_GATE");
    }

    #[test]
    fn crs_alpha_joint_prefers_static_edges() {
        std::env::remove_var("ENGRAM_ALPHA_SPEED_GATE");
        std::env::remove_var("ENGRAM_CRS_ALPHA_JOINT");
        assert!(crs_alpha_joint_enabled());
        let base = 0.80_f32;
        let static_s = apply_crs_alpha_joint(base, 0.12, "tile:static");
        let dyn_s = apply_crs_alpha_joint(base, 0.85, "tile:dyn");
        assert!(static_s > dyn_s);
        assert!((apply_crs_alpha_joint(base, 0.99, "scar:x") - base).abs() < 1e-5);
        std::env::set_var("ENGRAM_CRS_ALPHA_JOINT", "0");
        assert!(!crs_alpha_joint_enabled());
        assert!((apply_crs_alpha_joint(base, 0.85, "tile:x") - base).abs() < 1e-5);
        std::env::remove_var("ENGRAM_CRS_ALPHA_JOINT");
    }

    #[test]
    fn momentum_alpha_score_damps_high_vol_non_anchors() {
        let q = 0.9_f32;
        let p = 0.5_f32;
        let base = momentum_alpha_score(q, p, 0.0, true, "tile:x");
        let static_s = momentum_alpha_score(q, p, 0.12, true, "tile:x");
        let dyn_s = momentum_alpha_score(q, p, 0.85, true, "tile:x");
        assert!((base - (0.80 * q + 0.20 * p)).abs() < 1e-5);
        assert!(static_s > dyn_s);
        assert!(static_s < base + 1e-5);
        // Continuity protect
        let scar = momentum_alpha_score(q, p, 0.99, true, "scar:dead");
        assert!((scar - base).abs() < 1e-5);
        // Opt-out
        let off = momentum_alpha_score(q, p, 0.85, false, "tile:x");
        assert!((off - base).abs() < 1e-5);
    }

    #[test]
    fn high_alpha_damps_injection_rank_for_non_anchors() {
        std::env::remove_var("ENGRAM_ALPHA_SPEED_GATE");
        std::env::remove_var("ENGRAM_CRS_ALPHA_JOINT");
        let mut static_tile = artifact("tile:static_spec", 0.88, true, 2);
        static_tile.edge_volatility = 0.12;
        let mut dynamic_tile = artifact("tile:churn_spec", 0.88, true, 2);
        dynamic_tile.edge_volatility = 0.85;
        assert!(
            injection_rank_score(&static_tile) > injection_rank_score(&dynamic_tile),
            "static α should outrank high-α at equal CRS/hot"
        );
        // Handoff never damped even with high α
        let mut handoff = artifact("helper:session_handoff_latest", 0.94, true, 1);
        handoff.is_handoff = true;
        handoff.edge_volatility = 0.99;
        let undamped = {
            let mut h = handoff.clone();
            h.edge_volatility = 0.0;
            injection_rank_score(&h)
        };
        assert!((injection_rank_score(&handoff) - undamped).abs() < 1e-5);
    }

    fn completeness_input(
        has_primary: bool,
        has_handoff: bool,
        has_trace_head: bool,
        open_scars: usize,
        hot_tile_count: usize,
        presentation_nodes: usize,
        recall_mode: &'static str,
        gpu_hot_resident: bool,
        leg_block_count: usize,
    ) -> InjectionCompletenessInput {
        InjectionCompletenessInput {
            has_primary,
            has_handoff,
            has_trace_head,
            open_scars,
            hot_tile_count,
            presentation_nodes,
            recall_mode,
            gpu_hot_resident,
            leg_block_count,
        }
    }

    #[test]
    fn completeness_full_when_all_slots_present() {
        let c = compute_injection_completeness(completeness_input(
            true,
            true,
            true,
            0,
            3,
            5,
            "full_bvh_gpu",
            true,
            67_000,
        ));
        assert!(c.score >= 0.85, "score={}", c.score);
        assert!(c.missing.is_empty() || c.missing == ["open_scars_surfaced"]);
    }

    #[test]
    fn completeness_flags_missing_handoff() {
        let c = compute_injection_completeness(completeness_input(
            true,
            false,
            false,
            0,
            0,
            0,
            "sampled_bounded",
            false,
            67_000,
        ));
        assert!(c.score < 0.6);
        assert!(c.missing.contains(&"session_handoff"));
    }

    #[test]
    fn gpu_hot_slot_fails_when_large_store_warming() {
        assert!(!gpu_hot_slot_ready("sampled_bounded", false, 67_000));
        assert!(!nvme_recall_path_ready("cpu_linear"));
        assert!(gpu_hot_slot_ready("cpu_linear", false, 100));
    }

    #[test]
    fn gpu_hot_slot_requires_resident_on_full_bvh_gpu() {
        assert!(!gpu_hot_slot_ready("full_bvh_gpu", false, 67_000));
        assert!(gpu_hot_slot_ready("full_bvh_gpu", true, 67_000));
    }
}
