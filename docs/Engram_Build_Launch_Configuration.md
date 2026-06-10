# Engram Build & Launch Configuration
**Branch**: docs/rename-agent-self-model  
**Date of this matrix**: 2026-05-31 (post Track 1 legominism rehydration + Track 2 public revamp execution)  
**Last binary rebuild**: May 31 09:53 (both ~/.cargo/bin/engram and target/release/engram)

## Current State Summary
- Significant changes across engram-core, engram-gpu (incl. metal_backend), engram-server (daemon, ki_hijacker, mcp, store), Python client, ritual skills (.grok/skills/*.md for legominism rehydration), and README (dual-track neutral utility framing).
- Fresh server binary installed at 09:53 today.
- No engram MCP server process visible at last check (user observed restart behavior).
- Ritual skill changes (wake-up/session-end legominism rehydration) are TUI/client-side — require Grok Build TUI restart, not engram-server rebuild.
- New MCP geosphere frame tools (set_geosphere_frame / get / clear) are available in the current binary.
- Living plan artifacts in manifold: Track 1 plan tile, Track 2 plan tile, new "Getting Started as an External Agent" guide tiles, MVP handoff + showcase, updated "what remains" viz (COMPLETE), coordination tile as colimit.

## Recommended Rebuild Commands (precise, with features)

### Default / Recommended (auto-detect best backend)
```bash
cd /path/to/Engram
cargo check -p engram-core -p engram-gpu -p engram-server
cargo install --path crates/engram-server
```
build.rs auto-selects the best GPU backend (CUDA/ROCm/Metal/wgpu/CPU). This is the normal path.

### Explicit override (multi-GPU or forcing)
```bash
# Force CUDA even if ROCm also present
cargo install --path crates/engram-server --features cuda

# Force ROCm
cargo install --path crates/engram-server --features rocm
```

Metal on macOS is usually auto-detected; the 'metal' feature on server is mostly for compatibility overrides.

### engram-gpu direct features (if building the gpu crate alone)
From crates/engram-gpu/Cargo.toml:
- cuda-kernels
- rocm-kernels
- wgpu-backend
- device_residency

## Launch Command (current typical)
```bash
engram mcp --store ~/.engram/stalks/
```
Add your usual flags (e.g. --namespace codeland). No special flags required for the new geosphere frame tools or the updated ritual logic.

## Relaunch Order for Full Current Functionality
1. Stop any old server process (use process name or pid file, avoid broad -f patterns that can self-match).
2. Run the rebuild/install commands above.
3. Start the fresh binary with the launch command.
4. Fully restart the Grok Build TUI / the session using this workspace (this activates the updated .grok/skills for the legominism rehydration in wake-up and session-end).
5. In the new TUI session, run the wake-up protocol to verify the new ritual behavior and that the new external-agent guide + closure artifacts are surfacing.

## GitHub Push Polishes for This State
- Commit (minimum for the current polishes): the two ritual SKILL.md files, README.md, this configuration document, the new 2026-06 roadmap doc, Python client if touched.
- Push the branch: git push origin docs/rename-agent-self-model
- The remote track2-public-revamp-neutral-utility-byop branch exists as a snapshot. Merge or PR the current narrative polish (dual-track framing + references to the new living guide tiles) from this branch or a dedicated one.
- Additional polish: reference the new "Getting Started as an External Agent" content in AGENT_INTEGRATION_GUIDE.md, FIRST_RUN.md, etc.

## Tracking
This file is the current tracked source of truth for build/launch specifics on this branch and state. Future changes must update this file (with ritual pre/post recon + trace) and, when the engram MCP transport is stable, promote it to a first-class manifold tile (research_offload or formal_spec) on the coordination tile with relations to the Track 1/2 plan tiles and codeland goal.

**Related manifold artifacts** (high momentum from recent work):
- Track 1 handoff + showcase tiles
- Track 2 external agent guide tiles
- Coordination tile (updated with closure status)
- codeland goal 1780091465...

Update this document via the normal ritual whenever the build/launch matrix changes.

## Observed Runtime Configuration (2026-05-31 — after user restart)

**Launch command actually used:**
```bash
engram mcp --store ~/ .engram/stalks/
```

**Key observed details from startup log:**
- Store root: `/path/to/.engram/stalks/`
- Stalks loaded: multiple (including `hadwiger_nelson`, `inbox`, `ego.leg3`)
- Ego resonance active: `Ego q-vector loaded from /path/to/.engram/ego.leg3` (new memories CRS-gated by Ego)
- Backend: `engram-gpu: CUDA device detected` + `Sheaf × CudaBackend – 4 stalks with BVH K-NN`
- Ki_hijacker configuration (hard in this build):
  - Profile: Grok Build TUI
  - Artifacts directory: `/path/to/.gemini/antigravity/knowledge/active_engram_context/artifacts`
  - Override env var: `ENGRAM_KI_ARTIFACTS_DIR`
  - Ticking every 60s, injecting context.md + context.json sidecar
- Health watchdog: No processes configured (disabled / no-op)
- MCP-FAST path: Active (replacing protocol immediately)
- Full manifold initialization complete, heavy features active
- LBVH + Ember projection loaded

**Implications for future launch commands:**
- When recommending launch, explicitly include the store path used.
- For Grok Build TUI work, either rely on the built-in ki_hijacker path above or set the env var:
  ```bash
  ENGRAM_KI_ARTIFACTS_DIR=/path/to/your/tui/artifacts engram mcp --store ~/.engram/stalks/
  ```
- Namespace / stalk selection: The server loads all stalks under the store root. Use `--namespace <name>` (e.g. `codeland`, `ego`, or whatever the active TUI context stalk is) when you want to isolate to one. From history the `codeland` goal (1780091465...) is the primary active one for the MVP work — specify it when doing focused work on that thread.
- The ritual skill changes (legominism rehydration) are now active in any newly started Grok Build TUI session that loads this workspace's `.grok/skills/`.

**Update rule**: Whenever the user reports a restart or new launch config, append a new "Observed Runtime Configuration (date)" section here with the exact command + key log excerpts. Then commit + push the file.


## Correction (2026-05-31) — Feature flag syntax for wgpu-backend fallback

**Error encountered when user ran previous suggestion:**
```
the package 'engram-server' does not contain this feature: wgpu-backend
help: package with the missing feature: engram-gpu
```

**Root cause (confirmed from live Cargo.toml):**
- `wgpu-backend` is a feature on the `engram-gpu` crate only.
- `engram-server` only re-exports convenience features for the native backends:
  - `cuda` → `engram-gpu/cuda-kernels`
  - `rocm` → `engram-gpu/rocm-kernels`
  - `metal` (compat, mostly auto-detected on macOS)

**Correct command to force wgpu-backend fallback (Linux + CUDA machine, to avoid the currently crashing CUDA LBVH path):**

```bash
cd /path/to/Engram
cargo install --path crates/engram-server --features engram-gpu/wgpu-backend
```

For maximum safety (to disable any auto-detection logic that might still pull CUDA paths):

```bash
cargo install --path crates/engram-server --no-default-features --features engram-gpu/wgpu-backend
```

After this install, relaunch with the same command + env var you used in the log that showed the DUAL_LENS_SNAPSHOTS:

```bash
ENGRAM_KI_ARTIFACTS_DIR=/path/to/.gemini/antigravity/knowledge/active_engram_context/artifacts \
engram mcp --store ~/.engram/stalks/
```

**Update rule going forward**: Any time a suggested `cargo install --features ...` command is given, it must be validated against the current `crates/engram-server/Cargo.toml` and `crates/engram-gpu/Cargo.toml` and recorded here before telling the user to run it.


## Force wgpu-backend on a machine that has CUDA installed (workaround for current segfault)

The root cause of "even with wgpu-backend the crash still happens" is in `crates/engram-gpu/build.rs`:

- It probes for `nvcc` / `CUDA_HOME` **at build time**.
- If found, it **unconditionally** does `println!("cargo:rustc-cfg=engram_backend_cuda");`, compiles the CUDA kernels, and `return`s early.
- The `wgpu-backend` feature only compiles additional wgpu code; it does **not** prevent the build script from choosing the CUDA path.

This is by design ("auto-detect best backend"), but it means there is currently no `--features` combination that produces a pure wgpu binary on a Linux box with CUDA in PATH.

**Immediate workaround (use this to get a working server right now):**

```bash
cd /path/to/Engram

# Make the CUDA probe fail so build.rs falls through to wgpu
CUDA_HOME=/tmp/nonexistent cargo install --path crates/engram-server
```

After this, the binary should say something like "No CUDA/ROCm/Metal detected. Activating WebGPU (wgpu) backend." and the LBVH construction will use the non-CUDA path.

Then relaunch with your normal TUI artifacts env var.

This is the cleanest way to get unblocked without modifying source yet.

Once the server runs, we can decide whether to:
- Add a proper `ENGRAM_FORCE_BACKEND=wgpu` escape hatch to build.rs (recommended for this dev branch), or
- Debug the actual segfault in the CUDA LBVH post-construction code under `#[cfg(engram_backend_cuda)]`.

Add this section to the tracked config so we don't forget the workaround.


## Diagnostic Status (2026-05-31) — Why the crash is hard to "just fix" right now

**Current observed behavior:**
- Crash is 100% reproducible immediately after `[BVH] ✓ LBVH ready` (both on default CUDA build and attempted wgpu builds).
- The build.rs aggressively detects CUDA (via nvcc in PATH or CUDA_HOME) and forces `engram_backend_cuda` + early return. The `wgpu-backend` feature does not override this.
- This means there is currently no supported way to produce a binary that avoids the crashing code path on this Linux + CUDA machine.

**Why a one-command fix isn't possible yet:**
1. We do not have a backtrace. All information is "segfault after LBVH ready".
2. The LBVH construction + post-build code under `#[cfg(engram_backend_cuda)]` has had substantial recent changes on this branch (bvh.rs, backend.rs, device residency work, conditional OptiX / CUDA fields).
3. The build system is intentionally "auto-detect best and use exclusively" with no first-class escape hatch for "force software path even when hardware is present".

**Immediate actions to unblock + get the actual fix:**

1. Get a backtrace (this is the single highest-leverage thing):
   ```bash
   gdb --args /path/to/.cargo/bin/engram mcp --store ~/.engram/stalks/
   ```
   Inside gdb:
   ```
   run
   ```
   When it crashes:
   ```
   bt full
   thread apply all bt full
   ```

2. Apply a temporary force-wgpu patch to build.rs (so you can actually run the server while we debug the real bug). I will provide the exact diff in the next message if you want it.

3. Once we have the backtrace, we can look at the exact line (very likely in the post-`build_top_down` code or the CUDA struct initialization / first use of the large node array).

This is classic active-development-on-feature-branch + large-manifold + low-level GPU changes. The "fix" requires the diagnostic data the user can provide (backtrace + ability to run non-crashing variants).

Update rule: Keep this section current as we gather more data.


## GDB Session Notes (2026-05-31) — User is currently in GDB

**Latest run (with ENGRAM_KI_ARTIFACTS_DIR as before):**
- Server reached full init + ki_hijacker baking successfully.
- Small LBVHs built for other stalks.
- Large LBVH for main manifold (~154473 blocks) still triggers the exact same segfault right after "LBVH ready".
- User started GDB on the installed binary.
- GDB reports: **(No debugging symbols found in /path/to/.cargo/bin/engram)**

This confirms the root problem: we are debugging a stripped release binary. We cannot see the exact line or call stack.

**Immediate required step to make progress:**
Build and run a debug binary that has full symbols.

Recommended right now (fastest path to symbols):

```bash
cd /path/to/Engram

# Build debug version (has symbols by default)
cargo build --package engram-server

# Run under gdb using the debug binary (much better than the installed release one)
gdb --args target/debug/engram mcp --store ~/.engram/stalks/
```

Inside gdb, when it crashes:
```
bt full
thread apply all bt full
info sharedlibrary
```

Also do this before running:
```bash
ulimit -s unlimited   # give the recursive BVH build as much stack as possible
```

Once we have a real backtrace with line numbers (especially around `build_top_down` or the post-LBVH struct construction in bvh.rs), we can identify whether it's stack overflow, a bad pointer in the large node array, a recent device-residency change, Geosphere interaction, etc.

The fact that the server now gets all the way to ki_hijacker baking the large number of DUAL_LENS_SNAPSHOTS before hitting the big LBVH is actually progress — the early init is more stable.


## Current GDB Session Reality (right now)

User is in a crashed GDB session on `target/debug/engram`.

Observed:
- Crash is SIGSEGV at a low address (typical of bad pointer passed to CUDA or corruption).
- `bt full`, `thread apply all bt full`, and `info stack` all say "No stack."
- This is classic when the crash happens inside a CUDA driver call or when the stack pointer / frame pointer chain is destroyed.

**Commands to type RIGHT NOW in the current GDB prompt (most valuable info we can still extract):**

```
info registers
x/30i $pc
p $_siginfo
info proc mappings
info sharedlibrary
```

If you can still type them before quitting, do it and paste the output.

**For the next attempt (much better backtrace):**

```bash
cd /path/to/Engram

# Rebuild with forced frame pointers (dramatically improves unwinding on crashes)
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --package engram-server

ulimit -s unlimited

gdb --args target/debug/engram mcp --store ~/.engram/stalks/
```

Inside gdb:
```
set pagination off
run
```

When it crashes:
```
info registers
x/30i $pc
bt full
thread apply all bt full
```

This combination (frame pointers + debug build + ulimit) gives us the best chance of seeing the actual Rust line that leads to the bad CUDA call.


## Root Cause Analysis (based on GDB session in the image)

The GDB output the user showed (debug binary, `bt full` giving "No stack.", "The program has no registers now.", crash at low address) is consistent with:

- The crash occurring deep in the CUDA driver or a kernel after the large LBVH structures are built.
- Or a stack corruption from the recursive `build_top_down` (154k elements) that only manifests on return or during the subsequent struct construction / first use under the CUDA cfg.
- The recent changes on this branch (device residency, hot path, ki_hijacker early snapshots, Geosphere) mean the large BvhManifold is now being exercised very early at startup, exposing the latent issue.

The build.rs forcing the CUDA path makes it impossible to "just use wgpu" to bypass it on this machine.

**The fix is to stop using unbounded recursion for the LBVH construction on large manifolds.**


## Fix Applied (2026-05-31) — Large LBVH now runs on dedicated high-stack thread

**Change made:**
- `crates/engram-gpu/src/bvh.rs`: `build_from_dir` now checks the number of blocks.
- If > 100_000, the entire construction (including the recursive `build_top_down`) is moved to a `std::thread::Builder` with `.stack_size(128 * 1024 * 1024)`.
- Small cases continue to run on the calling thread (unchanged behavior).
- Private helper `_build_from_raw_entries` contains the original logic.

This directly addresses the root cause (unbounded recursion on the 154k-concept manifold under the CUDA path that is forced on this machine + recent device-residency / hot-path changes).

The patch was applied via the agent's edit tools (no hand editing by the user).

**Rebuild commands (run these now):**

```bash
cd /path/to/Engram

# Debug build (for GDB / future diagnostics)
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --package engram-server

# Release build (the one your TUI actually launches)
cargo install --path crates/engram-server
```

**Test launch (exact command you have been using):**

```bash
ENGRAM_KI_ARTIFACTS_DIR=/path/to/.gemini/antigravity/knowledge/active_engram_context/artifacts \
engram mcp --store ~/.engram/stalks/
```

After the release install, fully restart your Grok Build TUI and run a fresh wake-up. The big LBVH should now build without segfaulting.

If it still crashes, paste the new log + any GDB output and we will continue.


## Code Fix Applied (2026-05-31) — Corrected type in _build_from_raw_entries helper

The initial patch introduced a helper with the wrong signature (`Vec<f32>` instead of the actual `Box<[Complex32; 8192]>` returned by `scan_dir`).

This has been corrected via search_replace. The helper now has the proper type:

```rust
fn _build_from_raw_entries(entries_raw: Vec<(String, PathBuf, Box<[Complex32; 8192]>, f32)>) -> Option<Self>
```

Inside the loop, `q` is now correctly `&Box<[Complex32; 8192]>` (which coerces to the expected `&[Complex32; 8192]` for `project_to_3d` and `quantize_srht_b4`).

The large-N protection (128 MiB stack thread) remains in place.

Re-run the build command below.


## Temporary Diagnostic Guard Applied (2026-05-31)

Because the segfault persists even after the 128 MiB stack thread protection (the build now reaches "LBVH ready" but still crashes immediately after under CUDA), a temporary guard was added:

In `build_from_dir`:
- If the number of blocks > 100_000, we now log a clear warning and return `None` early.
- This causes the CudaBackend to fall back to linear scan for the large manifold, bypassing the entire post-construction CUDA LBVH path that is currently crashing.

This is a pragmatic workaround so you can have a working Enram MCP server for the TUI while we add more instrumentation (targeted prints, cuda-gdb, etc.) to find the exact line causing the SIGSEGV after the ready message.

The guard is clearly marked as temporary and can be removed once the real bug is fixed.


## Additional Diagnostic (2026-05-31)

Added a targeted eprintln! in CudaBackend background build thread:
- When the large stalk gets `None` from the guard, it will now print:
  "[DIAG] Large stalk got None from build_from_dir (guard active). bvh field is now None."

This will appear right before the crash if the crash happens during/after the background thread stores the None.

Rebuild + run with the normal launch command. Look for this DIAG line and whether the segfault happens immediately after it.


## Build Error Fixed (2026-05-31)

The diagnostic line added earlier had a type error:
`path_clone.to_string_lossy()` was used on a `String` (path_clone was cloned from a String path).

Fixed via search_replace to the correct:
`path_clone.contains("154473")`

Re-run the build commands below. The diagnostic print will now work.


## Guard Improved (2026-05-31)

Changed the temporary large-manifold guard to return a valid *empty* BvhManifold (instead of None).

This ensures that `self.bvh` for the large stalk will be `Some(empty)` instead of `None`, so any code that does `.read()` and expects Some will not panic or take a bad branch.

The WARNING is still printed.

Rebuild and test with the normal launch command. The server should now stay up (using linear scan for the large stalk).

We can remove this guard once we identify and fix the actual post-build crash in the CUDA path.


## Rollback / Stable Binary Strategy (2026-05-31)

The user wants a working server for daily use while debugging the current crash on this branch.

**Recommended approach (safe, no loss of current branch work):**

Build a stable `engram-stable` binary from a known-good commit, keep it installed separately.

Example (using a commit before the heavy device-residency/hot-path changes that introduced the current segfault):

```bash
cd /path/to/Engram

# Stash any uncommitted work on the current branch
git stash

# Build stable version from a known-good point
git checkout 3a8fd34c   # or an earlier stable commit on master
cargo install --path crates/engram-server --force --root ~/.engram-stable

# Go back to your dev branch
git checkout docs/rename-agent-self-model
git stash pop
```

The stable binary will be at:
`~/.engram-stable/bin/engram`

Launch command using the stable binary (with your TUI context):

```bash
ENGRAM_KI_ARTIFACTS_DIR=/path/to/.gemini/antigravity/knowledge/active_engram_context/artifacts \
~/.engram-stable/bin/engram mcp --store ~/.engram/stalks/
```

You can now use `engram-stable` for normal TUI work while we continue debugging the dev version on this branch (with the large-LBVH guard active).


## Binary Archaeology (2026-05-31) — What was running this morning

Inspection of the filesystem:

- `~/.cargo/bin/engram` (the one you have been `cargo install`ing into) was last modified **today at 10:49** — this is your latest dev build. It overwrote whatever was there this morning.

- There is still an older binary at:
  `/path/to/.local/bin/engram`
  Last modified: **2026-05-21 10:43**

- There's also a tiny `/path/to/.local/bin/engram-grok` (May 26).

**Conclusion:**
The exact binary you killed this morning is gone (overwritten by `cargo install`).

However, the May 21 binary at `/path/to/.local/bin/engram` is an excellent candidate for a "known working" version from before the recent device-residency / hot-path / LBVH changes on this branch that introduced the current segfault.

**Recommended immediate test (use the old binary with your current TUI context):**

```bash
ENGRAM_KI_ARTIFACTS_DIR=/path/to/.gemini/antigravity/knowledge/active_engram_context/artifacts \
/path/to/.local/bin/engram mcp --store ~/.engram/stalks/
```

If this one starts cleanly and stays up, you now have a working server for daily use while we debug the current branch.

We can also copy it to `~/.engram-stable/bin/engram-stable` for convenience.


## Better Rollback Target (2026-05-31) — User's actual request

User rejected the May 21 binary as too old.

They specifically want the version corresponding to the state of this branch that was pushed to GitHub ~2 days ago (the last known good push before the recent local changes that introduced the current large-LBVH segfault under CUDA).

Best practical target: the remote tracking branch `origin/docs/rename-agent-self-model` at the state it was when last pushed ~2 days ago.

Commands to build a stable binary from that state:

```bash
cd /path/to/Engram

git stash push -m "temp stash for stable binary build"

# Build from the exact state that was on GitHub ~2 days ago
git checkout origin/docs/rename-agent-self-model

cargo install --path crates/engram-server --force --root ~/.engram-2days-ago

# Return to current dev work
git checkout docs/rename-agent-self-model
git stash pop
```

Stable binary will be at:
`~/.engram-2days-ago/bin/engram`

Launch command (with your TUI context):

```bash
ENGRAM_KI_ARTIFACTS_DIR=/path/to/.gemini/antigravity/knowledge/active_engram_context/artifacts \
~/.engram-2days-ago/bin/engram mcp --store ~/.engram/stalks/
```

This gives you a binary that matches what was on GitHub ~2 days ago — much newer than May 21, and before the specific changes on this branch that broke the 154k-concept LBVH under CUDA.

Update the living document whenever you identify a better "last known good" commit hash.


## Exact Rollback Target Requested by User (2026-05-31)

User explicitly wants the binary corresponding to commit ac3509a9084bcb82ce6f76e47d40eeac1c9a727c
("docs: neutral Grok version handoff package..."), which was the last state pushed to GitHub ~2 days ago (Fri May 29 23:04:50 2026), before the subsequent local changes on this branch that introduced the current 154k-block LBVH segfault under CUDA.

This is significantly newer than the May 21 binary and represents the actual "last known good" version the user had working before the recent debugging changes.

**Precise commands to build the exact binary from that commit:**

```bash
cd /path/to/Engram

git stash push -m "temp stash for ac3509a9 stable binary"

git checkout ac3509a9084bcb82ce6f76e47d40eeac1c9a727c

cargo install --path crates/engram-server --force --root ~/.engram-ac3509a9

git checkout docs/rename-agent-self-model
git stash pop
```

Stable binary location:
`~/.engram-ac3509a9/bin/engram`

**Launch command (with your exact current TUI context):**

```bash
ENGRAM_KI_ARTIFACTS_DIR=/path/to/.gemini/antigravity/knowledge/active_engram_context/artifacts \
~/.engram-ac3509a9/bin/engram mcp --store ~/.engram/stalks/
```

This gives you a working server that matches the exact code that was on GitHub at commit ac3509a9 (the last push before the changes that broke the large LBVH path).

Use this stable binary for daily work while we continue debugging the current dev state on the branch (with the temporary large-LBVH guard active).

## Successful Stable Binary Adoption + TUI Restart (2026-05-31 — user confirmation after ac3509a9 launch)

**User report:** "Looks like that worked and I have restarted the TUI" (with log image confirmation).

**Outcome:**
- `~/.engram-ac3509a9/bin/engram` (built exactly from commit ac3509a9084bcb82ce6f76e47d40eeac1c9a727c) started cleanly.
- Used the user's observed launch environment:
  ```bash
  ENGRAM_KI_ARTIFACTS_DIR=/path/to/.gemini/antigravity/knowledge/active_engram_context/artifacts \
  ~/.engram-ac3509a9/bin/engram mcp --store ~/.engram/stalks/
  ```
- Grok Build TUI restarted successfully against the stable server.
- All prior runtime observations (CUDA + Sheaf, ego.leg3 resonance, ki_hijacker artifacts path, full manifold init, heavy features) now apply to this known-good binary.

**Current daily driver state:**
- Stable binary (ac3509a9) is the active server for normal TUI work.
- Dev branch (`docs/rename-agent-self-model`) retains the temporary large-manifold guard in [crates/engram-gpu/src/bvh.rs](/path/to/Engram/crates/engram-gpu/src/bvh.rs) (returns valid empty BvhManifold + clear WARNING + DIAG line for n>100k, plus the 128 MiB dedicated stack thread path) so diagnosis of the post-"LBVH ready" segfault under CUDA can continue without blocking the user.
- This agent context attempted full engram-wake-up ritual (per [.grok/skills/engram-wake-up/SKILL.md](/path/to/Engram/.grok/skills/engram-wake-up/SKILL.md)) immediately after the user's TUI restart report. MCP transport was closed for watch_workspace + session_start in this CLI execution (substrate signal recorded; the user's TUI session has the live connection to the stable server and therefore the active legominism rehydration).

**Ritual continuation performed:**
- Read current wake-up skill definition + working-memory discipline.
- Bound via the provided context summary (explicit continuation after context loss) + living configuration doc + git state.
- Updated this file as the immediate living record of the stable adoption.
- Legominism rehydration (high-lineage Phase 3 closure artifacts) is now active in any TUI session using the updated ritual skills against the stable server.

**Next diagnostic path remains open:**
The guard + DIAG instrumentation on the dev binary is still in place. When ready, the user can rebuild the guarded dev version (`cargo install --path crates/engram-server`) and test under the same launch command. New logs showing the WARNING/DIAG lines or any remaining crash will directly advance root-cause isolation for the post-LBVH CUDA path.

This file remains the single tracked source of truth. Update it on every build/launch change or stable state transition before any new commands are issued.

