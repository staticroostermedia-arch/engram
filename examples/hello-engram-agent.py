# examples/hello-engram-agent.py
# Lean 8-tool contract demo for external agents.
#
# Run (after engram MCP is available):
#   PYTHONPATH=integrations/python python examples/hello-engram-agent.py
#
# Demonstrates the Agent Memory MVP loop:
#   session_start → context_for_edit → recall → quick_trace → remember → session_end
#
# Load docs/AGENT_MEMORY_CONTRACT.md + SKILLS.md into real agent instructions.
# Current build: target/debug/engram (cargo build -p engram-server)

import os


class EngramClient:
    """Shim — replace with integrations/python/engram_client.py for live MCP."""

    def session_start(self, intent, include_spatial=False):
        print(f"[MCP] session_start(intent={intent!r})")
        print("  → continuation_bundle: {primary_goal, last_session_end, active_artifacts}")
        print("  → backend_readiness: {bvh_ready, recall_mode, leg_block_count}")

    def context_for_edit(self, path):
        print(f"[MCP] context_for_edit({path!r})")
        print("  → file-scoped spatial + related traces (no watch_workspace)")

    def recall(self, query, k=5, scope="anchors"):
        print(f"[MCP] recall({query!r}, k={k}, scope={scope!r})")

    def quick_trace(self, decision, why, **kwargs):
        print(f"[MCP] quick_trace(decision={decision!r}, why={why!r})")
        return "trace:hello_demo_fork"

    def remember(self, concept, text):
        print(f"[MCP] remember({concept!r})")

    def get_backend_readiness(self):
        print("[MCP] get_backend_readiness() → lean, sampled_bounded recall")

    def session_end(self, summary, prepare_compression=True):
        print(f"[MCP] session_end → structured handoff packet")
        print(f"  summary: {summary[:80]}...")


def load_contract_snippet():
    path = os.path.join("docs", "AGENT_MEMORY_CONTRACT.md")
    if os.path.exists(path):
        with open(path) as f:
            return f.read()[:600] + "\n... [load full file in agent context]"
    return "(docs/AGENT_MEMORY_CONTRACT.md not found)"


client = EngramClient()

print("\n=== Engram Agent Memory MVP — 8-Tool Lean Loop ===\n")
print("Contract excerpt:\n")
print(load_contract_snippet())

print("\n=== 1. WAKE (one call) ===")
client.session_start(intent="hello-engram-agent.py — lean 8-tool demo for Grok Build")

print("\n=== 2. WORK ===")
client.get_backend_readiness()
client.context_for_edit("/path/to/your/project/README.md")
client.recall("agent memory contract lean", scope="anchors", k=5)
trace = client.quick_trace(
    decision="Document lean contract as default agent path",
    why="Reduces wake from 5+ tools to 1; survives 181k stores without OOM",
)
client.remember("demo:lean_contract_understood", f"Produced {trace} in hello demo.")

print("\n=== 3. HANDOFF ===")
client.session_end(
    summary=f"Lean demo complete. {trace}. Next wake: session_start surfaces handoff inline.",
    prepare_compression=True,
)

print("\n=== Done ===")
print("Real agents should:")
print("  1. MCP config with safe env (integrations/grok-build/mcp.json)")
print("  2. Load docs/AGENT_MEMORY_CONTRACT.md + SKILLS.md at session start")
print("  3. Follow docs/skills/engram-wake-up.md (1-call wake)")
print("  4. Escalate to deep mode only when lean bundle is insufficient")
print("\nSee docs/GROK_BUILD_MEMORY.md for the xAI/Grok Build integration pitch.")