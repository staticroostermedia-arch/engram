# examples/hello-engram-agent.py
#
# START HERE if you opened examples/ first:
#   1. Human: FIRST_RUN.md §1–2 (build + MCP config)
#   2. Agent: docs/AGENT_MEMORY_CONTRACT.md (8-tool loop)
#   3. Then run this demo (shim — wire integrations/python for live MCP)
#
# Run (after engram MCP is available):
#   cargo build -p engram-server
#   PYTHONPATH=integrations/python python examples/hello-engram-agent.py
#
# Continuity path (Tier-4c): wake → remember → session_end → wake2 sees handoff.
# Shipped proof: cargo test continuity_wake_remember_end_wake2_handoff
#   or: scripts/continuity-demo.sh
#
# Loop: session_start → (optional) context_for_edit → recall → quick_trace → remember → session_end
# Prefer composites for real code: safe_edit_and_verify / update_with_tensor_bond
# Build: target/debug/engram --version

import os
import sys


class EngramClient:
    """Shim — replace with integrations/python/engram_client.py for live MCP."""

    def session_start(self, intent, include_spatial=False):
        print(f"[MCP] session_start(intent={intent!r})")
        print("  → continuation: primary_goal, suggested_actions, cold_start_fidelity, handoff preview")
        print("  → readiness / mcp_health (bvh, recall_mode, cufile_transfer_path honesty)")

    def ack_wake_queue(self, executed=True):
        print(f"[MCP] ack_wake_queue(executed={executed}) — required before context_for_edit on agent profile")

    def context_for_edit(self, path):
        print(f"[MCP] context_for_edit({path!r}) — or prefer safe_edit_and_verify for code edits")

    def recall(self, query, k=5, scope="anchors"):
        print(f"[MCP] recall({query!r}, k={k}, scope={scope!r})")

    def quick_trace(self, decision, why, **kwargs):
        print(f"[MCP] quick_trace(decision={decision!r}, why={why!r})")
        return "trace:hello_demo_fork"

    def remember(self, concept, text):
        print(f"[MCP] remember({concept!r})")

    def get_backend_readiness(self):
        print("[MCP] get_backend_readiness()")

    def session_end(self, summary, prepare_compression=True):
        print("[MCP] session_end → helper:session_handoff_latest (single latest-wins packet)")
        print(f"  summary: {summary[:80]}...")

    def read_concept(self, concept):
        print(f"[MCP] read_concept({concept!r})")


def load_contract_snippet():
    path = os.path.join("docs", "AGENT_MEMORY_CONTRACT.md")
    if os.path.exists(path):
        with open(path) as f:
            return f.read()[:600] + "\n... [load full file in agent context]"
    return "(docs/AGENT_MEMORY_CONTRACT.md not found)"


def main():
    client = EngramClient()

    print("\n=== Engram continuity demo — two-doc + 8-tool highway ===\n")
    print("Default load: docs/AGENT_MEMORY_CONTRACT.md + docs/skills/engram-wake-up.md\n")
    print("Contract excerpt:\n")
    print(load_contract_snippet())

    print("\n=== 1. WAKE (one call) ===")
    client.session_start(intent="hello-engram-agent.py — continuity loop demo")
    client.ack_wake_queue(executed=True)

    print("\n=== 2. WORK (lean; composites preferred for real edits) ===")
    client.get_backend_readiness()
    client.recall("agent memory contract lean", scope="anchors", k=5)
    trace = client.quick_trace(
        decision="Document lean continuity loop as default stranger path",
        why="Wake→work→handoff→wake2 must surface handoff without power-tool floods",
    )
    client.remember("demo:lean_contract_understood", f"Produced {trace} in hello demo.")

    print("\n=== 3. HANDOFF ===")
    client.session_end(
        summary=f"Lean continuity demo. {trace}. Next wake must surface handoff.",
        prepare_compression=True,
    )

    print("\n=== 4. WAKE2 (continuity signal) ===")
    client.session_start(intent="hello-engram-agent.py — second wake after handoff")
    client.read_concept("helper:session_handoff_latest")
    print("  → expect single SESSION HANDOFF PACKET v1 + primary_goal from prior end")

    print("\n=== Done ===")
    print("Real agents:")
    print("  1. FIRST_RUN §1–2 then only AGENT_MEMORY_CONTRACT + engram-wake-up")
    print("  2. Prefer mcp_engram_safe_edit_and_verify / update_with_tensor_bond")
    print("  3. Shipped proof: scripts/continuity-demo.sh or cargo test continuity_wake_remember")
    print("  4. Align TUI /goal with mcp_engram_goal_set_primary for the work block")
    return 0


if __name__ == "__main__":
    sys.exit(main())