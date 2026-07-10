# MCP Tools Reference

Engram exposes **85 MCP tools** (81 `mcp_engram_*` + 4 linguistic) as of the `tool_list()` source of truth in `crates/engram-server/src/mcp.rs` (includes `mcp_engram_lexicon_mint_word`). Most agents should use **8** — see [`AGENT_MEMORY_CONTRACT.md`](AGENT_MEMORY_CONTRACT.md). For edit/update fidelity, prefer **safe composites** (Tier 2): `mcp_engram_safe_edit_and_verify`, `mcp_engram_update_with_tensor_bond`.

**Decision map:** [`TOOL_DECISION_MAP.md`](TOOL_DECISION_MAP.md) — when to escalate to `update`, `query_with_momentum`, `search_by_relation`, goals, tiles, linguistic tools. **JIT deformation:** [`DEFORMATION_PLAYBOOKS.md`](DEFORMATION_PLAYBOOKS.md). **Count single source:** `fn tool_list()` in `mcp.rs` (counted 2026-07-10: 85 names).

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
session_start → ack_wake_queue → [work: context_for_edit + recall(scope=anchors) + quick_trace + remember] → session_end
```

With `ENGRAM_PROFILE=agent`, wake gate defaults to **hard** — `ack_wake_queue` is required before `context_for_edit` unless the queue was empty (auto-ack at wake).

### Soft tool tier (`ENGRAM_TOOL_TIER`)

| Env | Default under agent profile | Effect |
|-----|----------------------------|--------|
| `ENGRAM_TOOL_TIER=lean` | yes (if unset) | Soft-warn power tools in response meta (`tool_tier_warning`); hard-block `rebuild_bvh` / `force_spatial_ingest` unless deep mode |
| `ENGRAM_TOOL_TIER=power` / `all` | — | No gate |

Does **not** change the 85-tool list; only response discipline. Implementation: `tool_tier.rs` + early gate in `handle_tool_call`.

---

## Tier 2 — Power (use deliberately)

### Harness gates (agent profile)
- `ack_wake_queue` — after executing wake `suggested_actions`; unblocks `context_for_edit` in hard mode
- `ack_edit_arc` — clear edit-arc debt on read-only repeat passes; prefer `update` on `__arc` after edits (`lineage_check?`, `trace_id?`)

### Agent tool fidelity (safe composites — prefer for agents)
- `safe_edit_and_verify` — context_for_edit + quick_trace + optional `__arc` + verify + lineage + `tensor:edit_pattern_*`. Ritual: `processes/ritual/safe-code-edit.toml`
- `update_with_tensor_bond` — recall-first + update + tensor bond + optional scar on mismatch. Ritual: `processes/ritual/verified-memory-update.toml`
- Harness gate: `--suite agent-tool-fidelity` (≥95% correct usage). See [`HARNESS_INJECTION.md`](HARNESS_INJECTION.md).

### Code atlas (situated edit memory)
- `evolution_at_locus` — bounded loci + arc segments + trace chain at a file window (`auto_ingest` default true)
- `ingest_reference_frame` — mint linguistic reference frame + pillar blocks (one-shot, idempotent)

### Memory writes & evolution
- `update` — **preferred** over forget+remember (Lyapunov drift); `ENGRAM_UPDATE_COHERENCE=warn` checks provlog coherence (agent default)
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
- `thought_tile_create` dual-writes `tensor:tile__{stem}` mirror + bonds; `tile_type=propose_improvement` routes verified update on `target_concept`
- `update_with_tensor_bond` on `tile:*` syncs mirror + optional consolidation (`tensor_unification` in create/write responses)
- Rituals: `process:engram.ritual.thought-tile-to-tensor`, `process:engram.ritual.verified-update-with-consolidation`
- `promote_hot`, `promote_hot_batch`

### Lexicon seed (word atoms)
- `lexicon_mint_word` — mint `lexicon:word:*` with definition + etymology ProvLog, VSA OP_BIND of def/etym phases, dynamical CRS ≥ 0.74, relate to genesis pillars + `formal_spec:linguistic_reference_frame_v1`. Ritual: `processes/ritual/lexicon_seed.toml` (`agent:engram.ritual.lexicon-seed`). See [RITUALS.md](RITUALS.md) § Lexicon seed.

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
- `cold_start_fidelity` — score ∈ [0,1] from live continuation + readiness (also emitted on `session_start` / `get_continuation_bundle` as `cold_start_fidelity`). Ritual: `process:engram.ritual.cold-start-fidelity`
- `rebuild_bvh` — on-demand full index (deep mode; RAM/time cost)

### Context variables & corpus
- `var_declare`, `var_query`, `var_project` — session/program trace handles (e.g. `var:ctx_program_traces` at session_end)
- `leg_corpus`, `scrub_export` — LEG export and corpus hygiene

### Other power
- `list_concepts`, `list_namespaces`, `set_namespace`
- `export`, `import`, `scout`, `invoke_protocol`, `track_user`
- `demote_from_context`, `process_metrics`, `turn_record`, `thought_tile_draft_from_chain`

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

| Variable | Default (`ENGRAM_PROFILE=agent`) | Effect |
|----------|----------------------------------|--------|
| `ENGRAM_MEMORY_MODE` | `lean` | Anchor-first recall; no auto-BVH |
| `ENGRAM_WAKE_QUEUE_GATE` | `hard` | Block `context_for_edit` until `ack_wake_queue` |
| `ENGRAM_EDIT_ARC_GATE` | `soft` | Warn on repeat `context_for_edit` without `__arc` update |
| `ENGRAM_UPDATE_COHERENCE` | `warn` | Provlog coherence check on `update` (`off`\|`warn`\|`block`) |
| `ENGRAM_NREM_DISABLE` | `1` | Skip heavy NREM walk on large stores at session_end |
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

**Public docs:** Overview and examples in [`CATEGORICAL_LINGUISTIC_CALCULUS.md`](CATEGORICAL_LINGUISTIC_CALCULUS.md). E2e coverage in `crates/engram-server/src/mcp.rs` tests (CRS ≥ 0.85 roundtrip). Ritual wiring: `processes/ritual/ritual_linguistic_wake.toml`.

---

## Related

- [`AGENT_MEMORY_CONTRACT.md`](AGENT_MEMORY_CONTRACT.md) — **start here**
- [`GROK_BUILD_MEMORY.md`](GROK_BUILD_MEMORY.md) — Grok Build integration pitch
- [`RITUALS.md`](RITUALS.md) — full ritual philosophy
- [`GEOMETRIC_MEMORY.md`](GEOMETRIC_MEMORY.md) — substrate theory
- `crates/engram-server/src/mcp.rs` — tool implementations