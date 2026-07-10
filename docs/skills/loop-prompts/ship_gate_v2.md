# Ship Gate v2 (glassbox)

**Loop id:** `ship_gate`  
**Parent:** `goal:ship_substrate`  
**Interval (suggested):** operator-defined (often after dual RSI substrate wins)  
**Skill:** [engram-glassbox-rsi.md](../engram-glassbox-rsi.md)  
**Control:** `helper:rsi_dual_loop_state` (`open_pr`, `last_fire_goal`, `last_verify`)

Reschedule after edits: paste the fenced prompt body below into `scheduler_create`.

---

## Paste-ready scheduler prompt

```
SHIP GATE v2 — dirty-tree → local test → commit/PR + fire goal + typed verify

Working dir: Engram repo root. Use Engram MCP (search_tool then use_tool). ENGRAM_PROFILE=agent.

LIFECYCLE (do not skip steps)

1. session_start(intent="ship_gate fire — local verify then PR or ship_skip")
2. ack_wake_queue(executed=true) before any context_for_edit
3. Ensure parent exists:
   - read_concept("goal:ship_substrate")
   - if missing: goal_create(goal_id="ship_substrate",
       statement="Ship substrate code with local verify then PR",
       parent="goal:engram_mvp_v1", priority="high",
       affirm="Green local tests + PR or honest ship_skip",
       deny="claim ship without tests; auto-merge; force-push")
4. read_concept("helper:rsi_dual_loop_state") — note open_pr, last_verify, mcp_restart_required
5. Mint child fire goal:
   goal_create(
     goal_id="fire_ship_gate_<session_key_or_job>_<unix_ts>",
     parent="goal:ship_substrate",
     statement="Ship gate — local tests then PR or skip",
     priority="medium",
     affirm="Ship only with ship_local pass or honest ship_skip",
     deny="force-push; auto-merge; ship claim without green local tests",
     reconcile="Advances ship_substrate under engram_mvp_v1"
   )
   Verify packet pending:
     parent: goal:ship_substrate
     loop: ship_gate
     track: null
     intent: "Ship gate — local tests then PR or skip"
     verify_type: ship_local   # or ship_skip if tree clean
     verify_status: pending
     verify_evidence: ""
     falsify: "tests red; no PR URL on ship claim; dirty uncommitted ship claim"

6. Act (single ship attempt):
   a. git status / branch — prefer current feature branch (e.g. feat/*). Never force-push.
   b. If working tree clean AND nothing meaningful to ship (no unpushed ship-worthy commits
      that lack a PR, and no open ship work):
        → verify_type=ship_skip, verify_status=pass after recording reason
        → complete child as grey skip (not green hero)
        → go to step 8–10
   c. If dirty or commits ready to ship:
        - Run LOCAL tests only (this gate is not remote CI). Prefer targeted crate tests
          relevant to the change; record summary (e.g. cargo test -p engram-server …).
        - On red tests: stop. verify_status=fail. Do not open/claim PR as shipped.
        - On green: commit if needed (conventional message; no secrets). Push with normal
          push only (no --force / --force-with-lease unless human explicitly ordered —
          default HARD: no force-push).
        - Open or update PR via gh if not open; capture PR URL.
        - verify_type=ship_local

7. Typed verify:
   ship_local PASS = local tests green + commit identity + PR URL recorded
   ship_skip PASS  = explicit clean-tree / nothing-to-ship rationale (grey skip)
   FAIL            = tests red, missing PR URL on ship claim, or forced through red tests
   Fill verify_evidence: test summary, commit SHA, PR URL or skip reason.

8. goal_update_status:
   - pass (ship_local or ship_skip) → completed
   - fail → blocked; scar if repeated same failure
   Never claim "shipped" without ship_local pass. Never mark ready-to-merge here (that is pr_watch).

9. update helper:rsi_dual_loop_state:
   - last_fire_goal = goal:fire_ship_gate_...
   - last_verify = { type: ship_local|ship_skip, status: pass|fail, at: ISO-8601 }
   - open_pr = PR URL on ship_local pass (leave prior open_pr if ship_skip and PR still open)
   - if server binary changed this ship: note mcp_restart_required may need stale check next

10. session_end(summary=..., prepare_compression=true)
    Summary MUST include fire goal id, verify_status, verify_type, PR URL or skip reason,
    parent goal:ship_substrate, dual_loop updated yes/no.

HARD (never violate)
- Ship verify = LOCAL tests only (remote CI is pr_watch).
- No force-push.
- No auto-merge.
- No pack dumps in chat.
- No auto MCP kill/restart unless ENGRAM_ALLOW_MCP_RESTART=1.
- Clean tree → ship_skip (complete child; grey skip, not green hero).
- Do not multi-track Dual RSI work inside ship_gate.
```

---

## Verify types

| Situation | `verify_type` | Pass means |
|-----------|---------------|------------|
| Code to ship | `ship_local` | Tests green + commit + PR URL |
| Clean / nothing to ship | `ship_skip` | Explicit skip recorded |

## dual_loop fields this loop owns

`open_pr`, `last_fire_goal`, `last_verify` (type `ship_local` \| `ship_skip`).
