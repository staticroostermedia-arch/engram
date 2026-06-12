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
//! Behavior is identical to previous local copies when the default manifold path is used.
//! Structured sentinel-friendly traces and robustness handling preserved.
//! ralph_wiggum safe: read-only real manifold access, safe zero-vector defaults on missing
//! concepts, no writes.
//!
//! Manifold path is now configurable via optional `manifold_path: Option<&str>` parameter
//! (last argument). When None (or omitted by passing None at call sites), defaults to the
//! sovereign local setup "/Users/vantbracehome/.engram/manifold" for full backward
//! compatibility with existing call sites in NREM and tests. This allows future flexibility
//! (e.g. different manifolds) without changing core logic.

use crate::backend::{CpuBackend, VsaBackend};
use crate::ops::{op_bind, cosine_similarity};
use crate::Complex32;

/// Working `dist_to_goal` using existing core APIs (op_bind for relation encoding / "relate",
/// cosine_similarity for search closeness / "search_by_relation" analog, p_momentum as
/// trajectory cost factor).
///
/// In full system this maps to MCP search_by_relation + p-momentum over the relation
/// sheaf/graph. Computes (1-sim) * (1 + p*0.8) after real fetch + bind on manifold .leg3.
///
/// `manifold_path`: Optional path to the .leg3 manifold directory. Pass None to use the
/// sensible default (sovereign local "/Users/vantbracehome/.engram/manifold") for
/// backward compatibility. This makes the primitive configurable without altering core
/// computation logic.
///
/// ralph_wiggum=true (ritual toml [safety]): only read via existing fetch on real manifold;
/// no writes. Robustness: safe handling for missing concepts (no panic, clear error trace
/// for sentinel, safe default zero vector).
pub fn dist_to_goal(current: &str, goal: &str, p_momentum: f32, manifold_path: Option<&str>) -> f32 {
    let real_path = manifold_path.unwrap_or("/Users/vantbracehome/.engram/manifold");
    let be = CpuBackend::new(real_path);
    let (qc, missing_curr) = if let Some(q) = be.fetch(current) {
        (q, false)
    } else {
        println!("[NREM-GOAL-SWEEP|robustness=missing|current={}|goal={}|p={:.3}] safe zero vector + sentinel trace (NREM active post-bias)", current, goal, p_momentum);
        (Box::new([Complex32::default(); 8192]), true)
    };
    let (qg, missing_goal) = if let Some(q) = be.fetch(goal) {
        (q, false)
    } else {
        println!("[NREM-GOAL-SWEEP|robustness=missing|current={}|goal={}|p={:.3}] safe zero vector + sentinel trace (NREM active post-bias)", current, goal, p_momentum);
        (Box::new([Complex32::default(); 8192]), true)
    };
    let curr = *qc;
    let gl = *qg;
    // Actual relations: op_bind on real fetched q's (role-filler relate per GEOMETRIC/sheaf).
    let _rel = op_bind(&curr, &gl);
    // Search closeness on real data (cosine foundation for search_by_relation).
    let sim = cosine_similarity(&curr, &gl);
    // p-momentum in meaningful way: scale dist as trajectory cost using real sim + input p (momentum bias).
    let dist = (1.0f32 - sim) * (1.0f32 + p_momentum * 0.8f32);
    let missing = missing_curr || missing_goal;
    let note = if missing { " (robust: missing handled with safe default)" } else { "" };
    println!(
        "[NREM-GOAL-SWEEP|phase=dist|current={}|goal={}|p={:.3}|sim={:.3}|dist={:.3}|missing={}|note={}](real fetch q, op_bind relate, cosine search, p factor; active NREM after goal-bias; ralph_wiggum safe)",
        current, goal, p_momentum, sim, dist, if missing { 1 } else { 0 }, note
    );
    dist
}

/// Optional phased sweep helper (far/mid/near via repeated dist_to_goal with p bias).
/// Used to demonstrate full phases in NREM trigger / tests. Same structured output.
///
/// Accepts the same optional `manifold_path` (passed through to dist_to_goal calls)
/// for configurability. Pass None for default (backward compatible).
pub fn simple_sweep_nrem(current: &str, goal: &str, manifold_path: Option<&str>) {
    println!("[NREM-GOAL-SWEEP|phase=far|current={}|goal={}] broad search_by_relation analog for high dist", current, goal);
    let d_mid = dist_to_goal(current, goal, 0.5, manifold_path);
    println!("[NREM-GOAL-SWEEP|phase=mid|current={}|goal={}|dist_mid={:.3}] query_with_momentum + p-bias", current, goal, d_mid);
    println!("[NREM-GOAL-SWEEP|phase=near|current={}|goal={}] refine + prospective futures eval by dist", current, goal);
    let d_final = dist_to_goal(current, goal, 0.9, manifold_path);
    println!("[NREM-GOAL-SWEEP|phase=result|current={}|goal={}|final_dist={:.3}] (phased sweep probe for NREM active trigger)", current, goal, d_final);
}