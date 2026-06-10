# examples/spatial_geosphere_demo.py - Spatial AABB + Geosphere demo for Engram (runnable via MCP client or TUI)
# Demonstrates Item 1.5 spatial (force/context/recall_in_file) + geosphere frames + momentum (core of geometric non-flat).
# Run: PYTHONPATH=integrations/python python examples/spatial_geosphere_demo.py
#   (assumes engram MCP server running with current build: target/debug/engram or `cargo run -p engram-server`)
# Per GITHUB_MVP_PREP_PLAN.md Phase 2: 'spatial_geosphere_demo'. Follows full Code Edit Ritual + working-memory.
# Pre: watch + context + trace (this file); post: recontext + delta trace + relate/remember_solution.
#
# PATH NOTE (for GitHub/public clones): All workspace paths are now parameterized. Edit the WORKSPACE var
# in the script (or set via env) to your clone root before running the demo. No repo contains dev-specific /home/... paths.

# Minimal demo client (see mcp_client.py and integrations/python/engram_client.py for production)
class SpatialDemoClient:
    def __init__(self):
        print("SpatialDemoClient (demo shim): use live MCP (search_tool first for schemas, then use_tool).")
    def watch_workspace(self, path):
        print(f"[MCP] watch_workspace {path}  # mandatory; binds daemon AST watcher")
    def force_spatial_ingest(self, paths, recursive=False):
        print(f"[MCP] force_spatial_ingest paths={paths} recursive={recursive}  # 145+ AST items e.g. on rs")
    def context_for_file(self, path):
        print(f"[MCP] context_for_file {path}  # returns AABB AST nodes + CRS (pre-edit recon)")
    def recall_in_file(self, file_stem, start, end):
        print(f"[MCP] recall_in_file {file_stem} {start}-{end}  # pure AABB intersection results")
    def set_geosphere_frame(self, frame):
        print(f"[MCP] set_geosphere_frame {frame}  # symplectic phase context")
    def query_with_momentum(self, query):
        print(f"[MCP] query_with_momentum {query}  # 80% q + 20% p trajectory")
    def spatial_status(self):
        print("[MCP] spatial_status  # item1.5 bootstrap state (bootstrap_in_progress common)")
    def session_start(self, intent):
        print(f"[MCP] session_start intent={intent}")
    def session_end(self, summary, prepare_compression=True):
        print(f"[MCP] session_end ... prepare={prepare_compression}")

client = SpatialDemoClient()

# === Ritual Pre (working-memory + Code Edit Ritual v1) ===
# (In real: call mcp_engram_record_reasoning_trace first with decision, spatial from context, goal_context)
client.session_start(intent="spatial_geosphere_demo - Phase2 prep example for non-flat spatial + geo")
# IMPORTANT for public GitHub: replace "/path/to/your/engram" below with the absolute path to *your* local clone root.
# (e.g. /home/you/projects/engram or /Users/you/engram). This keeps demos portable without leaking original dev paths.
WORKSPACE = "/path/to/your/engram"  # <-- EDIT FOR YOUR CLONE
client.watch_workspace(WORKSPACE)  # always first (mandatory for spatial AABB)
client.spatial_status()
client.force_spatial_ingest([
    f"{WORKSPACE}/crates/engram-server/src/mcp.rs",
    f"{WORKSPACE}/crates/engram-server/src/store.rs",
    f"{WORKSPACE}/README.md",
    f"{WORKSPACE}/docs/GITHUB_MVP_PREP_PLAN.md"
], recursive=False)
client.context_for_file(f"{WORKSPACE}/crates/engram-server/src/mcp.rs")  # pre recon
client.recall_in_file("mcp", 100, 150)  # example AABB range (adjust from actual context)

# === Core Demo ===
client.set_geosphere_frame({"note": "demo frame for github prep spatial", "harmonic": 432})
client.query_with_momentum("github mvp prep spatial geosphere ritual")  # directional p-tensor
client.context_for_file(f"{WORKSPACE}/docs/GITHUB_MVP_PREP_PLAN.md")

# === Ritual Post ===
# (In real: re-call context/recall, record delta trace chained prev, relate to goal, remember_solution, scar friction)
client.spatial_status()
print("Spatial + geosphere demo complete. In live: expect AST nodes, CRS, momentum signals, item1.5 updates.")
print("See docs/GEOMETRIC_MEMORY.md (spatial AABB, force, geosphere), RITUALS.md (Code Edit), GITHUB_MVP_PREP_PLAN.md.")
# session_end in full client usage

# To make more live: after starting server, use the MCP tools directly in your agent env (search first).