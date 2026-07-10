# Reschedule loops onto Glass-Box RSI v2 prompts

**When:** After editing any `*_v2.md` body, or when rotating scheduler jobs onto fire-goal + typed-verify prompts.  
**Operator skill:** [engram-glassbox-rsi.md](../engram-glassbox-rsi.md)  
**Prompt bodies:** this directory ([README](README.md))

In-repo markdown does **not** update live schedulers. You must **delete the old job** and **create a new one** with the paste-ready fenced prompt from the matching `*_v2.md`.

---

## HARD procedure

1. **Copy** the fenced scheduler prompt from the target `*_v2.md` (entire body inside the fence).
2. **`scheduler_delete(id=…)`** the old job if it still appears in `scheduler_list`.
3. **`scheduler_create(interval=…, prompt=…, recurring=true)`** with the new body.
4. Record the **new job id** returned by create (ids rotate; never treat the table below as permanent).
5. Optional: `session_start` / quick_trace note which loops were rotated.

Do **not** leave both old and new Dual RSI (or Ship) jobs running the same loop — duplicate fires mint duplicate child goals.

**PR watch:** schedule only while `helper:rsi_dual_loop_state.open_pr` is set; delete or pause when PR merges/closes.

---

## Known job IDs (snapshot — must refresh)

> **IDs expire ~7 days** after creation (harness max lifetime). Refresh via `scheduler_list` before delete. This table is a **point-in-time** operator aid from the glassbox cutover conversation; update the table when you reschedule.

| Loop | Interval | Snapshot job id | v2 prompt body | Parent |
|------|----------|-----------------|----------------|--------|
| Dual RSI | 20m | `019f4d8d86ec` | [dual_rsi_v2.md](dual_rsi_v2.md) | `goal:dual_rsi_program` |
| Hermies | 2h | `019f4d8f921b` | *(legacy / non-glassbox body — keep or rewrite separately)* | — |
| Meta | 8h | `019f4d8fa9dd` | *(legacy / non-glassbox body — keep or rewrite separately)* | — |
| Aliveness | 1d | `019f4d8fc387` | [aliveness_bench_v2.md](aliveness_bench_v2.md) | `goal:dual_rsi_program` |
| Research | 3d | `019f4d8fdbc1` | *(legacy / non-glassbox body — keep or rewrite separately)* | — |
| Consciousness | 30m | `019f4daa1d06` | *(legacy / non-glassbox body — keep or rewrite separately)* | — |
| Ship | 1d | `019f4db6bcc9` | [ship_gate_v2.md](ship_gate_v2.md) | `goal:ship_substrate` |
| PR Watch | 2h | `019f4dbb269c` | [pr_watch_v2.md](pr_watch_v2.md) | `goal:ship_substrate` |
| MCP Stale | 1d | `019f4dbb497a` | [mcp_stale_v2.md](mcp_stale_v2.md) | dual_rsi or ship |

**Glassbox v2 cutover priority (prompt bodies in this folder):** Dual RSI, Ship, PR Watch, MCP Stale, Aliveness.  
Hermies / Meta / Research / Consciousness stay on their existing prompts until those loops get glassbox rewrites.

---

## Example: rotate Dual RSI to v2

```
# 1. List — confirm id still live
scheduler_list

# 2. Delete snapshot (or current) Dual RSI job
scheduler_delete(id="019f4d8d86ec")   # only if still listed; else use id from list

# 3. Create with body from dual_rsi_v2.md fenced block
scheduler_create(
  interval="20m",
  recurring=true,
  prompt="<paste entire fenced prompt from dual_rsi_v2.md>"
)

# 4. Note new id from create response; update this table
```

Suggested intervals (operator choice): Dual RSI ~20m; PR watch while `open_pr` set ~15–30m or 2h; MCP stale after rebuilds / 1d; Aliveness ~30–60m or 1d; Ship gate on demand / 1d after substrate wins.

---

## After reschedule

- First fire must: `session_start` → ack → mint `goal:fire_*` → typed verify → update `helper:rsi_dual_loop_state`.
- LEG glassbox (`?view=glassbox`) only reflects state after dual_loop + fire goals exist ([mint parents runbook](../../../scripts/mint_glassbox_parent_goals.md)).
- No auto-merge; no stage flip without `verify_status=pass`.

## Related

- [loop-prompts README](README.md)
- [glassbox design](../../superpowers/specs/2026-07-10-glassbox-rsi-design.md)
- Plan Task 9: [2026-07-10-glassbox-rsi.md](../../superpowers/plans/2026-07-10-glassbox-rsi.md)
