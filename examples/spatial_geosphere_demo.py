# examples/spatial_geosphere_demo.py - Spatial AABB + Geosphere demo for Engram (runnable via MCP client or TUI)
# Demonstrates Item 1.5 spatial (force/context/recall_in_file) + geosphere frames + momentum (core of geometric non-flat).
# Run: PYTHONPATH=integrations/python python examples/spatial_geosphere_demo.py
#   (assumes engram MCP server running with current build: target/debug/engram or `cargo run -p engram-server`)
# spatial_geosphere_demo — follows Code Edit Ritual + working-memory (docs/skills/engram-working-memory.md).
# Lean 8-tool pre: session_start + ack_wake_queue + context_for_edit (no watch_workspace at wake).
#
# PATH NOTE (for GitHub/public clones): All workspace paths are now parameterized. Edit the WORKSPACE var
# in the script (or set via env) to your clone root before running the demo. No repo contains dev-specific /home/... paths.

# Minimal demo client (see mcp_client.py and integrations/python/engram_client.py for production)
class SpatialDemoClient:
    def __init__(self):
        print("SpatialDemoClient (demo shim): use live MCP (search_tool first for schemas, then use_tool).")
    def session_start(self, intent):
        print(f"[MCP] session_start intent={intent}")
    def ack_wake_queue(self, executed=True):
        print(f"[MCP] ack_wake_queue executed={executed}  # hard gate before context_for_edit")
    def context_for_edit(self, path):
        print(f"[MCP] context_for_edit {path}  # lean pre-edit: spatial + related traces (replaces watch at wake)")
    def force_spatial_ingest(self, paths, recursive=False):
        print(f"[MCP] force_spatial_ingest paths={paths} recursive={recursive}  # recovery/bootstrap only")
    def context_for_file(self, path):
        print(f"[MCP] context_for_file {path}  # deep mode: file-scoped AABB recon")
    def recall_in_file(self, file_stem, start, end):
        print(f"[MCP] recall_in_file {file_stem} {start}-{end}  # pure AABB intersection results")
    def set_geosphere_frame(self, frame):
        print(f"[MCP] set_geosphere_frame {frame}  # symplectic phase context")
    def query_with_momentum(self, query):
        print(f"[MCP] query_with_momentum {query}  # 80% q + 20% p trajectory (deep mode)")
    def spatial_status(self):
        print("[MCP] spatial_status  # item1.5 bootstrap state (bootstrap_in_progress common)")
    def session_end(self, summary, prepare_compression=True):
        print(f"[MCP] session_end ... prepare={prepare_compression}")

client = SpatialDemoClient()

# === Ritual Pre (lean 8-tool + working-memory) ===
client.session_start(intent="spatial_geosphere_demo - Phase2 prep example for non-flat spatial + geo")
client.ack_wake_queue(executed=True)
# IMPORTANT for public GitHub: replace "/path/to/your/engram" below with the absolute path to *your* local clone root.
WORKSPACE = "/path/to/your/engram"  # <-- EDIT FOR YOUR CLONE
client.spatial_status()
# Optional recovery only (not wake default): force_spatial_ingest when passive daemon ingest is down
# client.force_spatial_ingest([f"{WORKSPACE}/crates/engram-server/src/mcp.rs"], recursive=False)
client.context_for_edit(f"{WORKSPACE}/crates/engram-server/src/mcp.rs")  # lean pre recon
client.recall_in_file("mcp", 100, 150)  # example AABB range (adjust from actual context)

# === Core Demo ===
client.set_geosphere_frame({"note": "demo frame for github prep spatial", "harmonic": 432})
client.query_with_momentum("github mvp prep spatial geosphere ritual")  # directional p-tensor
client.context_for_edit(f"{WORKSPACE}/docs/GEOMETRIC_MEMORY.md")

# === Ritual Post ===
client.spatial_status()
print("Spatial + geosphere demo complete. In live: expect AST nodes, CRS, momentum signals, item1.5 updates.")
print("See docs/GEOMETRIC_MEMORY.md (spatial AABB, force, geosphere), docs/RITUALS.md (Code Edit).")

# To make more live: after starting server, use the MCP tools directly in your agent env (search first).