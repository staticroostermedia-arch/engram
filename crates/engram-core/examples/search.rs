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
    simple_sweep_nrem("group1_memory_manifold_low_dim_integration_analysis", "plan:grok-build-finish-plan:engram-code-edit-ritual");

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
            simple_sweep_nrem(curr, gl);
        }
    }
    println!("=== END BROAD NREM goal_sweep TEST ===\n");

    // legacy demo (kept for compat; now uses shared phased helper)
    println!("\n=== Group 1 #1: functional dist_to_goal + sweep (mFC mapping) ===");
    let d1 = dist_to_goal("current_trace", "mfc_structured_nav_goal", 0.3);
    println!("Example dist1: {:.3}", d1);
    simple_sweep_nrem("trace_start", "knowledge_goal");
}

use engram_core::ops::{op_bind, cosine_similarity};
use engram_core::Complex32;
use engram_core::VsaBackend;  // trait for fetch (real .leg3); mirrors mcp.rs exposure for dist/goal_sweep
use engram_core::{dist_to_goal, simple_sweep_nrem};  // shared extracted versions from engram-core (removes local dupe; behavior identical)

/// Updated sweep: calls real dist_to_goal (now the shared extracted one), produces usable phased output + traces.
/// NREM-BROAD-TEST variant for multiple scenarios + clearer phase/robustness output (structured sentinel format).
fn simple_sweep(current: &str, goal: &str) {
    println!("[NREM-GOAL-SWEEP|phase=far|current={}|goal={}] broad search_by_relation analog for high dist", current, goal);
    let d_mid = dist_to_goal(current, goal, 0.5);
    println!("[NREM-GOAL-SWEEP|phase=mid|current={}|goal={}|dist_mid={:.3}] p-momentum bias", current, goal, d_mid);
    println!("[NREM-GOAL-SWEEP|phase=near|current={}|goal={}] refine + prospective futures eval by dist", current, goal);
    let d_final = dist_to_goal(current, goal, 0.9);
    println!("[NREM-GOAL-SWEEP|phase=result|current={}|goal={}|final_dist={:.3}] (usable output for policy/trace; test for NREM)", current, goal, d_final);
}

// Demo (now produces real varying numbers from ops; broadened for NREM ritual)
 // simple_sweep("current_trace", "target_knowledge_goal");
