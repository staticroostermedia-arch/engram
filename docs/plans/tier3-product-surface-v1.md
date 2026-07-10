# Tier 3 Plan — Product Surface for Strangers v1

**Status:** Shipped (2026-07-08)  
**Parent:** Tier 1 continuity · Tier 2 trust/hardware · **Tier 3 = stranger can onboard on the 8-tool + composites highway**  
**Audience:** Builder + implementing agents  

---

## Outcomes (acceptance)

1. **Two-doc default load:** After install (`FIRST_RUN` §1–2), agents load only  
   `docs/AGENT_MEMORY_CONTRACT.md` + `docs/skills/engram-wake-up.md`.  
   README / FIRST_RUN state this explicitly; no competing “must read 5+ docs first.”

2. **Composites-as-default:** `mcp_engram_safe_edit_and_verify` and  
   `mcp_engram_update_with_tensor_bond` are preferred for edit/update on the  
   highway (contract + FIRST_RUN paste + TOOL_DECISION_MAP / MCP_TOOLS_REFERENCE).

3. **Lean-safe wake queue:** Agent/lean wake top `suggested_actions` never include  
   lean-avoid tools (`watch_workspace`, `rebuild_bvh`, broad wake `summarize`, …).  
   Locked by `finalize_wake_strips_all_lean_avoid_tools` +  
   `agent_wake_suggested_actions_never_include_lean_avoid`.

4. **Honest tool counts:** Published totals match `tool_list()` length (currently **84**);  
   `tool_list_count_matches_docs_contract_numbers` fails on drift (live list + doc parse).

---

## Explicit non-goals

- Full CRS literal migration beyond Tier 2 trust band  
- cuFile DMA engineering / dual-GPU scheduler redesign  
- Hard-block all power tools; marketplace; Metal/ROCm; multi-agent CRDT  
- Monolith split of `mcp.rs` / `store.rs` (Tier 4+)  
- LEG Browser redesign or new visual product UI  
- Rewriting all skill files / 84 tool descriptions  
- Changing `tool_list` count by deleting tools  

---

## Files

| Area | Paths |
|------|--------|
| Entry | `README.md`, `FIRST_RUN.md` |
| Highway | `docs/AGENT_MEMORY_CONTRACT.md`, `docs/skills/engram-wake-up.md` |
| Align | `docs/MCP_TOOLS_REFERENCE.md`, `docs/TOOL_DECISION_MAP.md` |
| Tests | `mcp.rs` (tool count, stranger docs, lean wake), `cold_start_fidelity.rs` |
| Plan | this file |

---

## Relation to ladder

| Tier | Focus | Status |
|------|--------|--------|
| 1 | Continuity dogfood | Proven |
| 2 | CRS + hardware honesty + soft tool tier | Shipped |
| **3** | **Product surface for strangers** | **This plan — shipped** |
| 4+ | Monolith split, multi-agent, LEG panels | Later |

---

*Plan version: tier3-product-surface-v1 · shipped 2026-07-08*
