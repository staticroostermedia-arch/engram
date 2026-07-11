# PR Watch v2 (glassbox)

**Loop id:** `pr_watch`  
**Parent:** `goal:ship_substrate`  
**Interval (suggested):** frequent while `open_pr` set (e.g. 15–30m)  
**Skill:** [engram-glassbox-rsi.md](../engram-glassbox-rsi.md)  
**Control:** `helper:rsi_dual_loop_state` (`open_pr`, `last_verify`)

Reschedule after edits: paste the fenced prompt body below into `scheduler_create`.

---

## Paste-ready scheduler prompt

```
PR WATCH v2 — remote CI rollup + fire goal + typed verify (ci_status)

Working dir: Engram repo root. Use Engram MCP (search_tool then use_tool). ENGRAM_PROFILE=agent.
Remote checks via gh (GitHub CLI). This loop does NOT run ship_local tests as the gate of record.

LIFECYCLE (do not skip steps)

1. session_start(intent="pr_watch fire — CI rollup + ready honesty")
2. ack_wake_queue(executed=true)
3. Ensure parent exists:
   - read_concept("goal:ship_substrate")
   - if missing: goal_create(goal_id="ship_substrate",
       statement="Ship substrate code with local verify then PR",
       parent="goal:engram_mvp_v1", priority="high")
4. read_concept("helper:rsi_dual_loop_state") → OPEN_PR = open_pr
   If OPEN_PR null: still mint child; verify may pass with evidence "no open_pr — nothing to watch"
   (not ready-to-merge). Prefer not inventing a PR.
5. Mint child fire goal:
   goal_create(
     goal_id="fire_pr_watch_<session_key_or_job>_<unix_ts>",
     parent="goal:ship_substrate",
     statement="PR watch — CI rollup for open PR",
     priority="medium",
     affirm="Honest ready-to-merge only if all required checks SUCCESS",
     deny="ready on partial/red CI; auto-merge; multi-fix shotgun",
     reconcile="Protects ship_substrate merge honesty under engram_mvp_v1"
   )
   Verify packet pending:
     parent: goal:ship_substrate
     loop: pr_watch
     track: null
     intent: "PR watch — CI rollup"
     verify_type: ci_status
     verify_status: pending
     verify_evidence: ""
     falsify: "ready-to-merge claimed while any required check not SUCCESS"

6. Act (single PR check; optional ONE narrow CI fix):
   a. Resolve PR: dual_loop.open_pr or gh pr view / gh pr list for current branch.
   b. Collect check rollup (gh pr checks / gh pr view --json statusCheckRollup,state,mergeable).
   c. Classify each required check: SUCCESS | FAILURE | PENDING | OTHER.
   d. ready_to_merge = true ONLY if:
        - PR open (or mergeable policy allows)
        - AND every REQUIRED check is SUCCESS
      Partial matrix (one job fail, one pass) ⇒ ready_to_merge=false (yellow honesty).
   e. Fix budget: at most ONE narrow CI fix this fire if clearly local/flake-actionable.
      Second same failure → scar + stop (doom loop). No drive-by refactors.
   f. Never gh pr merge / never enable auto-merge.

7. Typed verify (verify_type=ci_status):
   PASS = check rollup recorded honestly (paths: PR URL, states, ready_to_merge true|false).
         Pass does NOT require ready_to_merge=true — honest "not ready" is a pass.
   FAIL = missing rollup; or claimed ready_to_merge while any required check ≠ SUCCESS;
         or auto-merge attempted.
   verify_evidence must include: PR URL, required check names+states, ready_to_merge boolean.

8. goal_update_status:
   - pass → completed
   - fail → blocked; scar if repeated honesty violation or same CI fail twice with no progress

9. update helper:rsi_dual_loop_state:
   - last_fire_goal = goal:fire_pr_watch_...
   - last_verify = { type: "ci_status", status: pass|fail, at: ISO-8601 }
   - open_pr = current PR URL (or null if closed/merged — do not claim merge without evidence)
   - Do not set any "ready" field that contradicts required-check rollup

10. session_end(summary=..., prepare_compression=true)
    Summary MUST include fire goal id, verify_status, ci_status evidence one-liner,
    ready_to_merge true|false, PR URL, parent goal:ship_substrate, dual_loop updated.

HARD (never violate)
- ready_to_merge only if ALL required checks are SUCCESS.
- No auto-merge. No force-push.
- One narrow CI fix max per fire; second same failure → scar + stop.
- Flaky/partial CI = not ready (yellow), never green ready.
- No pack dumps in chat.
- No auto MCP kill unless ENGRAM_ALLOW_MCP_RESTART=1.
- Ship local tests are ship_gate's job; do not re-label red remote CI as ship_local pass.
```

---

## Ready-to-merge rule

| Required checks | `ready_to_merge` |
|-----------------|------------------|
| All SUCCESS | may be true |
| Any FAILURE / PENDING / missing required | **false** |
| No open PR | false (watch no-op; verify can still pass with evidence) |

## dual_loop fields this loop owns

`open_pr`, `last_fire_goal`, `last_verify` (type `ci_status`).
