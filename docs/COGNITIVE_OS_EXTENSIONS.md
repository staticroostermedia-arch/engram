# Cognitive OS Extensions (E1–E9)

**Goal:** `goal:engram_cognitive_os_extensions_v1`  
**Baseline:** Engram ≥ 0.7.0-beta.13  

Nine net-new surfaces that turn the geometric memory substrate into a **cognitive OS layer**. All tools are **additive** — the lean 8-tool contract is unchanged.

| ID | Extension | MCP / API | Host scale |
|----|-----------|-----------|------------|
| **E1** | Progressive / budgeted wake | `session_start(max_tokens?, max_bytes?, wake_priority?)` + `wake_budget` meta; `mcp_engram_expand_wake` | minimal default tokens ≪ cuda_dual (`wake_budget::default_max_tokens_for_profile`) |
| **E2** | Skill auto-distillation | `mcp_engram_distill_skills`, `mcp_engram_promote_skill_draft` | capped window; auto-pin only if `ENGRAM_DISTILL_AUTO_PIN=1` |
| **E3** | Counterfactual branches | `mcp_engram_branch_{create,checkout,merge,abandon}`; wake `active_branch` | process-local registry + concept tags |
| **E4** | Dream curriculum | `mcp_engram_dream_run` → `metric:dream_*` | **auto-schedule off by default**; never on `minimal` |
| **E5** | Multi-agent leases | `mcp_engram_lease_{acquire,release,break}`; conflict mint | single-key TTL only |
| **E6** | Selective sync packs | `mcp_engram_sync_{export,import}` (`engram_sync_pack_v1`) | quarantine CRS ≤ 0.6 on import |
| **E7** | Topology health | `mcp_engram_topology_health(sample_limit?)` | sample-capped; smaller on minimal |
| **E8** | Structured query | `mcp_engram_query_structured` | limit max 200; scan cap 5k |
| **E9** | Foreign external knowledge | `mcp_engram_ingest_external`, `mcp_engram_accept_external` | anchors omit foreign; URL fetch off |

## Non-goals (this goal)

- Time-aware decay / rehearsal  
- Full MCP tool-graph protocol  
- Multi-lock CRDT / multi-concept leases  
- Auto-pin distilled skills without promote  
- High-CRS sync import without quarantine  

## Code map

| Module | Path |
|--------|------|
| Budgeted wake | `crates/engram-server/src/wake_budget.rs` |
| Structured query | `structured_query.rs` |
| Topology | `topology_health.rs` |
| Branches | `branch_memory.rs` |
| Leases | `lease_conflict.rs` |
| Foreign | `foreign_stratum.rs` |
| Sync packs | `sync_pack.rs` |
| Distill | `skill_distill.rs` |
| Dream | `dream_curriculum.rs` |
| MCP dispatch | `cognitive_os_dispatch.rs` |

## Env

| Env | Effect |
|-----|--------|
| `ENGRAM_WAKE_MAX_TOKENS` | Optional hard budget for session_start continuation |
| `ENGRAM_WAKE_BUDGET_DEFAULT=1` | Use host-profile default token budget even without max_tokens arg |
| `ENGRAM_DISTILL_AUTO_PIN=1` | Promote path pins drafts (default off) |
| `ENGRAM_DREAM_AUTO=0/1` | Auto-schedule dream (still forced off on minimal) |
| `ENGRAM_CONFLICT` | `refuse` \| `mint_and_refuse` (default) |
| `ENGRAM_EXTERNAL_URL_FETCH=1` | Allow https ingest (default local paths only) |

See also: [AGENT_MEMORY_CONTRACT.md](AGENT_MEMORY_CONTRACT.md), [HARDWARE_FIT.md](HARDWARE_FIT.md), [CLAIMS_LEDGER.md](../CLAIMS_LEDGER.md).
