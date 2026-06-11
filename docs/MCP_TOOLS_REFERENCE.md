# MCP Tools Reference

Engram exposes **66 MCP tools** (62 `mcp_engram_*` + 4 linguistic). Most agents should use **8** — see [`AGENT_MEMORY_CONTRACT.md`](AGENT_MEMORY_CONTRACT.md).

**Decision map (all 66):** [`TOOL_DECISION_MAP.md`](TOOL_DECISION_MAP.md) — when to escalate to `update`, `query_with_momentum`, `search_by_relation`, goals, tiles, linguistic tools.

Tools are grouped by tier:

| Tier | Count | Use when |
|------|-------|----------|
| **Essential** | 8 | Every Grok Build / agent session (lean default) |
| **Power** | ~30 | Deep mode, meta-work, goals, tiles, verification |
| **Lean: avoid** | ~12 | Harmful or redundant on 100k+ stores in lean mode |
| **Specialist** | rest | Geosphere, scout, import/export, protocols |

---

## Tier 1 — Essential (8 tools)

| Tool | Purpose |
|------|---------|
| `mcp_engram_session_start` | **Wake.** Inline JSON: `continuation`, `readiness`, optional `spatial`. Args: `intent`, `include_spatial?`, `spatial_max_files?` |
| `mcp_engram_context_for_edit` | **Pre-edit.** Spatial AST + related traces for one file. Args: `path`, `line_start?`, `line_end?`, `auto_ingest?` |
| `mcp_engram_recall` | **Read.** Args: `query`, `k?`, `scope?` (`anchors`\|`hot`\|`all`) |
| `mcp_engram_quick_trace` | **Fork.** Low-friction `trace:*`. Args: `decision`, `why`, + optional chain fields |
| `mcp_engram_remember` | **Write new.** Args: `concept`, `text`. Recall first; if match >0.85 use `update` |
| `mcp_engram_session_end` | **Handoff.** Structured JSON + `helper:session_handoff_latest`. Args: `summary` |
| `mcp_engram_get_backend_readiness` | **Status.** `memory_mode`, `recall_mode`, `bvh_ready`, block count |
| `mcp_engram_set_memory_mode` | **Mode.** `lean` (default) or `deep` (full recall, auto-BVH on large stores) |

### Lean session loop

```
session_start → [work: context_for_edit + recall(scope=anchors) + quick_trace + remember] → session_end
```

---

## Tier 2 — Power (use deliberately)

### Memory writes & evolution
- `update` — **preferred** over forget+remember (Lyapunov drift)
- `batch_remember`, `pin`, `forget`, `forget_old`
- `remember_solution` — crystallize working fixes to praxis
- `record_reasoning_trace` — full A/D/R trace (use `quick_trace` for daily work)
- `scar` — repulsion for dead-ends / doom loops

### Goals
- `goal_create`, `goal_set_primary`, `goal_list`, `goal_status`, `goal_update_status`
- `goal_decompose`, `goal_get_children`, `goal_search`

### Graph / sheaf
- `relate`, `relate_batch`, `search_by_relation`, `visualize`

### Thought tiles (meta-work arcs)
- `thought_tile_create`, `thought_tile_write_result`, `thought_tile_create_visualization`
- `promote_hot`, `promote_hot_batch`

### Verification & health
- `verify_manifold_integrity`, `verify_block_lawfulness`, `verify_behavior`
- `genesis`, `spatial_status`, `stats`, `summarize`, `recall_recent`

### Deep recall & discovery
- `query_pure` — geo K-NN on hot set
- `query_with_momentum` — q+p blend (80/20)
- `read_concept` — full untruncated block body
- `get_continuation_bundle` — TUI compression boundary only (redundant at wake)

### Spatial (legacy split — prefer `context_for_edit`)
- `context_for_file`, `recall_in_file`
- `incremental_spatial_ingest`, `force_spatial_ingest`

### Session / handoff extras
- `rebuild_bvh` — on-demand full index (deep mode; RAM/time cost)

### Other power
- `list_concepts`, `list_namespaces`, `set_namespace`
- `export`, `import`, `scout`, `invoke_protocol`, `track_user`

---

## Tier 3 — Lean: avoid (unless explicit need)

| Tool | Why |
|------|-----|
| `watch_workspace` | Full-repo watcher; use `context_for_edit` + `incremental_spatial_ingest` instead. Deferred by `ENGRAM_DEFER_WATCH_INGEST=1` |
| `rebuild_bvh` | 10–40GB RAM on 100k+ blocks unless intentional deep mode |
| `list` / `list_concepts` | O(n) store scan |
| `summarize` at wake | Duplicates inline `session_start` bundle |
| `get_continuation_bundle` at wake | Redundant with inline bundle |
| `query_with_momentum` at wake | Use `recall(scope=anchors)` first |
| `force_spatial_ingest` (full tree) | Use `include_spatial` on session_start or single-file ingest |

---

## Tier 4 — Specialist

### Geosphere (frame/lens)
- `set_geosphere_frame`, `get_geosphere_frame`, `clear_geosphere_frame`

---

## Environment variables (MCP defaults)

| Variable | Default (lean) | Effect |
|----------|----------------|--------|
| `ENGRAM_MEMORY_MODE` | `lean` | Anchor-first recall; no auto-BVH |
| `ENGRAM_DEFER_BVH` | `1` | Skip background BVH build |
| `ENGRAM_DEFER_WATCH_INGEST` | `1` | No recursive watch/ingest on bind |
| `ENGRAM_DISABLE_SHEAF` | `1` | Single backend on `--store` |
| `ENGRAM_OPTIX_ENABLED` | `0` | Skip OptiX PTX path in MCP |
| `ENGRAM_KI_DISABLE` | `1` | Skip ki bake loop on large stores |

---

## Process sheaf (automatic)

`session_start` loads `processes/*.toml` (wake-up, session-end, subvisor, etc.). Agents do not call these directly.

---

## Linguistic Calculus (full P1–P5 surface; MCP exposure complete Phase 6)

All phases wired additively in `tool_list()` + `dispatch`/`handle_tool_call` (mcp.rs) with inputSchema + result (crs + bundle/phase/equiv).

- **P1 primitives** (core): `Leg3Pointer::mint_linguistic` (ZEDOS_LINGUISTIC 0x4C / POLY 0x4D / FIBERED 0x4E), LinguisticWord/ContextPatch/DiscourseBundle in types (payload in q + functor meta); no core layout change.
- **P2 sheaf**: `processes/linguistic/linguistic-calculus.toml` + `fibered-equivalence.toml` (sheaf_role, h1_handler=OP_GEOMETRIC_PRODUCT, invariants); loaded by `load_process_sheaf`.
- **P3 functor ops** (exposed): `mcp_compress_linguistic` (bundle → phase/payload crs+preview), `mcp_decompress_linguistic` (phase/bundle → bundle, homotopy CRS roundtrip), `mcp_fibered_linguistic_equivalence` (bundle_a/b → crs equiv via VSA/cos).
- **P4 calculus** (exposed): `mcp_linguistic_calculus` (bundle + operation:"differentiate"|"integrate"|"operadic_compose" + optional path_bundles/morphisms; returns crs + result bundle/phase; mints ZEDOS_TRAINING + NREM relates to ritual:nrem + goal + sheaf process).
- **P5 ritual**: exposure via `mcp_engram_session_start` (loads `ritual/ritual_linguistic_wake.toml` + `nrem-consolidation.toml` from processes/ritual + linguistic/); calculus mcp + remember/relate/promote/verify for NREM/ego.leg3 promotion (crs>=0.85 gate, fibered homotopy, produces ego.leg3 + linguistic high-crs bundles). See invariants in tomls (class mixing scar, lyapunov).

Full e2e (mint→compress→differentiate→operadic→decompress→NREM→ego.leg3 roundtrip CRS>=0.85 + homotopy/text-coeff fidelity) in mcp.rs tests + hygiene. Used by P5 rituals (wake/NREM). See RITUALS.md §Phase5, processes/ritual/ritual_linguistic_wake.toml.

**Phase 6 public polish complete (Sub-agent 1, 3-iter plan/edit/verify loop):** Added dedicated `docs/CATEGORICAL_LINGUISTIC_CALCULUS.md` (full geometric/sheaf/ritual/P1-6 story + 3 bridging exs with CRS numbers + how to try + links); prominent README section after Memory Model (mcp_linguistic_calculus ex + mixed scale/bridge + roundtrip NREM/ego + “How to try” + hygiene + link); additive updates to RITUALS.md (new Phase 6 section), this ref, GITHUB_MVP_PREP_PLAN.md (exec log with sub_id=019eb27f-d94f-7ea0-b580-36adb41c19c8, hygiene verbatim from cargo test 23+ok / check / target/debug/engram 0.4.0 + ls / verify, CRS 0.85+, iter details, readiness). All e2e tests green, manifold ready (verify high-CRS sampled; prior gates 0.85+ in mixed lifecycle), public sharing prepared on feat/mvp-github-prep-2026-06. No core changes; invariants held (.leg3/CRS/sheaf). Dogfooded via session_start/recall/context/quick_trace + final remember/relate to goal:mvp_gap_closure_v1. Narrow sub calls. See CATEGORICAL_LINGUISTIC_CALCULUS.md + RITUALS Phase 5/6 for full.

---

## Related

- [`AGENT_MEMORY_CONTRACT.md`](AGENT_MEMORY_CONTRACT.md) — **start here**
- [`GROK_BUILD_MEMORY.md`](GROK_BUILD_MEMORY.md) — Grok Build integration pitch
- [`RITUALS.md`](RITUALS.md) — full ritual philosophy
- [`GEOMETRIC_MEMORY.md`](GEOMETRIC_MEMORY.md) — substrate theory
- `crates/engram-server/src/mcp.rs` — tool implementations