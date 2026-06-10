# Item 1.5 Close-out Records (Ready for Manifold Push)

## 1. Observation Trace Content

**Suggested concept name:** trace:heavy_boot_mcp_instability_correlation_2026-05-27

Content:

TRACE: Heavy OptiX initialization caused extended MCP transport instability during server restart.

decision_point: Observed repeated "Transport closed" on MCP tools even after multiple "Pipeline ready" messages during a server restart on a large manifold with full OptiX enabled.

justification: The server uses a Fast MCP Path that returns a lightweight placeholder immediately, then kicks off full initialization (CudaBackend + massive OptiX BVH construction for ~150k primitives + ki_hijacker) in a background thread. During this heavy synchronous GPU/CPU work, the main MCP event loop (stdin/stdout JSON-RPC) becomes unresponsive, causing client-side transport closures. Fresh logs showed repeated full OptiX module compilation + GAS builds even after initial "ready" signals, while the process sustained ~65%+ CPU and 23% RAM for many minutes. Some writes succeeded intermittently, confirming partial functionality under load.

cost_of_gap: Agent unable to reliably record reasoning traces, update state blocks, or perform proper session-end during a critical period. Weakens the geometric self-model and Item 1.5 hygiene goals.

improved_process: 
- Treat "backend pipeline ready" as distinct from "full agent-facing readiness".
- Add explicit load / initialization phase awareness in wake-up hygiene checks.
- Consider future improvements: deferred ki_hijacker first bake, readiness signal, or backpressure on MCP during heavy init.
- Document this class of issue in Item 1.5 materials.

linked_to: item1.5_spatial_ingestion_state_engram, goal:item1.5_spatial_discipline_adoption, helper:process_gap_scar_template

---

## 2. State Update Content

**Block:** item1.5_spatial_ingestion_state_engram (append / update)

Add:

heavy_init_mcp_impact_observed: true
date: 2026-05-27
description: During restart, prolonged high CPU/RAM from OptiX BVH construction for 150k primitives caused extended MCP transport failures ("Transport closed"), even after "Pipeline ready" logs. This blocked reliable geometric recording during the session. Some intermittent writes succeeded.
lesson: "Healthy" low-level backend logs do not equal stable agent experience. Heavy initialization phases must be modeled as reduced-reliability periods for MCP tools.
action: Captured in trace:heavy_boot_mcp_instability_correlation_2026-05-27 and close-out records. Added to process gap awareness for future wake-ups.

---

## 3. Session-End Summary Points (for the actual call or local record)

Key elements to include:

- Server was restarted successfully; OptiX RT-Core pipeline eventually initialized.
- However, the heavy background initialization (OptiX for large primitive count + manifold load) caused the MCP layer to be unreliable for an extended period.
- This directly impacted the ability to perform Item 1.5 work geometrically (recording observations, updating state, proper session-end).
- The observation was captured locally in docs/item1.5_closeout_records.md and will be pushed to the manifold when MCP stabilizes.
- This is recorded as a valuable real-world data point for Item 1.5 on agent-facing reliability during backend initialization.
- All other Item 1.5 progress (tool improvements, ritual integrations, scripts, policy docs) was advanced as much as possible locally.

### Ritual Execution Note (2026-05-27)
The /engram-session-end ritual was executed with strong text + local artifact primacy because MCP transport remained closed during the post-restart heavy OptiX window.

The definitive handoff document is now `docs/SESSION_END_2026-05-27.md` (finalized with verbatim user diagnosis, the listening scar, the exact safe start-of-day `ENGRAM_OPTIX_ENABLED=0` launch command, and explicit `agent_instance_terminal` marker).

The three high-value blocks staged here (observation trace, state update, summary points) remain the payload to push via MCP on the first stable session after the lazy OptiX binary is deployed and the server is responsive.

This closeout record itself is now referenced from the terminal state document.

