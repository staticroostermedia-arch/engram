# Dual RSI v2 (glassbox)

**Loop id:** `dual_rsi`  
**Parent:** `goal:dual_rsi_program`  
**Interval (suggested):** 20m (operator schedules via `scheduler_create`)  
**Skill:** [engram-glassbox-rsi.md](../engram-glassbox-rsi.md)  
**Control:** `helper:rsi_dual_loop_state`

Reschedule after edits: paste the fenced prompt body below into `scheduler_create`.

---

## Paste-ready scheduler prompt

```
DUAL RSI v2 — ONE track + fire goal + typed verify

Working dir: Engram repo root. Use Engram MCP (search_tool then use_tool). ENGRAM_PROFILE=agent.

LIFECYCLE (do not skip steps)

1. session_start(intent="dual_rsi fire — one track + typed verify")
2. ack_wake_queue(executed=true) before any context_for_edit
3. Ensure parent exists:
   - read_concept("goal:dual_rsi_program")
   - if missing: goal_create(goal_id="dual_rsi_program",
       statement="Dual RSI substrate+Gemma stage machine with typed verify",
       parent="goal:engram_mvp_v1", priority="high",
       affirm="One S/G/M win per fire with verify", deny="multi-track; pack dumps; stage flip without pass")
4. read_concept("helper:rsi_dual_loop_state") → TRACK = track_next (must be S|G|M)
   If dual_loop missing: scar thin handoff; still mint child with TRACK=S default only if no better signal.
5. Mint child fire goal:
   goal_create(
     goal_id="fire_dual_rsi_<session_key_or_job>_<unix_ts>",
     parent="goal:dual_rsi_program",
     statement="Dual RSI track <TRACK> — one win",
     priority="medium",
     affirm="Advance dual_rsi_program on pass",
     deny="multi-track; packs in chat; stage flip without verify pass",
     reconcile="Compounds engram_mvp_v1 continuity + PEFT path"
   )
   Embed verify packet (status=pending) in goal note / remember metric:verify_<fire_id>:
     parent: goal:dual_rsi_program
     loop: dual_rsi
     track: <TRACK>
     intent: "Dual RSI track <TRACK> — one win"
     verify_type: <see TRACK map>
     verify_status: pending
     verify_evidence: ""
     falsify: <see TRACK map>
   Optional: goal_set_primary to the child for this fire.

6. Execute ONE track only (TRACK from dual_loop). Never S+G or G+M in one fire.

   === TRACK S (substrate) — verify_type=substrate_local ===
   Win = one substrate/continuity artifact without chat pack dumps.
   Preferred path:
   - Prefer mcp_engram_leg_corpus (or equivalent) build that writes disk_export_path
     under data/lora-export/ (or ENGRAM_LORA_EXPORT_DIR). Chat must show packs=[] unless
     ENGRAM_LORA_EXPORT_INLINE=1.
   - Or targeted cargo/test + integrity sample for a small substrate fix.
   - Optional grow: scripts/grow_leg_sft.sh after disk pack batch exists.
   Falsify: disk path missing; packs dumped in chat; integrity sample fails.

   === TRACK G (Gemma stage) — verify_type=gemma_stage ===
   Advance ONE stage on the stage machine when possible:
     offline → hermies_up → packs → jsonl → peft_metrics → adapter_live → eval_gate
     (post-eval optional: gguf_lora — blocked until llama.cpp maps Gemma4 LoRA tensors)
   Typical tools/scripts (pick next unfinished stage only):
   - hermies_up: endpoint http://127.0.0.1:11435 healthy; record cos/dim
   - packs/jsonl: disk batch → scripts/export_leg_corpus_jsonl.py → leg_geometry_sft.jsonl
   - peft_metrics / adapter_live: scripts/peft_leg_geometry_train.py → peft_metrics.json + adapter_path
   - eval_gate: scripts/eval_leg_geometry_gate.py → eval_gate_metrics.json
   Do NOT flip gemma.stage in dual_loop until verify pass.
   Falsify: stage metric file missing/status not ok; claimed stage without artifact path.

   === TRACK M (meta policy) — verify_type=meta_policy ===
   Policy-only fire: read dual_loop + last verifies + open_pr/blockers; write rationale;
   set track_next to S|G|M with justification; optional scar on friction/doom loop.
   No large code ship; no PEFT train; no multi-track work disguised as meta.
   Falsify: dual_loop not updated with rationale; silent track flip without note.

7. Typed verify (required before complete):
   S: disk path exists OR cargo/test summary + integrity sample; packs not in chat
   G: stage metric file/status ok (peft_metrics / eval_gate / adapter path as claimed)
   M: dual_loop rationale written (and track_next set intentionally)
   Fill verify_status=pass|fail + verify_evidence (paths, row counts, metrics concept).
   Optional: remember metric:verify_<fire_id> related to child goal.

8. goal_update_status on fire child:
   - pass → completed
   - fail → blocked (or abandoned); scar if same fail twice (doom loop → stop)
   Never claim stage advanced / substrate shipped without pass.

9. update helper:rsi_dual_loop_state via mcp_engram_update (always, pass or fail):
   - track_last = TRACK (if work ran)
   - track_next = next track (only advance stage fields / optimistic next on pass)
   - last_fire_goal = goal:fire_dual_rsi_...
   - last_verify = { type: <verify_type>, status: pass|fail, at: ISO-8601 }
   - gemma.* only advanced on G verify pass
   - parents includes goal:dual_rsi_program
   On fail: do not flip gemma.stage; set blockers / scar if repeated.

10. session_end(summary=..., prepare_compression=true)
    Summary MUST include:
    - fire goal id (goal:fire_dual_rsi_...)
    - verify_status (pass|fail)
    - verify_type + one-line evidence
    - TRACK + parent goal:dual_rsi_program
    - whether dual_loop was updated

HARD (never violate)
- ONE track per fire (no multi-track).
- No full packs / pack dumps in chat — paths + short summaries only.
- No stage flip in dual_loop or Gemma metrics without verify_status=pass.
- No force-push. No auto-merge. No auto MCP kill/restart unless ENGRAM_ALLOW_MCP_RESTART=1.
- LEG is read-only; does not run this loop.
- Token economy: state atom + pointers; avoid re-mint core lexicon / manifesto re-read.
```

---

## Track → verify_type map

| TRACK | `verify_type` | Pass means |
|-------|---------------|------------|
| S | `substrate_local` | Disk artifact and/or targeted test + integrity; no pack dump |
| G | `gemma_stage` | Stage advanced + metric atom / path ok |
| M | `meta_policy` | dual_loop updated with rationale; optional scar |

## dual_loop fields this loop owns

`track_last`, `track_next`, `last_fire_goal`, `last_verify`, `gemma.*` (on G pass), `rationale`, blockers.
