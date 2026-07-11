# Loop prompts (Glass-Box RSI v2)

Canonical **scheduler bodies** for Engram Glass-Box RSI. Every fire mints a child `goal:fire_*`, runs a **typed verify**, then updates `helper:rsi_dual_loop_state.last_fire_goal` + `last_verify`.

**Operator skill:** [engram-glassbox-rsi.md](../engram-glassbox-rsi.md)  
**Schemas:** [fire_verify_packet_v1.json](../../schemas/fire_verify_packet_v1.json), [dual_loop_state_v1.json](../../schemas/dual_loop_state_v1.json)  
**Spec / plan:** [glassbox-rsi design](../../superpowers/specs/2026-07-10-glassbox-rsi-design.md), [implementation plan](../../superpowers/plans/2026-07-10-glassbox-rsi.md)

Do **not** leave verify out — LEG glassbox depends on fire goals + `last_verify`.

---

## Prompts

| File | Loop | Parent | `verify_type` |
|------|------|--------|----------------|
| [dual_rsi_v2.md](dual_rsi_v2.md) | `dual_rsi` | `goal:dual_rsi_program` | `substrate_local` \| `gemma_stage` \| `meta_policy` (by S/G/M track) |
| [ship_gate_v2.md](ship_gate_v2.md) | `ship_gate` | `goal:ship_substrate` | `ship_local` or `ship_skip` |
| [pr_watch_v2.md](pr_watch_v2.md) | `pr_watch` | `goal:ship_substrate` | `ci_status` (ready only if **all** required checks SUCCESS) |
| [mcp_stale_v2.md](mcp_stale_v2.md) | `mcp_stale` | dual_rsi or ship | `binary_vs_proc` |
| [aliveness_bench_v2.md](aliveness_bench_v2.md) | `aliveness` | `goal:dual_rsi_program` | `metrics_atom` |

Each file has a **paste-ready** fenced block for the scheduler prompt field.

---

## Reschedule

After editing any prompt body:

1. Copy the fenced prompt from the file.
2. Call **`scheduler_create`** with the new prompt (and desired interval).
3. Cancel or replace the previous scheduled job if your harness keeps old ids.

Do not assume in-repo files auto-update live schedulers — **reschedule with `scheduler_create`**.

Suggested intervals (operator choice): Dual RSI ~20m; PR watch while `open_pr` set ~15–30m; MCP stale after rebuilds; Aliveness ~30–60m; Ship gate on demand / after substrate wins.

---

## Shared HARD rules (all loops)

- `session_start` + `ack_wake_queue` every fire.
- Mint child `goal:fire_*` under durable parent before acting.
- Typed verify **before** `goal_update_status` → completed.
- Update `helper:rsi_dual_loop_state` with `last_fire_goal` + `last_verify`.
- `session_end` summary includes fire goal id + `verify_status`.
- **No** full packs in chat; **no** multi-track Dual RSI; **no** force-push; **no** auto-merge.
- **No** auto MCP kill/restart unless `ENGRAM_ALLOW_MCP_RESTART=1`.
- No stage flip / ship-shipped / ready-to-merge without `verify_status=pass` on the matching gate.
- LEG Browser is **read-only** — it does not run these loops.

---

## Related

- Wake / handoff: [engram-wake-up.md](../engram-wake-up.md), [engram-session-end.md](../engram-session-end.md)
- 8-tool contract: [AGENT_MEMORY_CONTRACT.md](../../AGENT_MEMORY_CONTRACT.md)
- LEG: [LEG_BROWSER.md](../../LEG_BROWSER.md) (`?view=glassbox`)
