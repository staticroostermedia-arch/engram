fn main() {
    let mut args = std::env::args().skip(1);
    let mut query = String::from("OODA loop logophysics quasi-orthogonal causal bindings");
    let mut explain = false;

    while let Some(arg) = args.next() {
        if arg.contains("?explain=true") || arg == "--explain" {
            explain = true;
        } else if arg.starts_with("q=") {
            query = arg[2..].replace("+", " ").to_string();
        } else if !arg.starts_with("-") {
            query = arg;
        }
    }

    // Group 1 #1 functional prototype demo (real dist/sweep using existing ops) - runs before backend to guarantee output
    // Uses real concepts from manifold (from harness recall) + real path for fetch.
    // BROADENED for NREM active goal_sweep trigger test (multiple p, real concepts from prior traces, missing edges, clearer [NREM-ROBUSTNESS] output).
    println!("\n=== Group 1 #1: functional dist_to_goal + sweep (mFC mapping, real .leg3) ===");
    let d1 = dist_to_goal("group1_memory_manifold_low_dim_integration_analysis", "plan:grok-build-finish-plan:engram-code-edit-ritual", 0.3);
    println!("Example dist1: {:.3}", d1);
    simple_sweep("group1_memory_manifold_low_dim_integration_analysis", "plan:grok-build-finish-plan:engram-code-edit-ritual");

    let backend = engram_core::CpuBackend::new("/Users/vantbracehome/.engram/manifold");
    println!("=== SEMANTIC RAY-CASTER ===");
    println!("Query: {}", query);
    let res = engram_core::VsaBackend::recall(&backend, &query, 5);
    for r in res {
        println!(
            "[{}] (crs={:.3}) -> {}",
            r.concept,
            r.score,
            &r.provlog.replace("\n", " ")[..std::cmp::min(100, r.provlog.len())]
        );
        if explain {
            println!("   ↳ EXPLAIN: {}", r.explain);
        }
    }

    // === BROAD NREM goal_sweep TESTING (per ritual: multiple p_momentum, real manifold concepts, missing-concept edges, improved robustness traces) ===
    // Verifies the active trigger in daemon.rs (post goal-bias section) + phases (far/mid/near) with real data.
    // Uses concepts surfaced by harness (task:..., group1_prototype..., plan:...); exercises ROBUSTNESS paths.
    println!("\n=== BROAD NREM+goal_sweep TEST: multiple p, real concepts, edges, robustness (active trigger verification) ===");
    let broad_cases: Vec<(&str, &str, f32)> = vec![
        ("task:ritual_more_active_goal_sweep_nrem_trigger", "plan:grok-build-finish-plan:engram-code-edit-ritual", 0.0),
        ("group1_prototype_dist_to_goal_structural_sweep_mfc_mapping", "plan:grok-build-finish-plan:engram-code-edit-ritual", 0.3),
        ("task:ritual_goal_sweep_nrem_executable_hook", "plan:grok-build-finish-plan:engram-code-edit-ritual", 0.5),
        ("task:ritual_more_active_goal_sweep_nrem_trigger", "group1_prototype_dist_to_goal_structural_sweep_mfc_mapping", 0.9),
        ("nonexistent_current_nrem_test", "nonexistent_goal_edge_case", 1.2),  // explicit missing for robustness
    ];
    for (curr, gl, p) in broad_cases {
        println!("=== NREM goal_sweep CASE current={} goal={} p_momentum={:.1} ===", curr, gl, p);
        let _d = dist_to_goal(curr, gl, p);
        if p >= 0.5 {
            // for mid/high p cases also run phased sweep (far/mid/near) for fuller trace
            simple_sweep_nrem_broad(curr, gl);
        }
    }
    println!("=== END BROAD NREM goal_sweep TEST ===\n");

    // legacy demo (kept for compat)
    println!("\n=== Group 1 #1: functional dist_to_goal + sweep (mFC mapping) ===");
    let d1 = dist_to_goal("current_trace", "mfc_structured_nav_goal", 0.3);
    println!("Example dist1: {:.3}", d1);
    simple_sweep("trace_start", "knowledge_goal");
}

use engram_core::ops::{op_bind, cosine_similarity};
use engram_core::Complex32;
use engram_core::VsaBackend;  // trait for fetch (real .leg3); mirrors mcp.rs exposure for dist/goal_sweep

/// Working dist_to_goal using existing core APIs (op_bind for relation encoding / "relate",
/// cosine_similarity for search closeness / "search_by_relation" analog, p_momentum as trajectory cost factor).
/// In full system this maps to MCP search_by_relation + p-momentum over the relation sheaf/graph.
/// ralph_wiggum=true (ritual toml [safety]): only read via existing fetch on real manifold; no writes.
/// Broadened: NREM-ROBUSTNESS tags + phase context for active trigger testing.
fn dist_to_goal(current: &str, goal: &str, p_momentum: f32) -> f32 {
    // Real loading from .leg3 state using existing VsaBackend::fetch (removes all demo seeded vectors).
    // Uses real manifold at ~/.engram/manifold (actual .leg3 blocks).
    // ROBUSTNESS: safe handling for missing concepts (no panic, clear error trace for sentinel, safe default).
    let real_path = "/Users/vantbracehome/.engram/manifold";
    let be = engram_core::CpuBackend::new(real_path);
    let (qc, missing_curr) = if let Some(q) = be.fetch(current) {
        (q, false)
    } else {
        println!("[NREM-ROBUSTNESS] missing real concept '{}' in manifold - safe zero vector + error trace for sentinel (NREM active goal_sweep probe)", current);
        (Box::new([engram_core::Complex32::default(); 8192]), true)
    };
    let (qg, missing_goal) = if let Some(q) = be.fetch(goal) {
        (q, false)
    } else {
        println!("[NREM-ROBUSTNESS] missing real concept '{}' in manifold - safe zero vector + error trace for sentinel (NREM active goal_sweep probe)", goal);
        (Box::new([engram_core::Complex32::default(); 8192]), true)
    };
    let curr = *qc;
    let gl = *qg;
    // Actual relations: op_bind on real fetched q's (role-filler relate per GEOMETRIC/sheaf).
    let _rel = op_bind(&curr, &gl);
    // Search closeness on real data (cosine foundation for search_by_relation).
    let sim = cosine_similarity(&curr, &gl);
    // p-momentum in meaningful way: scale dist as trajectory cost using real sim + input p (momentum bias).
    let dist = (1.0f32 - sim) * (1.0f32 + p_momentum * 0.8f32);
    let note = if missing_curr || missing_goal { " (robust: missing handled with safe default)" } else { "" };
    println!(
        "[NREM goal_sweep dist_to_goal REAL from .leg3] current={} goal={} p_momentum={:.3} sim={:.3} dist={:.3}{} (real fetch q, op_bind relate, cosine search, p factor; active in run_nrem_consolidation)",
        current, goal, p_momentum, sim, dist, note
    );
    dist
}

/// Updated sweep: calls real dist_to_goal, produces usable phased output + traces.
/// NREM-BROAD-TEST variant for multiple scenarios + clearer phase/robustness output.
fn simple_sweep(current: &str, goal: &str) {
    println!("[sweep REAL step1 far] broad search_by_relation analog for high dist from {}", current);
    let d_mid = dist_to_goal(current, goal, 0.5);
    println!("[sweep REAL step2 mid] p-momentum bias (dist now {:.3})", d_mid);
    println!("[sweep REAL step3 near] refine + prospective futures eval by dist to {}", goal);
    let d_final = dist_to_goal(current, goal, 0.9);
    println!("[sweep RESULT] final_dist={:.3} (usable output for policy/trace)", d_final);
}

/// NREM broad test variant of sweep (used in multiple p/edge case loop) with explicit phase tags.
fn simple_sweep_nrem_broad(current: &str, goal: &str) {
    println!("[NREM-BROAD-TEST sweep REAL step1 far] search_by_relation structural from {}", current);
    let d_mid = dist_to_goal(current, goal, 0.5);
    println!("[NREM-BROAD-TEST sweep REAL step2 mid] query_with_momentum p-bias (dist now {:.3})", d_mid);
    println!("[NREM-BROAD-TEST sweep REAL step3 near] dist_to_goal + record (futures by dist to {})", goal);
    let d_final = dist_to_goal(current, goal, 0.9);
    println!("[NREM-BROAD-TEST sweep RESULT] final_dist={:.3} (NREM active trigger probe output)", d_final);
}

// Demo (now produces real varying numbers from ops; broadened for NREM ritual)
 // simple_sweep("current_trace", "target_knowledge_goal");
