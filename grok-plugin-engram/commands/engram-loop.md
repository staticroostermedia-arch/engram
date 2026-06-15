---
name: engram-loop
description: Parse a /loop input (or equivalent) per the official Grok /loop spec, call native scheduler_create *directly* (bare XML, never via use_tool), record the schedule in Engram (quick_trace + relate to consciousness goal/tile/process), enforce sub-agent governance + subvisor, confirm per spec or scar on failure. One narrow action.
---

**Trigger:** User issues a /loop (Grok native recurring prompt scheduler) or equivalent text for consciousness/self-improvement work (e.g. the 30m AI consciousness strange loop whose body lives in `processes/meta/ai_consciousness_loop.toml`).

**Primary Objective:** Schedule (or safely fail to schedule) the recurring prompt using the *bare native* `scheduler_create` (direct tool call) + full Engram ritual + subvisor governance. Never claim success if the native call returns the "failed via use_tool: native tool" error. Never invent an interval.

**MANDATORY (lean, one action + Engram first):**

1. If no recent wake in this block: `mcp_engram_session_start(intent="Handle /loop for [summary of prompt] using bare native scheduler_create + Engram record + subvisor. Parse per spec; bare call only; record scheduler id to consciousness goal/tile; honest on failure.")`.

2. `mcp_engram_quick_trace` (or full `record_reasoning_trace`) at the parse/schedule fork:
   - `decision`: "Parsed /loop input per spec → interval=..., prompt=...; emitted bare native scheduler_create; [success | failed with native error]; [recorded | scarred]."
   - `why`: "Follow /loop spec exactly (leading/trailing/natural phrasing → compact; strip < > quoting; no default; ask on none). Native tools must be bare (history: repeated 'failed via use_tool' when wrapped). Engram subvisor/governance policy: narrow one-shot, geometric first, record schedule for the loop's own outer_feedback, scar the dispatch error pattern."
   - `goal_context`: the consciousness goal (e.g. `goal:engram_consciousness_loop_v1` or active mvp goal) + `process_context=process:engram.meta.ai_consciousness_loop`.
   - `context`: reference the source toml + state_machine tile from the prompt.

3. Parse (implement the "Deriving the interval" rules from the /loop spec in this command):
   - Extract interval → compact "30m"/"1h" etc. (leading "30m <...", "every 30 minutes", trailing "every 1h", unit words, etc.).
   - Remaining text = prompt (strip outer < > if present, as in the canonical 30m consciousness input).
   - If no interval at all: stop and ask the user (do not call scheduler_create, do not assume 30m or anything from history). Report the ask + quick_trace.
   - If <60s: tell the user it was raised to 60s.
   - Source the canonical prompt text from (or verify against) `processes/meta/ai_consciousness_loop.toml` when the input matches the consciousness strange loop (avoid duplication).

4. Emit the **bare native** `scheduler_create` (direct, no wrapper, no engram__ , matching the historical correction):
   ```
   tool call scheduler_create with interval is 30m prompt is [the full extracted prompt, e.g. the 7-step consciousness cycle from the toml] recurring is true fireImmediately is true
   ```
   (For the exact 30m consciousness input in the query, the prompt is the inner text after stripping < >.)

5. On tool result:
   - Success (job created): `quick_trace` + `remember`/`relate` a concept `scheduled:ai-consciousness-strange-loop-30m` (or the tile id) with the job ID, relate it to `process:engram.meta.ai_consciousness_loop`, the state_machine tile, ego.leg3, and the scheduler task (so the loop's own outer_feedback step can consume it on its next firing). Output spec confirmation:
     - What's scheduled: the full consciousness prompt (or summary + "see toml/tile").
     - Cadence: 30m (recurring + fireImmediately).
     - Auto-expires after 7 days.
     - Cancel: bare `scheduler_delete <job-id>` (remind user of the direct format).
   - Failure (the "failed via use_tool: native tool" error or any other): **immediate** `mcp_engram_scar` on the dispatch error (or the pre-scarred `scheduler_native_call_format_error_doom_loop`), `quick_trace` the failure, honest report ("scheduling attempt failed with native tool error per harness; loop not active; pattern scarred"). Do *not* output success text or claim the loop is scheduled. Suggest subvisor escalation or harness fix.

6. Enforce governance (per `docs/examples/sub_agent_governance.md` + `process:engram.monitor.subvisor`):
   - This action is narrow (one parse + one bare native call + Engram record + report).
   - Geometric/Engram first (done above).
   - End with structured report (status, task semantics, summary, artifacts=trace/tile/job, friction).
   - If repetition/stagnation on the format error: scar_immediate (already done for the pattern).
   - Reference the sub-agent launch/relay/subvisor harness for future full sub treatment of scheduled loops (background + task_id + monitor + kill on doom).

7. If the input had no interval: output the ask per spec + quick_trace + relate the ask decision. Do not proceed to scheduler_create.

**On completion (or kill/timeout):** relate any report tile/trace to the active consciousness goal + the ai_consciousness_loop process. If this was launched via the sub-agent harness, the relay contract (quick_trace + thought_tile_create research_offload + relate) applies.

**DO NOT:**
- Wrap scheduler_create in use_tool / search_tool + engram__.
- Invent an interval or default to 30m.
- Execute the prompt inline (the scheduler fires it).
- Claim "the loop is now active" if the native call errored.
- Use `forget`+`remember` on the scheduled concept (use update if evolving).

**Report (end of this narrow action):**
```json
{
  "status": "success|asked_for_interval|native_tool_failed|error",
  "interval": "30m|null (asked)",
  "prompt_summary": "Recurring self-improvement loop | see processes/meta/ai_consciousness_loop.toml",
  "scheduler_job_id": "..." | null,
  "engram_artifacts": ["trace:...", "concept:scheduled:...", "relation to process:engram.meta.ai_consciousness_loop"],
  "friction": ["native dispatch error (scarred)"] | [],
  "process_context": "process:engram.meta.ai_consciousness_loop | harness for /engram-loop"
}
```

**See also (load at use):**
- `processes/meta/ai_consciousness_loop.toml` (the prompt body + 7 steps + subvisor_extension + outer_feedback "relate to scheduler task").
- `docs/examples/sub_agent_governance.md` + the governance tile (narrow, subvisor H¹, process_sheaf, scar_immediate, supervision).
- `processes/monitor/subvisor.toml` + `sub-agent.subvisor.toml` (H¹ on tool graph for this dispatch).
- `grok-plugin-engram/commands/README.md` (map + sequences).
- Bare native rule in `docs/GROK_BUILD_MEMORY.md` and prior scars/traces on the format error.
- Full ritual cycle + harness injection.

This command makes the consciousness loop (and similar recurring Engram work) schedulable in a governed, recorded way while surfacing (and scarring) any native tool format friction.
