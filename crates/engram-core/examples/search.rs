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
        println!("[{}] (crs={:.3}) -> {}", r.concept, r.score, &r.provlog.replace("\n", " ")[..std::cmp::min(100, r.provlog.len())]);
        if explain {
            println!("   ↳ EXPLAIN: {}", r.explain);
        }
    }
}
