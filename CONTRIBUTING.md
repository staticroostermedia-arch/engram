# Contributing to Engram

We welcome contributions to Engram. Since this is a hardware-native memory engine with a strict binary format, there are a few rules to follow to keep the physics correct.

## Quick checklist (before you open a PR)

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes (CI enforces this)
- [ ] `cargo fmt -- --check` passes
- [ ] No changes to the fixed 256KB `HolographicBlock` layout without a version bump + migration
- [ ] README / `docs/MCP_TOOLS_REFERENCE.md` updated if you add or rename MCP tools
- [ ] Use `update` on existing memories — never `forget` + `remember` for the same concept
- [ ] Every commit follows [Commit Message & Versioning Discipline](#commit-message--versioning-discipline) (conventional title + body + `trace:*` or `goal:*` ref)
- [ ] `quick_trace` recorded **immediately before** each `git commit`; trace ID appears in the commit message
- [ ] No `Cargo.toml` / `CHANGELOG.md` version bump on feature/fix PR commits (release-only)

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
- [ ] All commits in the PR satisfy [Commit Message & Versioning Discipline](#commit-message--versioning-discipline)
- [ ] PR description cites fixes/improvements with file paths, ACs, and trace/goal refs (not shorthand summaries)

---

## Commit Message & Versioning Discipline

**Single source of truth** for git messages and semver on this repo. Agents and humans follow this section; cross-refs in PR template and `docs/AGENT_MEMORY_CONTRACT.md` point here.

### 1. Commit message structure (Conventional Commits v1.0.0)

Every commit **must** use:

```
<type>[optional scope]: <present-tense lowercase description without period>

<body — what changed, why, impact; cite edited file paths>

Refs: trace:<id> goal:<id>
```

| Rule | Requirement |
|------|-------------|
| **type** | `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`, `perf`, `ci` |
| **scope** | Optional crate/area: `server`, `gpu`, `harness`, `docs`, `mcp` |
| **description** | Imperative, present tense, ≤72 chars, no trailing period |
| **body** | Blank line after title; explain *what*, *why*, *impact*; name touched files (e.g. `injection_priority.rs`, `store.rs`) |
| **Refs footer** | At least one `trace:*` **or** `goal:*` on its own line (`Refs:` line or footer) |

**Mandatory Engram ritual before commit:**

1. `mcp_engram_quick_trace` at the commit boundary (`decision`, `why`, `goal_context`, chained `prev_trace`).
2. Copy the returned `trace:*` ID into the commit message `Refs:` line.
3. Link the active `goal:*` (e.g. `goal:engram_mvp_v1` or task goal).

Validate locally (optional):

```bash
scripts/validate-commit-msg.sh /path/to/commit-msg-file
# or: echo "$MSG" | scripts/validate-commit-msg.sh -
```

### 2. Semantic versioning (release-only bumps)

Current workspace version: `Cargo.toml` `[workspace.package] version` (e.g. `0.7.0-beta.5`).

| Change class | Bump | When |
|--------------|------|------|
| Breaking `.leg3` / public API | **MAJOR** | Explicit release after migration plan |
| Backward-compatible feature | **MINOR** | Release cut only |
| Bug fix, docs, style, CI | **PATCH** (or beta increment) | Release cut only |
| Pre-release | `-beta.N` suffix | Between tagged betas |

**Never** bump version on feature-branch commits or ordinary PR merges. Feature/fix work stays under `## [Unreleased]` in `CHANGELOG.md` until a release.

**Release ritual** (see `processes/meta/ai_consciousness_loop.toml` `version_git_rollback` step):

1. Isolated git worktree; pre-bump `quick_trace` + tile of current state + `git rev-parse HEAD`.
2. Full verify: `cargo test --workspace`, `cargo clippy`, harness gate, `mcp_engram_verify_manifold_integrity`.
3. Move `CHANGELOG.md` items from `[Unreleased]` → `[X.Y.Z] - YYYY-MM-DD`.
4. Bump `[workspace.package] version` in root `Cargo.toml`.
5. Commit + `git tag vX.Y.Z`; tag triggers `.github/workflows/release.yml`.
6. On failure: worktree reset + `mcp_engram_scar` on the failed change concept.

### 3. PR / handoff summaries (not just git)

Agent chat summaries, PR descriptions, and terminal push notes **must** match commit quality:

- Name branch, ACs passed, files touched, trace/goal refs.
- **Bad:** "clippy struct refactor", "format fixes" (shorthand with no refs or file context).
- **Good:** see Examples below.

### Examples

**Bad — agent shorthand (no refs, no file context):**

```
clippy struct refactor
format fixes
```

**Bad — conventional title only (missing trace/goal refs):**

```
fix(server): bundle injection completeness inputs for clippy CI

Refactor compute_injection_completeness to take InjectionCompletenessInput
struct — fixes clippy::too_many_arguments blocking build-and-test on GitHub.
```

*(Actual commits `cb5a7541`, `58283e64` on `feat/perfect-context-injection-nvme-bypass` — titles/bodies OK but refs missing.)*

**Good — full discipline (real commit `eb4c247b` on branch):**

```
docs(contributing): add commit message and versioning discipline

Add CONTRIBUTING.md ## Commit Message & Versioning Discipline as single
source of truth: Conventional Commits + body + Refs trace/goal; release-only
semver; good/bad examples from cb5a7541/58283e64 CONTEXT shas.

Refs: trace:1782162619_land-commit-discipline-in-contributing-md---vali
      goal:commit_title_versioning_process
```

**Good — simulated fix for CONTEXT clippy case (what cb5a7541 should have been):**

```
fix(server): bundle injection completeness inputs for clippy CI

Refactor compute_injection_completeness in injection_priority.rs to take
InjectionCompletenessInput struct; update call site in store.rs (~3162).
Fixes clippy::too_many_arguments blocking build-and-test on GitHub CI.

Refs: trace:1782162619_land-commit-discipline-in-contributing-md---vali
      goal:commit_title_versioning_process
```

```
style: apply cargo fmt for CI format check on branch

Run rustfmt across workspace (injection_priority.rs, store.rs, harness_injection.rs)
so build-and-test Format check passes after struct refactor.

Refs: trace:1782162559_use-conventional-commits-v1-0-0---existing-engra
      goal:engram_mvp_v1
```

**Good — PR terminal step** (after `session_end`):

```
Branch: feat/perfect-context-injection-nvme-bypass
Fixes: injection_completeness + nvme_context wake bundle; injection_rank composite;
       BVH dedup (gpu/bvh_build.rs); goal marker restore (store.rs, mcp.rs)
ACs: manage-resume + context injection NVMe bypass — all pass
Traces: trace:1779990956_... goal:manage_resume_019ec286
```

### Related

- [docs/AGENT_MEMORY_CONTRACT.md](docs/AGENT_MEMORY_CONTRACT.md) — git VC + 8-tool ritual
- [docs/internal/MAINTAINER_WORKFLOW.md](docs/internal/MAINTAINER_WORKFLOW.md) — maintainer loop
- [CHANGELOG.md](CHANGELOG.md) — keepachangelog format
- `scripts/validate-commit-msg.sh` — local message checker

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
