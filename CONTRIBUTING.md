# Contributing to Engram

We welcome contributions to Engram. Since this is a hardware-native memory engine with a strict binary format, there are a few rules to follow to keep the physics correct.

## Quick checklist (before you open a PR)

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes (CI enforces this)
- [ ] `cargo fmt -- --check` passes
- [ ] No changes to the fixed 256KB `HolographicBlock` layout without a version bump + migration
- [ ] README / `docs/MCP_TOOLS_REFERENCE.md` updated if you add or rename MCP tools
- [ ] Use `update` on existing memories — never `forget` + `remember` for the same concept

---

## Development Setup

```bash
git clone https://github.com/staticroostermedia-arch/engram.git
cd engram

# Build everything
cargo build --workspace

# Run all tests
cargo test --workspace

# Run clippy (required before PR)
cargo clippy --workspace -- -D warnings
```

---

## Architecture Overview

The workspace has four crates:

| Crate | Role |
|---|---|
| `engram-core` | The HolographicBlock format, VSA operators (OP_ADD, OP_BIND), BLAKE3 Merkle chain, CRS/ADR physics |
| `engram-server` | MCP server, background daemon (file watcher + NREM consolidation + health watchdog), REST API |
| `engram-cli` | CLI binary — wraps `engram-core` for direct manifold management |
| `engram-gpu` | CUDA/ROCm/Metal/WebGPU backends for parallel ANN search |

---

## Critical Rules

### 1. Never Break the `.leg3` Format
The `HolographicBlock` struct in `engram-core/src/lib.rs` is a **fixed 262,144-byte C-struct**. Fields are at fixed byte offsets. Any change that alters struct layout will silently corrupt every existing manifold on disk. Changes to this struct require a format version bump and a migration tool.

### 2. Use `mcp_engram_update` — Never `forget` + `remember`
When modifying an existing memory block, always use the `update` path. `forget` + `remember` destroys the block's Lyapunov drift history (Merkle chain, CRS trajectory, ADR state). The `update` path preserves this history and applies a stability check before accepting the new content.

### 3. VSA Operator Correctness
`OP_ADD` is commutative superposition. `OP_BIND` is Hadamard product (invertible, non-commutative when combined with `OP_SHIFT`). Do not use scalar multiplication in place of `OP_INVERT`. See `crates/engram-core/src/vsa.rs` for the canonical implementations.

### 4. CRS Is Not a User-Settable Field
The Coherence-Reliability Score is computed entirely by the ADR thermodynamic gate from the block's Lyapunov drift. Do not set it manually outside of `pin()` (which locks it at 1.0) or the genesis seeding path.

### 5. The Daemon Has Three Loops — Don't Break Any of Them
`crates/engram-server/src/daemon.rs` runs three independent async loops:
- **File Watcher** — inotify/fsevents integration for live AST re-ingestion
- **NREM Consolidation** — periodic ego narrative tensor compression
- **Health Watchdog** — process monitoring with Agency Proposal minting

Contributions to the daemon must not block any of these loops. Use `tokio::spawn` for any I/O-bound work.

---

## Adding a New MCP Tool

1. Add the tool's JSON schema definition to the `tools/list` response in `crates/engram-server/src/mcp.rs`
2. Add the handler arm in the `match tool_name` block in the same file
3. Add the tool to the MCP Tools Reference table in `README.md` with an accurate description
4. Update the tool count in the README header (`## MCP Tools Reference (N Tools)`)
5. Add a test in `crates/engram-server/src/mcp.rs` or a separate integration test

---

## Pull Request Checklist

- [ ] `cargo clippy --workspace -- -D warnings` passes with no new warnings
- [ ] `cargo test --workspace` passes
- [ ] No changes to the fixed byte layout of `HolographicBlock`
- [ ] README tool count and table updated if new tools were added
- [ ] FIRST_RUN.md updated if the setup flow changed
- [ ] No blocking calls in async daemon loops

---

## What We're Looking For

- **GPU backends:** ROCm and Metal backends are functional but less battle-tested than CUDA. Improvements welcome.
- **Tree-Sitter language coverage:** We currently parse Rust, Python, TypeScript, JavaScript, Go, Java, C, C++. Adding more languages is straightforward — see `crates/engram-core/src/ingest/ast.rs`.
- **Embedding server compatibility:** Currently tested against llama.cpp and ONNX-hosted nomic-embed. Other OpenAI-compatible endpoints should work but haven't been verified.
- **WebGPU backend:** The Poincaré hyperbolic INT8 search backend is production-ready but the WebGPU transport layer has known latency issues on some platforms.

---

## Contributor / operator docs (not in README hero)

These are for substrate builders and long-running operator setups:

| Doc | Purpose |
|-----|---------|
| [docs/SUBSTRATE_WINS_PLAN.md](docs/SUBSTRATE_WINS_PLAN.md) | Harness injection program (shipped; historical record) |
| [docs/HARNESS_INJECTION.md](docs/HARNESS_INJECTION.md) | Current wake injection contract |
| [docs/LONG_SLEEP_WAKEUP_PROTOCOL.md](docs/LONG_SLEEP_WAKEUP_PROTOCOL.md) | Cold-boot / long-sleep wake |
| [docs/AGENTIC_FIRST_LONG_SLEEP_SUBSTRATE.md](docs/AGENTIC_FIRST_LONG_SLEEP_SUBSTRATE.md) | Agentic-first substrate notes |
| [docs/long_sleep_verification_suite_design.md](docs/long_sleep_verification_suite_design.md) | Verification suite design |
| [docs/internal/real_usage_testing_guide.md](docs/internal/real_usage_testing_guide.md) | Real-usage testing (maintainer) |
| [processes/meta/](processes/meta/) | Advanced `/loop` workflow playbooks (not sheaf-loaded) |

---

*Engram is developed by Aric Goodman and Static Rooster Media. Patent Pending US19/372,256. Licensed under AGPL-3.0-only.*
