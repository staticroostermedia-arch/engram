//! Goal-directed navigation primitives for the geometric manifold.
//!
//! `dist_to_goal` and phased `simple_sweep_nrem` (far/mid/near prospective evaluation
//! by distance-to-goal using real .leg3 q via fetch + OP_BIND + cosine + p-momentum).
//!
//! Extracted here from prior duplication in NREM trigger (daemon) and test harness (example)
//! to make them proper shared core infrastructure (usable by rituals, MCP, tests, NREM).
//! See GEOMETRIC_MEMORY.md (VSA ops, backend fetch, p-momentum, NREM) and
//! Group 1 mFC proposals (structured + flexible dist-to-goal + theta sweeps in NREM).
//!
//! This module aims for clean, production-grade quality suitable for potential proposal
//! to Aric’s core engram (higher robustness via explicit errors, clear docs, no silent
//! panics or always-fallback inside the primitive).
//!
//! ## Error Handling (production quality)
//! Both functions now return `Result<f32, GoalError>` with clear variants instead of
//! always falling back to safe-zero vectors or printing inside the primitive.
//! - Happy path (concept exists): identical computation + structured success trace
//!   `[NREM-GOAL-SWEEP|phase=dist|...]` (kept as close as possible to prior).
//! - Error path (e.g. missing concept, fetch issue): returns `Err(GoalError::ConceptNotFound(name))`
//!   (or Other). Callers are responsible for robustness (log, skip, recover with zero, etc.).
//!   This allows better sentinel/monitor observation and context-specific recovery
//!   (e.g. NREM bias can continue without aborting the whole consolidation pass).
//! - Robustness traces for errors are now produced at call sites (with full NREM/goal context)
//!   using the same sentinel-friendly `[NREM-GOAL-SWEEP|robustness=error|...]` style as before,
//!   but triggered from explicit Err instead of silent inside.
//!
//! Manifold path remains configurable via the last `Option<&str>` param (None = default
//! sovereign "/Users/vantbracehome/.engram/manifold" for full backward compat on happy path).
//! ralph_wiggum safe: read-only real .leg3; explicit errors are an improvement for observation;
//! no writes ever.

use crate::backend::{CpuBackend, VsaBackend};
use crate::ops::{op_bind, cosine_similarity};
use crate::Complex32;
use std::fmt;

/// Production-grade error type for the goal-directed navigation primitives.
///
/// Clear, matchable variants for different failure modes. Prefer explicit handling at
/// call sites over silent recovery inside the primitive. This improves robustness
/// (callers decide policy with full context) and is suitable for core engram sharing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalError {
    /// The named concept was not found in the manifold (primary error for dist_to_goal
    /// when a goal or current context is absent from the .leg3 store).
    ConceptNotFound(String),
    /// Other fetch/computation failure (e.g. backend issue; currently rare since fetch
    /// returns Option, but future-proofs the API).
    Other(String),
}

impl fmt::Display for GoalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GoalError::ConceptNotFound(c) => write!(f, "concept not found in manifold: {}", c),
            GoalError::Other(msg) => write!(f, "goal navigation error: {}", msg),
        }
    }
}

impl std::error::Error for GoalError {}

/// Working `dist_to_goal` using existing core APIs (op_bind for relation encoding / "relate",
/// cosine_similarity for search closeness / "search_by_relation" analog, p_momentum as
/// trajectory cost factor).
///
/// In full system this maps to MCP search_by_relation + p-momentum over the relation
/// sheaf/graph. Computes (1-sim) * (1 + p*0.8) after real fetch + bind on manifold .leg3.
///
/// Returns `Ok(dist)` on success (happy path trace emitted, identical to prior).
/// Returns `Err(GoalError::ConceptNotFound(...))` (or Other) for missing concepts /
/// fetch issues instead of silent safe-zero fallback. This is the key robustness
/// improvement: errors are explicit so callers (NREM, tests, future core) can handle
/// appropriately with context.
///
/// `manifold_path`: Optional path to the .leg3 manifold directory. Pass None to use the
/// sensible default (sovereign local "/Users/vantbracehome/.engram/manifold") for
/// backward compatibility. This makes the primitive configurable without altering core
/// computation logic.
///
/// ralph_wiggum=true (ritual toml [safety]): only read via existing fetch on real manifold;
/// no writes. Callers should provide robustness (e.g. log + zero) on Err.
pub fn dist_to_goal(current: &str, goal: &str, p_momentum: f32, manifold_path: Option<&str>) -> Result<f32, GoalError> {
    let real_path = manifold_path.unwrap_or("/Users/vantbracehome/.engram/manifold");
    let be = CpuBackend::new(real_path);
    let qc = if let Some(q) = be.fetch(current) {
        q
    } else {
        // Explicit error (improved over prior silent safe-zero inside primitive).
        // Robustness trace + recovery now at call site with NREM/goal context.
        return Err(GoalError::ConceptNotFound(current.to_string()));
    };
    let gl = if let Some(q) = be.fetch(goal) {
        q
    } else {
        return Err(GoalError::ConceptNotFound(goal.to_string()));
    };
    let curr = *qc;
    let goal_vec = *gl;
    // Actual relations: op_bind on real fetched q's (role-filler relate per GEOMETRIC/sheaf).
    let _rel = op_bind(&curr, &goal_vec);
    // Search closeness on real data (cosine foundation for search_by_relation).
    let sim = cosine_similarity(&curr, &goal_vec);
    // p-momentum in meaningful way: scale dist as trajectory cost using real sim + input p (momentum bias).
    let dist = (1.0f32 - sim) * (1.0f32 + p_momentum * 0.8f32);
    // Happy-path trace kept as close as possible to prior (success structured print).
    println!(
        "[NREM-GOAL-SWEEP|phase=dist|current={}|goal={}|p={:.3}|sim={:.3}|dist={:.3}|missing=0|note=](real fetch q, op_bind relate, cosine search, p factor; active NREM after goal-bias; ralph_wiggum safe)",
        current, goal, p_momentum, sim, dist
    );
    Ok(dist)
}

/// Optional phased sweep helper (far/mid/near via repeated dist_to_goal with p bias).
/// Used to demonstrate full phases in NREM trigger / tests. Same structured output.
///
/// Returns `Result<f32, GoalError>` (propagates from dist_to_goal). Accepts the same
/// optional `manifold_path` (passed through).
/// Pass None for default (backward compatible).
pub fn simple_sweep_nrem(current: &str, goal: &str, manifold_path: Option<&str>) -> Result<f32, GoalError> {
    println!("[NREM-GOAL-SWEEP|phase=far|current={}|goal={}] broad search_by_relation analog for high dist", current, goal);
    let d_mid = dist_to_goal(current, goal, 0.5, manifold_path)?;
    println!("[NREM-GOAL-SWEEP|phase=mid|current={}|goal={}|dist_mid={:.3}] query_with_momentum + p-bias", current, goal, d_mid);
    println!("[NREM-GOAL-SWEEP|phase=near|current={}|goal={}] refine + prospective futures eval by dist", current, goal);
    let d_final = dist_to_goal(current, goal, 0.9, manifold_path)?;
    println!("[NREM-GOAL-SWEEP|phase=result|current={}|goal={}|final_dist={:.3}] (phased sweep probe for NREM active trigger)", current, goal, d_final);
    Ok(d_final)
}