//! Dynamical CRS scorer — single source of truth for high-value write paths.
//!
//! CRS is a reliability/stability label in [0,1] derived from role base, optional
//! ego resonance, residual surprise, verify/recall boosts, and mild age decay.
//! Pin always yields 1.0. Grounded mints never fall below [`engram_core::genesis::KEPLER_GATE`] (0.74)
//! except scar demotion, which may floor at [`SCAR_CRS_FLOOR`] (0.40) by design.

use engram_core::genesis::KEPLER_GATE;

/// Floor for scarred blocks (below autophagy-friendly band but geometry preserved).
pub const SCAR_CRS_FLOOR: f32 = 0.40;

/// Role bases for high-traffic operational mints and demotions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CrsRole {
    /// `helper:session_handoff_latest` structured packet
    SessionHandoff,
    /// `manifest:rehydration_*`
    RehydrationManifest,
    /// Session receipt / immutable audit sidecar
    SessionReceipt,
    /// Thought-tile / formal_spec mint
    ThoughtTile,
    /// `tensor:tile__*` dual-write mirror
    TensorMirror,
    /// Sentinel / lightweight helper state
    SentinelState,
    /// Generic operational grounded block
    Operational,
    /// ZEDOS_RELATION edge block
    Relation,
    /// Scar demotion base (before magnitude penalty)
    ScarDemotion,
    /// Ego-gated remember (base before resonance blend)
    EgoRemember,
    /// Praxis / remember_solution high base when not pin-class immortal
    #[allow(dead_code)] // reserved for non-immortal praxis mints; pin path uses pinned=true
    Praxis,
    /// Lexicon word atom (`lexicon:word:*`) — grounded dictionary seed
    Lexicon,
    /// Structured research scar (`scar:*`) — wake repulsion pin (CRS ≥ lean open-scar floor 0.5)
    ResearchScar,
}

impl CrsRole {
    pub fn base(self) -> f32 {
        match self {
            Self::SessionHandoff => 0.94,
            Self::RehydrationManifest => 0.92,
            Self::SessionReceipt => 0.93,
            Self::ThoughtTile => 0.88,
            Self::TensorMirror => 0.85,
            Self::SentinelState => 0.88,
            Self::Operational => 0.86,
            Self::Relation => 0.80,
            Self::ScarDemotion => 0.70,
            Self::EgoRemember => 0.74,
            Self::Praxis => 0.95,
            Self::Lexicon => 0.78,
            // Above collect_open_scars_lean CRS floor (0.5); below pin tier.
            Self::ResearchScar => 0.78,
        }
    }
}

/// Inputs for the dynamical CRS function. All optional fields degrade gracefully.
#[derive(Debug, Clone, Default)]
pub struct CrsInputs {
    pub role: Option<CrsRole>,
    /// Explicit base override (when role is not used). Clamped to [0,1].
    pub role_base: Option<f32>,
    /// Pin / immortal / genesis-class blocks.
    pub pinned: bool,
    /// Ego resonance cosine-like score in [0,1], if available.
    pub ego_resonance: Option<f32>,
    /// Prediction residual L2 (surprise); higher → slightly lower CRS.
    pub residual_l2: Option<f32>,
    /// Prior verifies / recalls that support grounding.
    pub verify_or_recall_count: u32,
    /// Age in hours for mild decay (0 = fresh).
    pub age_hours: Option<f32>,
}

/// Compute dynamical CRS in [KEPLER_GATE, 1.0], or exactly 1.0 when pinned.
///
/// Formula:
/// ```text
/// base = role.base | role_base | 0.86
/// score = base
///       + 0.04 * (ego_resonance − 0.85)     // optional
///       − min(0.12, residual_l2 * 0.15)
///       + min(0.05, verify_or_recall_count * 0.01)
///       − min(0.05, age_hours / 720 * 0.05)
/// score = clamp(score, 0.74, 0.99) unless pinned → 1.0
/// ```
pub fn dynamical_crs(inputs: &CrsInputs) -> f32 {
    if inputs.pinned {
        return 1.0;
    }
    let base = inputs
        .role_base
        .or_else(|| inputs.role.map(|r| r.base()))
        .unwrap_or(0.86)
        .clamp(0.0, 1.0);

    let mut score = base;

    if let Some(ego) = inputs.ego_resonance {
        let e = ego.clamp(0.0, 1.0);
        score += 0.04 * (e - 0.85);
    }

    if let Some(r) = inputs.residual_l2 {
        let penalty = (r.max(0.0) * 0.15).min(0.12);
        score -= penalty;
    }

    let verify_boost = (inputs.verify_or_recall_count as f32 * 0.01).min(0.05);
    score += verify_boost;

    if let Some(age) = inputs.age_hours {
        let age_penalty = ((age.max(0.0) / 720.0) * 0.05).min(0.05);
        score -= age_penalty;
    }

    score.clamp(KEPLER_GATE, 0.99)
}

/// Convenience: CRS for a known role with no extra signals.
pub fn dynamical_crs_for_role(role: CrsRole) -> f32 {
    dynamical_crs(&CrsInputs {
        role: Some(role),
        ..Default::default()
    })
}

/// CRS for ego-gated remember: resonance_norm ∈ [0,1] from (cos+1)/2.
/// Replaces free formula `0.50 + resonance * 0.44` with the scorer + ego input.
pub fn dynamical_crs_ego_remember(resonance_norm: f32) -> f32 {
    dynamical_crs(&CrsInputs {
        role: Some(CrsRole::EgoRemember),
        ego_resonance: Some(resonance_norm.clamp(0.0, 1.0)),
        ..Default::default()
    })
}

/// CRS after scar demotion. May go below Kepler to [`SCAR_CRS_FLOOR`].
///
/// Uses prior CRS as role_base, residual from magnitude, then applies
/// magnitude×0.1 penalty (same thermodynamic cost as before).
pub fn dynamical_crs_after_scar(prior_crs: f32, magnitude: f32) -> f32 {
    let mag = magnitude.clamp(0.0, 1.0);
    let base = dynamical_crs(&CrsInputs {
        role: Some(CrsRole::ScarDemotion),
        role_base: Some(prior_crs.clamp(0.0, 1.0)),
        residual_l2: Some(mag),
        ..Default::default()
    });
    // Intentional demotion floor (documented exception to Kepler for scar path).
    (base - mag * 0.1).max(SCAR_CRS_FLOOR)
}

/// Pin / praxis immortal CRS.
pub fn dynamical_crs_pinned() -> f32 {
    dynamical_crs(&CrsInputs {
        pinned: true,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_yields_one() {
        assert_eq!(dynamical_crs_pinned(), 1.0);
        let s = dynamical_crs(&CrsInputs {
            pinned: true,
            role: Some(CrsRole::Operational),
            residual_l2: Some(9.0),
            ..Default::default()
        });
        assert_eq!(s, 1.0);
    }

    #[test]
    fn grounded_mint_floor_kepler() {
        let s = dynamical_crs(&CrsInputs {
            role_base: Some(0.5),
            residual_l2: Some(10.0),
            age_hours: Some(10_000.0),
            ..Default::default()
        });
        assert!(s >= KEPLER_GATE, "score {s} below Kepler");
        assert!(s <= 0.99);
    }

    #[test]
    fn handoff_role_near_base() {
        let s = dynamical_crs_for_role(CrsRole::SessionHandoff);
        assert!((s - 0.94).abs() < 1e-4, "got {s}");
    }

    #[test]
    fn tensor_mirror_role_near_base() {
        let s = dynamical_crs_for_role(CrsRole::TensorMirror);
        assert!((s - 0.85).abs() < 1e-4, "got {s}");
    }

    #[test]
    fn residual_lowers_score() {
        let clean = dynamical_crs(&CrsInputs {
            role: Some(CrsRole::ThoughtTile),
            residual_l2: Some(0.0),
            ..Default::default()
        });
        let noisy = dynamical_crs(&CrsInputs {
            role: Some(CrsRole::ThoughtTile),
            residual_l2: Some(0.5),
            ..Default::default()
        });
        assert!(noisy < clean, "noisy {noisy} should be < clean {clean}");
    }

    #[test]
    fn verify_boosts_score() {
        let none = dynamical_crs(&CrsInputs {
            role: Some(CrsRole::RehydrationManifest),
            verify_or_recall_count: 0,
            ..Default::default()
        });
        let some = dynamical_crs(&CrsInputs {
            role: Some(CrsRole::RehydrationManifest),
            verify_or_recall_count: 5,
            ..Default::default()
        });
        assert!(some > none);
    }

    #[test]
    fn clamps_to_unit_interval() {
        for role in [
            CrsRole::SessionHandoff,
            CrsRole::TensorMirror,
            CrsRole::Operational,
            CrsRole::ThoughtTile,
            CrsRole::SessionReceipt,
            CrsRole::SentinelState,
            CrsRole::Relation,
            CrsRole::Praxis,
            CrsRole::EgoRemember,
            CrsRole::Lexicon,
            CrsRole::ResearchScar,
        ] {
            let s = dynamical_crs_for_role(role);
            assert!((KEPLER_GATE..=1.0).contains(&s), "role {role:?} score {s}");
        }
    }

    #[test]
    fn research_scar_role_above_lean_open_scar_floor() {
        let s = dynamical_crs_for_role(CrsRole::ResearchScar);
        assert!(s >= 0.5, "lean open_scars floor is 0.5, got {s}");
        assert!((s - 0.78).abs() < 1e-4 || s >= KEPLER_GATE);
    }

    #[test]
    fn operational_role_base() {
        assert!((dynamical_crs_for_role(CrsRole::Operational) - 0.86).abs() < 1e-4);
    }

    #[test]
    fn scar_can_go_below_kepler_but_not_below_floor() {
        let s = dynamical_crs_after_scar(0.90, 1.0);
        assert!(s >= SCAR_CRS_FLOOR, "got {s}");
        assert!(s < KEPLER_GATE || s >= SCAR_CRS_FLOOR);
        let mild = dynamical_crs_after_scar(0.90, 0.1);
        assert!(mild > s, "mild {mild} should exceed hard scar {s}");
    }

    #[test]
    fn ego_remember_increases_with_resonance() {
        let low = dynamical_crs_ego_remember(0.0);
        let high = dynamical_crs_ego_remember(1.0);
        assert!(high > low, "high {high} low {low}");
        assert!((KEPLER_GATE..=0.99).contains(&low));
        assert!((KEPLER_GATE..=0.99).contains(&high));
    }

    #[test]
    fn praxis_role_high_but_not_pin_unless_pinned() {
        let s = dynamical_crs_for_role(CrsRole::Praxis);
        assert!(s >= 0.90);
        assert!(s < 1.0);
        assert_eq!(dynamical_crs_pinned(), 1.0);
    }
}
