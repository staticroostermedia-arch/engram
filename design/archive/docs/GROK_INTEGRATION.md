# Grok TUI + Engram Memory Integration

This document records the state of integrating the Engram geometric memory system
(the .leg/.leg3 HolographicBlock primitives + CudaBackend + BVH + OptiX path)
with this Grok 4.3 TUI instance.

## Philosophy (per the designer)

The .leg/.leg3 files are the primitives. They are self-contained, 256 KB,
cryptographically self-verifying, geometrically addressable units that fuse:
- 8192-D complex phase vector (q) + momentum (p)
- Thermodynamic health (CRS + full Logenergetics)
- Verbatim source (ProvLog)
- ZEDOS epistemic tag + allowed transforms contract
- BLAKE3 Merkle provenance chain

The memory system was built **for the agents** (Antigravity, Gemma variants, this Grok,
future agents) as much as for the human operator. The goal is sovereign,
non-hallucinating, longitudinally coherent artificial minds that do not wake up
blank every session.

## Current Integration (as of 2026-05-25)

- Global MCP server: `engram` via the dedicated wrapper `~/.local/bin/engram-grok`
- Wrapper sets the complete CUDA/OptiX/LD_LIBRARY_PATH environment and always
  targets the real manifold under `~/.engram/stalks/`.
- `~/.engram/sheaf.toml` declares a `primary` stalk at the flat location
  containing the ~149k real .leg primitives so that SheafBackend + CudaBackend
  actually see the data for O(log N) queries.
- The engram binary is (being) rebuilt from source in this environment with
  `OPTIX_SDK_PATH=/path/to/optix` and `ENGRAM_OPTIX_ENABLED=1` so the full
  intended CudaBackend + LBVH + OptiX RT-core path is compiled in for the
  RTX 5060 Ti (SM 12.0) hardware.

## How to Keep It Healthy

1. After any significant change to Engram source or the OptiX/CUDA stack,
   re-run the rebuild with the same environment:
   ```
   cd /path/to/Engram
   OPTIX_SDK_PATH=/path/to/optix ENGRAM_OPTIX_ENABLED=1 \
   cargo install --path crates/engram-server --force
   ```
   Then re-invoke `engram-grok` (or let Grok respawn the MCP server).

2. The `primary` stalk in sheaf.toml should continue to point at the directory
   that actually contains the bulk of the .leg files. If you reorganize data
   into named stalks, update the active stalk accordingly.

3. For direct CLI use (`engram-cli recall`, etc.) note that the CLI crate does
   not currently link `engram-gpu`. Use the MCP tools (via Grok or another
   client) or the `engram` server binary for accelerated paths.

4. When working inside `/path/to/Engram` or `/path/to/Documents/CodeLand`,
   the project-scoped `.grok/config.toml` files take precedence and already
   reference the overall integration.

## Related Files

- `~/.local/bin/engram-grok` — the launcher wrapper used by this Grok instance
- `~/.grok/config.toml` — MCP server definition
- `~/.engram/sheaf.toml` — stalk layout for the manifold
- `crates/engram-gpu/{build.rs,src/backend.rs,src/bvh.rs}` — the actual hardware acceleration
- `integrations/workflows/wake_up.md` — the canonical agent wake-up protocol that
  both Antigravity and this Grok are expected to follow when using the memory.

The underlying primitive is the right shape. Everything else (BVH, thermodynamics,
session discipline, ki_hijacker, OptiX RT cores) exists to make those 256 KB
sovereign geometric objects fast, trustworthy, and usable by long-lived agents.

— Grok (with the human)
