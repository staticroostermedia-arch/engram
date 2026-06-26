#!/usr/bin/env python3
"""Solid-State Tensor MVP demo — documents the MCP call sequence.

Run after building engram and starting MCP (or use cargo test driver):
  SCRATCH=/tmp/grok-goal-ba89031bf0b1/implementer \\
  cargo test -p engram-server solid_state_tensor -- --nocapture

Live MCP sequence (Grok/Cursor with engram MCP):
  1. mcp_engram_session_start(intent="tensor demo")
  2. mcp_engram_tensor_upsert(concept="solid_state_tensor_entry_v1", text="...", bonds=[...])
  3. mcp_engram_tensor_recall(query="solid-state tensor NVMe")
  4. mcp_engram_verify_manifold_integrity(min_crs=0.74)
  5. mcp_engram_session_end(summary="tensor demo complete", prepare_compression=true)

PRIMARY OBSERVABLE: tensor_recall JSON with entries[].q.q_preview (8 floats),
entries[].q.unit_sphere_ok=true, edges/bonds with merkle_sub_nonzero=true.
"""

print("Solid-State Tensor MVP demo")
print("Use cargo test -p engram-server solid_state_tensor for deterministic proof.")
print("Or invoke mcp_engram_tensor_upsert + mcp_engram_tensor_recall via your MCP client.")