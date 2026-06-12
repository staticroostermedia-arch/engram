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

    let backend = engram_core::CpuBackend::new("/path/to/.engram/stalks/");
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
}

// === Group 1 #1 prototype sketch (mFC mapping from proposals doc) ===
// dist_to_goal + basic graph nav using existing relate/search_by_relation + p-momentum.
// This is a minimal demonstration only — no full navigation, no new modules.
// In real use: leverage VsaBackend + relation graph (search_by_relation / relate) with p as trajectory bias/cost.

fn dist_to_goal(current: &str, goal: &str, p_momentum: f64) -> f64 {
    // Sketch: traverse via search_by_relation (graph edges), bias/refine with p-momentum
    // (momentum as "progress toward goal" or path cost accumulator).
    // Real impl would call into core graph ops (ops.rs, lib.rs) or MCP search_by_relation
    // iteratively or via momentum-query, returning manifold dist (e.g. steps or phase dist).
    println!(
        "[dist_to_goal prototype] current={} goal={} p_momentum={:.3} (extend with relate/search_by_relation + p)",
        current, goal, p_momentum
    );
    // Placeholder (real: computed graph dist refined by momentum direction)
    42.0
}

// Simple "sweep" as 2-3 step phased multi-step evaluation (far -> near by dist-to-goal).
// Later can move to processes/*.toml or ritual toml as declarative steps.
// Phase 1 (far): broad search_by_relation for high-dist candidates.
// Phase 2 (mid): p-momentum bias to reduce dist.
// Phase 3 (near): refine + prospective "futures" eval (dist-to-goal as policy signal).

fn simple_sweep(current: &str, goal: &str) {
    println!("[sweep prototype] step1 far: broad relate/search for high dist from {}", current);
    let d_mid = dist_to_goal(current, goal, 0.5);
    println!("[sweep prototype] step2 mid: p-momentum bias (d={:.1})", d_mid);
    println!("[sweep prototype] step3 near: refine + evaluate futures by dist to {}", goal);
    let _final = dist_to_goal(current, goal, 0.9);
}

// Demo call (uncomment in real run to exercise)
 // simple_sweep("current_trace", "target_knowledge_goal");
