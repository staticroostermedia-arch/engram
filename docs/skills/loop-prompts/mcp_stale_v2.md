# MCP Stale v2 (glassbox)

**Loop id:** `mcp_stale`  
**Parent:** `goal:dual_rsi_program` (preferred) or `goal:ship_substrate` if checking post-ship binary  
**Interval (suggested):** after server rebuilds / ship merges, or periodic (e.g. 30–60m)  
**Skill:** [engram-glassbox-rsi.md](../engram-glassbox-rsi.md)  
**Control:** `helper:rsi_dual_loop_state` (`mcp_restart_required`, `last_verify`)

Reschedule after edits: paste the fenced prompt body below into `scheduler_create`.

---

## Paste-ready scheduler prompt

```
MCP STALE v2 — binary vs process + fire goal + typed verify (binary_vs_proc)

Working dir: Engram repo root. Use Engram MCP (search_tool then use_tool). ENGRAM_PROFILE=agent.
Purpose: detect whether live MCP/engram process is older than target/debug/engram (or installed) binary.

LIFECYCLE (do not skip steps)

1. session_start(intent="mcp_stale fire — binary_vs_proc")
2. ack_wake_queue(executed=true)
3. Ensure parent exists (prefer dual RSI program; ship parent ok if fire is post-ship):
   - read_concept("goal:dual_rsi_program") and/or "goal:ship_substrate"
   - if dual_rsi_program missing: goal_create(goal_id="dual_rsi_program",
       statement="Dual RSI substrate+Gemma stage machine with typed verify",
       parent="goal:engram_mvp_v1", priority="high")
   PARENT = goal:dual_rsi_program  # or goal:ship_substrate when explicitly post-ship
4. read_concept("helper:rsi_dual_loop_state") — prior mcp_restart_required, open_pr
5. Mint child fire goal:
   goal_create(
     goal_id="fire_mcp_stale_<session_key_or_job>_<unix_ts>",
     parent="<PARENT>",
     statement="MCP stale check — binary vs process",
     priority="medium",
     affirm="Honest FRESH|STALE|OFFLINE atom; restart only if allowed",
     deny="auto-kill MCP without ENGRAM_ALLOW_MCP_RESTART=1; silent STALE",
     reconcile="Keeps agent MCP on current binary for dual_rsi/ship honesty"
   )
   Verify packet pending:
     parent: <PARENT>
     loop: mcp_stale
     track: null
     intent: "MCP stale — binary_vs_proc"
     verify_type: binary_vs_proc
     verify_status: pending
     verify_evidence: ""
     falsify: "claimed FRESH while process start < binary mtime; or auto-killed without allow"

6. Act (probe only + optional allowed restart):
   a. Resolve binary path (prefer): /home/a/Documents/Engram/target/debug/engram
      (or `which engram` / build path from env). Record mtime epoch + ISO.
   b. Resolve process: engram mcp / engram-server / host MCP pid (ps, /proc/<pid>, or
      scripts/engram-mcp-health.sh if present). Record start epoch / elapsed.
   c. Classify overall:
        OFFLINE — no process found
        STALE   — process start_epoch < binary mtime epoch (binary newer than process)
        FRESH   — process running and start_epoch >= binary mtime epoch
   d. Restart policy:
        - Default: DO NOT kill or restart MCP.
        - Only if ENGRAM_ALLOW_MCP_RESTART=1 AND operator intent allows: restart once,
          re-probe, record outcome.
        - Never force-kill unrelated processes; never force-push as part of this loop.

7. Typed verify (verify_type=binary_vs_proc):
   PASS = classification atom written with evidence (binary path+mtime, pid+start, overall).
         FRESH, STALE, and OFFLINE can all be verify pass if honestly recorded.
   FAIL = no probe; contradictory claim (e.g. FRESH with process older than binary);
         unauthorized restart/kill attempted.
   verify_evidence: overall=…; binary=… mtime=…; pid=… start=…; allow_restart=0|1

8. goal_update_status:
   - pass → completed
   - fail → blocked; scar if repeated false FRESH or unauthorized restart

9. update helper:rsi_dual_loop_state:
   - last_fire_goal = goal:fire_mcp_stale_...
   - last_verify = { type: "binary_vs_proc", status: pass|fail, at: ISO-8601 }
   - mcp_restart_required = true if STALE or OFFLINE (and still needs restart);
     false if FRESH
   - Optional short note atom: remember metric:mcp_stale_<date> related to parent

10. session_end(summary=..., prepare_compression=true)
    Summary MUST include fire goal id, verify_status, overall FRESH|STALE|OFFLINE,
    mcp_restart_required, parent id, dual_loop updated.

HARD (never violate)
- No auto MCP kill/restart unless ENGRAM_ALLOW_MCP_RESTART=1.
- No force-push. No auto-merge.
- No pack dumps in chat.
- Honest STALE is success of the *check*; do not hide restart debt.
- Do not multi-track Dual RSI or ship code in this fire.
```

---

## Classification

| overall | Meaning | `mcp_restart_required` |
|---------|---------|------------------------|
| FRESH | process ≥ binary mtime | false |
| STALE | binary newer than process | true |
| OFFLINE | no process | true (needs start) |

## dual_loop fields this loop owns

`mcp_restart_required`, `last_fire_goal`, `last_verify` (type `binary_vs_proc`).
