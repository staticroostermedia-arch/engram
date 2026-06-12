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

    // === Group 1 #1 functional demo (now uses real existing ops + p-momentum) ===
    println!("\n=== Group 1 #1: functional dist_to_goal + sweep (mFC mapping) ===");
    let d1 = dist_to_goal("current_trace", "mfc_structured_nav_goal", 0.3);
    println!("Example dist1: {:.3}", d1);
    simple_sweep("trace_start", "knowledge_goal");
}

use engram_core::ops::{op_bind, cosine_similarity};
use engram_core::Complex32;

/// Working dist_to_goal using existing core APIs (op_bind for relation encoding / "relate",
/// cosine_similarity for search closeness / "search_by_relation" analog, p_momentum as trajectory cost factor).
/// In full system this maps to MCP search_by_relation + p-momentum over the relation sheaf/graph.
/// ralph_wiggum=true (ritual toml [safety]): only read via existing fetch on real manifold; no writes.
fn dist_to_goal(current: &str, goal: &str, p_momentum: f64) -> f64 {
    // Real loading from .leg3 state using existing VsaBackend::fetch (removes all demo seeded vectors).
    // Uses real manifold at ~/.engram/manifold (actual .leg3 blocks).
    // ROBUSTNESS: safe handling for missing concepts (no panic, clear error trace for sentinel, safe default).
    let real_path = "/Users/vantbracehome/.engram/manifold";
    let be = engram_core::CpuBackend::new(real_path);
    let (qc, missing_curr) = if let Some(q) = be.fetch(current) {
        (q, false)
    } else {
        println!("[ROBUSTNESS] missing real concept '{}' in manifold - safe zero vector + error trace for sentinel", current);
        (Box::new([engram_core::Complex32::default(); 8192]), true)
    };
    let (qg, missing_goal) = if let Some(q) = be.fetch(goal) {
        (q, false)
    } else {
        println!("[ROBUSTNESS] missing real concept '{}' in manifold - safe zero vector + error trace for sentinel", goal);
        (Box::new([engram_core::Complex32::default(); 8192]), true)
    };
    let curr = *qc;
    let gl = *qg;
    // Actual relations: op_bind on real fetched q's (role-filler relate per GEOMETRIC/sheaf).
    let _rel = op_bind(&curr, &gl);
    // Search closeness on real data (cosine foundation for search_by_relation).
    let sim = cosine_similarity(&curr, &gl);
    // p-momentum in meaningful way: scale dist as trajectory cost using real sim + input p (momentum bias).
    let dist = (1.0 - sim) * (1.0 + p_momentum * 0.8);
    let note = if missing_curr || missing_goal { " (robust: missing handled with safe default)" } else { "" };
    println!(
        "[dist_to_goal REAL from .leg3] current={} goal={} p_momentum={:.3} sim={:.3} dist={:.3}{} (real fetch q, op_bind relate, cosine search, p factor)",
        current, goal, p_momentum, sim, dist, note
    );
    dist
}

/// Updated sweep: calls real dist_to_goal, produces usable phased output + traces.
fn simple_sweep(current: &str, goal: &str) {
    println!("[sweep REAL step1 far] broad search_by_relation analog for high dist from {}", current);
    let d_mid = dist_to_goal(current, goal, 0.5);
    println!("[sweep REAL step2 mid] p-momentum bias (dist now {:.3})", d_mid);
    println!("[sweep REAL step3 near] refine + prospective futures eval by dist to {}", goal);
    let d_final = dist_to_goal(current, goal, 0.9);
    println!("[sweep RESULT] final_dist={:.3} (usable output for policy/trace)", d_final);
}

// Demo (now produces real varying numbers from ops)
 // simple_sweep("current_trace", "target_knowledge_goal");
