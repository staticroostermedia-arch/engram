//! Verification plan entry: `cargo test -p engram-server --test store -- --quiet`
//!
//! Canonical implementation lives in `src/store.rs` unit tests; this integration
//! target exists so the plan-named `--test store` command resolves on the bin crate.

#[test]
fn build_continuation_bundle_emits_injection_observables() {
    use std::process::Command;

    // Scope to bin unit tests only — unscoped filter also matches this integration test (infinite loop).
    let status = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "test",
            "-p",
            "engram-server",
            "--bin",
            "engram",
            "build_continuation_bundle_emits_injection_observables",
            "--",
            "--quiet",
        ])
        .status()
        .expect("spawn cargo test for canonical store.rs unit test");

    assert!(
        status.success(),
        "canonical build_continuation_bundle_emits_injection_observables failed"
    );
}