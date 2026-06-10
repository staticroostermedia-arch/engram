# Item 1.5 Close-out Draft (Heavy Boot + MCP Instability)

## Observation
During server restart / heavy initialization, the OptiX backend performed a very large pipeline creation (149k+ primitives). Even after "Pipeline ready" and ki_hijacker baking messages, the MCP layer remained unstable from the agent's perspective ("Transport closed" on tool calls).

This occurred while the engram process was still using ~66% CPU and 23%+ RAM many minutes after start.

## Root Cause (from code review)
The MCP mode uses a deliberate "Fast Path":
- Lightweight placeholder returned immediately for protocol handshake.
- Real heavy work (full CudaBackend + OptiX BVH + ki_hijacker + large manifold load) runs in a background thread.
- During this background work, the main MCP event loop can become unresponsive.

This is a known design trade-off for "fast ready" UX that creates exactly the agent-facing reliability problem observed.

## Lesson for Item 1.5
- "Healthy" backend logs (OptiX pipeline ready) do not equal healthy agent experience.
- Heavy initialization phases must be explicitly modeled as periods of reduced MCP reliability.
- Future wake-up / hygiene checks should consider backend load signals (CPU, recent heavy bakes, etc.) when deciding whether to trust the current manifold state.

## Recommended Actions
- Document this behavior in relevant skills and guides.
- Consider future improvements: better readiness signaling, load-aware backpressure on MCP, or deferred heavy features.
- Use the process gap scar mechanism when this class of issue is discovered in operation.

## Related Artifacts
- item1.5_spatial_ingestion_state_engram
- helper:process_gap_scar_template
- trace:force_spatial_ingest_directory_support_added (and related)
- docs/item1.5_reingestion_policy.md
- scripts/ for bootstrap tooling

Status: Captured for geometric recording when MCP is stable or via manual import.
