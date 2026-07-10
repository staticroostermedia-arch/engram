# Aliveness Bench v2 (glassbox)

**Loop id:** `aliveness`  
**Parent:** `goal:dual_rsi_program`  
**Interval (suggested):** periodic health (e.g. 30–60m)  
**Skill:** [engram-glassbox-rsi.md](../engram-glassbox-rsi.md)  
**Control:** `helper:rsi_dual_loop_state` + `metric:dual_rsi_aliveness_*`

Reschedule after edits: paste the fenced prompt body below into `scheduler_create`.

---

## Paste-ready scheduler prompt

```
ALIVENESS BENCH v2 — metrics_atom + fire goal + typed verify

Working dir: Engram repo root. Use Engram MCP (search_tool then use_tool). ENGRAM_PROFILE=agent.
Purpose: write a single aliveness metrics atom for LEG/dual_loop health strip — not a multi-track RSI fire.

LIFECYCLE (do not skip steps)

1. session_start(intent="aliveness fire — metrics_atom")
2. ack_wake_queue(executed=true)
3. Ensure parent exists:
   - read_concept("goal:dual_rsi_program")
   - if missing: goal_create(goal_id="dual_rsi_program",
       statement="Dual RSI substrate+Gemma stage machine with typed verify",
       parent="goal:engram_mvp_v1", priority="high")
4. read_concept("helper:rsi_dual_loop_state") — track_next, gemma.stage, open_pr, mcp_restart_required
5. Mint child fire goal:
   goal_create(
     goal_id="fire_aliveness_<session_key_or_job>_<unix_ts>",
     parent="goal:dual_rsi_program",
     statement="Aliveness bench — write metrics atom",
     priority="medium",
     affirm="metric:dual_rsi_aliveness_* written + related",
     deny="pack dumps; multi-track S/G/M work; fake high fidelity",
     reconcile="Feeds LEG glassbox health strip under dual_rsi_program"
   )
   Verify packet pending:
     parent: goal:dual_rsi_program
     loop: aliveness
     track: null
     intent: "Aliveness bench — metrics atom"
     verify_type: metrics_atom
     verify_status: pending
     verify_evidence: ""
     falsify: "atom not written or not related; invented fidelity without tool evidence"

6. Act (probe + write — no dual RSI track execution):
   Collect what is cheaply available (use tools; do not invent):
   - cold_start_fidelity / session readiness if available (session_start already may include)
   - mean hub CRS / verify_manifold_integrity sample (optional, sample only)
   - hermies endpoint health if dual_loop lists endpoint (e.g. :11435) — cos/dim if known
   - gemma.stage, track_next, peft/adapter pointer from dual_loop
   - leg_block_count / open_scars_count if readiness or summarize exposes them
   - open_pr, mcp_restart_required from dual_loop
   Write atom via remember (concept name pattern):
     metric:dual_rsi_aliveness_<YYYY-MM-DD>
     or metric:dual_rsi_aliveness_<YYYY-MM-DD>_<HHMM> if multiple same day
   Content: compact one-block summary (fidelity, mean_hub_crs, hermies, stage, track_next,
   leg_block_count, open_scars, mcp_restart_required). Paths/pointers only — no packs.
   relate atom → goal:dual_rsi_program and helper:rsi_dual_loop_state (and parent fire goal).

7. Typed verify (verify_type=metrics_atom):
   PASS = metric:dual_rsi_aliveness_* exists with non-empty body AND related to parent
         (and preferably dual_loop helper).
   FAIL = no write; empty atom; no relation; fidelity claimed without source.
   verify_evidence: concept id + key fields one-liner (e.g. fidelity=0.93 stage=eval_gate).

8. goal_update_status:
   - pass → completed
   - fail → blocked; scar if repeated empty aliveness

9. update helper:rsi_dual_loop_state:
   - last_fire_goal = goal:fire_aliveness_...
   - last_verify = { type: "metrics_atom", status: pass|fail, at: ISO-8601 }
   - Do NOT flip track_next or gemma.stage from aliveness alone
   - Optionally pointer field / note to latest aliveness concept if dual_loop allows free keys

10. session_end(summary=..., prepare_compression=true)
    Summary MUST include fire goal id, verify_status, metrics atom concept id,
    parent goal:dual_rsi_program, dual_loop updated yes/no.

HARD (never violate)
- This fire writes metrics_atom only — no multi-track Dual RSI, no ship/PR merge.
- No pack dumps in chat.
- No force-push. No auto-merge.
- No auto MCP kill unless ENGRAM_ALLOW_MCP_RESTART=1.
- Do not flip gemma stage or track_next from aliveness without a Dual RSI verify pass.
- Honest low fidelity / needs_review is allowed; do not polish numbers.
```

---

## Atom naming

| Pattern | Use |
|---------|-----|
| `metric:dual_rsi_aliveness_YYYY-MM-DD` | Daily rollup |
| `metric:dual_rsi_aliveness_YYYY-MM-DD_HHMM` | Multiple fires same day |

## dual_loop fields this loop owns

`last_fire_goal`, `last_verify` (type `metrics_atom`). Does **not** own `track_next` / `gemma.stage`.
