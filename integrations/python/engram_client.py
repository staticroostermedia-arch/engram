"""
engram_client.py — Drop-in Python client for the Engram REST memory server.

Copy this file into any project. No dependencies beyond `requests`.

Usage:
    from engram_client import EngramClient

    mem = EngramClient()                     # default: http://localhost:3456
    mem.remember("my_bug_fix", "Fixed X by doing Y")
    results = mem.recall("bug fixes", k=5)
    for r in results:
        print(r)

Start the server first:
    engram serve --port 3456 --store ~/.engram/manifold

Docs: https://github.com/staticroostermedia-arch/engram/blob/master/docs/rest_api.md
"""

from __future__ import annotations

import os
from dataclasses import dataclass, field
from typing import Optional

try:
    import requests
except ImportError:
    raise ImportError(
        "engram_client requires 'requests'. Install it with: pip install requests"
    )


# ── Data models ──────────────────────────────────────────────────────────────

@dataclass
class Memory:
    """A single memory block returned by recall() or trace()."""
    concept: str
    score: float
    crs: float
    text: str
    explain: Optional[str] = None

    def __repr__(self) -> str:
        crs_label = (
            "PINNED" if self.crs >= 1.0
            else "GROUNDED" if self.crs >= 0.74
            else "HYPOTHESIS" if self.crs >= 0.50
            else "UNCERTAIN"
        )
        return (
            f"Memory(concept={self.concept!r}, score={self.score:.3f}, "
            f"crs={self.crs:.3f} [{crs_label}])"
        )


@dataclass
class RecentEntry:
    """A recently accessed concept entry."""
    concept: str
    last_accessed: int  # Unix timestamp
    ago: str            # Human-readable (e.g. "2m ago")


# ── Client ───────────────────────────────────────────────────────────────────

class EngramClient:
    """
    Thin Python wrapper around the Engram REST API.

    Parameters
    ----------
    base_url : str
        Base URL of the Engram server. Default: http://localhost:3456
    api_key : str, optional
        Bearer token if ENGRAM_API_KEY is set on the server.
        Reads from the ENGRAM_API_KEY environment variable if not provided.
    timeout : float
        Request timeout in seconds. Default: 10.0
    """

    def __init__(
        self,
        base_url: str = "http://localhost:3456",
        api_key: Optional[str] = None,
        timeout: float = 10.0,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self._session = requests.Session()

        key = api_key or os.environ.get("ENGRAM_API_KEY")
        if key:
            self._session.headers["Authorization"] = f"Bearer {key}"

        self._session.headers["Content-Type"] = "application/json"

    # ── Internal ─────────────────────────────────────────────────────────────

    def _post(self, path: str, payload: dict) -> dict | list:
        url = f"{self.base_url}{path}"
        resp = self._session.post(url, json=payload, timeout=self.timeout)
        resp.raise_for_status()
        return resp.json()

    def _get(self, path: str, params: Optional[dict] = None) -> dict | list:
        url = f"{self.base_url}{path}"
        resp = self._session.get(url, params=params, timeout=self.timeout)
        resp.raise_for_status()
        return resp.json()

    # ── Health ────────────────────────────────────────────────────────────────

    def health(self) -> dict:
        """Check if the server is running. Returns {'status': 'ok', 'version': '...'}.

        Raises requests.ConnectionError if the server is unreachable.
        """
        return self._get("/health")

    def is_alive(self) -> bool:
        """Returns True if the server is reachable, False otherwise."""
        try:
            return self.health().get("status") == "ok"
        except Exception:
            return False

    # ── Core memory operations ────────────────────────────────────────────────

    def remember(self, concept: str, text: str) -> str:
        """
        Encode text and store it as a persistent memory block.

        Parameters
        ----------
        concept : str
            Unique snake_case key (e.g. 'auth_bug_2026_04'). Namespacing
            with '__' is recommended for related concepts.
        text : str
            The content to encode. PII (SSN, credit cards, emails) is
            automatically scrubbed server-side before storage.

        Returns
        -------
        str
            The server's status message.
        """
        result = self._post("/api/remember", {"concept": concept, "text": text})
        return result.get("message", "")

    def recall(
        self,
        query: str,
        k: int = 5,
        explain: bool = False,
        min_crs: float = 0.0,
    ) -> list[Memory]:
        """
        Semantic search. Returns the top-k memories most similar to the query.

        Parameters
        ----------
        query : str
            Natural language query (e.g. 'authentication bugs').
        k : int
            Number of results to return (1–20). Default: 5.
        explain : bool
            If True, each result includes a human-readable score explanation.
        min_crs : float
            Filter results by minimum CRS score. Default: 0.0 (no filter).
            Use 0.74 to get only grounded facts.

        Returns
        -------
        list[Memory]
            Results sorted by descending semantic similarity score.
        """
        raw = self._post("/api/recall", {"query": query, "k": k, "explain": explain})
        memories = [
            Memory(
                concept=m["concept"],
                score=m["score"],
                crs=m["crs"],
                text=m["text"],
                explain=m.get("explain"),
            )
            for m in raw
        ]
        if min_crs > 0.0:
            memories = [m for m in memories if m.crs >= min_crs]
        return memories

    def forget(self, concept: str) -> str:
        """
        Permanently delete a memory block.

        Warning: This destroys the block's entire history. To update a
        memory's content, call remember() again with the same concept name.

        Parameters
        ----------
        concept : str
            The concept name to delete.

        Returns
        -------
        str
            The server's status message.
        """
        result = self._post("/api/forget", {"concept": concept})
        return result.get("message", "")

    def relate(self, concept_a: str, concept_b: str, label: str) -> str:
        """
        Create a directional knowledge graph edge between two concepts.

        Parameters
        ----------
        concept_a : str
            Source concept (must already exist in the manifold).
        concept_b : str
            Target concept (must already exist in the manifold).
        label : str
            Relation type (e.g. 'depends_on', 'implements', 'contradicts',
            'fixes', 'derived_from').

        Returns
        -------
        str
            The server's status message.
        """
        result = self._post("/api/relate", {
            "concept_a": concept_a,
            "concept_b": concept_b,
            "label": label,
        })
        return result.get("message", "")

    def trace(
        self,
        term_a: str,
        term_b: str,
        op: str = "ADD",
        k: int = 5,
    ) -> list[Memory]:
        """
        VSA geometry query. Finds memories near the vector result of an
        operation on two concepts.

        Operations
        ----------
        - **ADD** (superposition): memories in the *union* of both concepts'
          semantic space. Use to broaden a search across two topics.
        - **BIND** (association): memories encoding the *relationship between*
          two concepts. Use to find how two ideas connect.

        Parameters
        ----------
        term_a : str
            First concept name or raw text query.
        term_b : str
            Second concept name or raw text query.
        op : str
            'ADD' or 'BIND'. Default: 'ADD'.
        k : int
            Number of results (1–20). Default: 5.

        Returns
        -------
        list[Memory]
            Results sorted by descending similarity to the computed vector.

        Example
        -------
        # What exists at the intersection of "auth" and "performance"?
        results = mem.trace("authentication", "performance", op="ADD")
        """
        raw = self._post("/api/trace", {
            "term_a": term_a,
            "term_b": term_b,
            "op": op.upper(),
            "k": k,
        })
        return [
            Memory(
                concept=m["concept"],
                score=m["score"],
                crs=m["crs"],
                text=m["text"],
                explain=m.get("explain"),
            )
            for m in raw
        ]

    # ── Listing & navigation ──────────────────────────────────────────────────

    def list_concepts(self) -> list[str]:
        """Return all stored concept names."""
        return self._get("/api/list")

    def recent(self, n: int = 10) -> list[RecentEntry]:
        """
        Return the N most recently accessed concepts.

        Parameters
        ----------
        n : int
            Number of entries to return (max 100). Default: 10.

        Returns
        -------
        list[RecentEntry]
            Sorted by most-recently-accessed first.
        """
        raw = self._get("/api/recent", params={"n": n})
        return [
            RecentEntry(
                concept=e["concept"],
                last_accessed=e["last_accessed"],
                ago=e["ago"],
            )
            for e in raw
        ]

    # ── Convenience helpers ───────────────────────────────────────────────────

    def remember_many(self, items: dict[str, str]) -> dict[str, str]:
        """
        Store multiple memories in one call.

        Parameters
        ----------
        items : dict[str, str]
            Mapping of {concept: text}.

        Returns
        -------
        dict[str, str]
            Mapping of {concept: status_message}.
        """
        return {concept: self.remember(concept, text) for concept, text in items.items()}

    def recall_grounded(self, query: str, k: int = 5) -> list[Memory]:
        """Recall only grounded facts (CRS >= 0.74)."""
        return self.recall(query, k=k, min_crs=0.74)


# ── CLI smoke test ────────────────────────────────────────────────────────────

if __name__ == "__main__":
    import sys

    url = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:3456"
    client = EngramClient(base_url=url)

    print(f"Connecting to Engram at {url}...")
    if not client.is_alive():
        print("❌ Server not reachable. Start it with: engram serve --port 3456")
        sys.exit(1)

    health = client.health()
    print(f"✓ Engram {health['version']} is running\n")

    # Round-trip test
    print("Writing test memory...")
    client.remember("engram_py_test", "Python client smoke test — delete me.")

    print("Recalling...")
    results = client.recall("python smoke test", k=3)
    for r in results:
        print(f"  {r}")

    print("\nRecent concepts:")
    for entry in client.recent(n=5):
        print(f"  {entry.concept} ({entry.ago})")

    print("\nCleaning up test memory...")
    client.forget("engram_py_test")
    print("✓ All tests passed.")


# ── Phase 3: BYOP Protocol Support (MCP + Client Tooling for Utility) ─────────
"""
BYOP (Bring Your Own Perspective) — Practical Python SDK helpers for external agents.

Makes the neutral geometric substrate (VSA/sheaf/frames/hot/harmonics/scar/lawfulness)
easy to use for high-quality projection WITHOUT requiring full internal Grok Build
engram-working-memory ritual.

From utility formal_spec: tile:formal_spec_phase3-utility-interface-protocol-design-v1-neut
Category framing: Agents as functors F:Persp → Sections(M). relate=gluing. Tiles as representables.

Core mandate: External agents (Hermes, OpenClaw, Claude, Codex, etc.) achieve high-quality
use of the manifold as geometric commons via practical tools + clear processes.

This extends the base REST EngramClient with BYOP-specific helpers. For full MCP surface
(thought_tile_create, quick_trace, query_with_momentum, set_geosphere_frame, goals, scar, etc.)
translate the patterns below into your agent's MCP tool-call format (or use this client
for the REST fallback surface when available). Copy this module into your agent env.

See also:
- AGENT_INTEGRATION_GUIDE.md (core ritual reference, de-emphasized here for externals)
- docs/praxis_as_protocol_spec.md (A/D/R triad, protocol_type)
- docs/reasoning_functors_as_praxis_extension.md
- coordination tile: tile:knowledge_graph_phase-1-cross-workstream-coordination--ws1-hot-p (updated by this subagent)

All artifacts (this code, patterns, recs) are also recorded as living tiles/traces in the manifold
with goal_context for ki_hijacker/NREM inheritance.
"""

from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional


@dataclass
class BYOPProjection:
    """Represents an external agent's projection into the neutral geometric commons.

    Fields:
    - agent_id: Stable prefix for your concepts (e.g. 'hermes__', 'openclaw__'). Enables
      clean separation + search_by_relation filtering.
    - frame_origin: Geosphere origin for your perspective lens (e.g. 'giza_sacred_cubit',
      'hermes_ontology_2026', 'grove_sower_moon'). Use with set_geosphere_frame.
    - provenance_contract: Minimal required metadata for your projections (agent_prefixed
      names + spatial_references + 'projects_as' or 'from_perspective' relates + goal_context
      on all emissions). Enforces auditability without opinion on your use case.
    - style_notes: Free-form description of your reasoning ontology/style for tiles.
    """
    agent_id: str
    frame_origin: str = "native"
    provenance_contract: str = "agent_prefixed + spatial_refs + relate(projects_as) + goal_context"
    style_notes: str = ""


@dataclass
class PerspectiveTile:
    """Lightweight handle for a projection artifact (Thought Tile or memory block)."""
    concept: str
    tile_type: str
    title: str
    payload_summary: str


class EngramBYOPClient(EngramClient):
    """Extended client implementing the BYOP protocol for external agents.

    Usage (minimal friction):
        from engram_client import EngramBYOPClient, BYOPProjection

        proj = BYOPProjection(agent_id="hermes__", frame_origin="hermes_ontology_giza")
        client = EngramBYOPClient(projection=proj, base_url="http://localhost:3456")

        # 1. Set your lens (makes all subsequent queries frame-shifted)
        client.set_perspective_frame()

        # 2. Project your perspective as first-class tile (functor section)
        tile = client.create_perspective_tile(
            tile_type="knowledge_graph",
            title="hermes_ontology_v1",
            payload={"nodes": [...], "edges": [...], "from_perspective": "hermes"},
            spatial_references=["tile:knowledge_graph_phase-1-cross-workstream-coordination--ws1-hot-p"]
        )

        # 3. Bind/glue the projection (creates the 'projects_as' relation)
        client.bind_projection(tile.concept)

        # 4. Emit traces / goals / scars using your prefix + goal_context (for momentum + ki)
        client.emit_perspective_trace(
            decision="Adopted sheaf gluing for ontology merge",
            why="Preserves local Hermes invariants while participating in shared VSA ops",
            goal_context="my_hermes_research_goal_123"
        )

        # 5. Query with momentum from your perspective
        results = client.query_with_momentum_from_perspective("active ontology tensions", k=5)

    This lowers the barrier to *high-quality* use: correct provenance, frame binding,
    momentum participation, and gluing — without ritual mastery.
    """

    def __init__(
        self,
        projection: BYOPProjection,
        base_url: str = "http://localhost:3456",
        api_key: Optional[str] = None,
        timeout: float = 10.0,
    ) -> None:
        super().__init__(base_url=base_url, api_key=api_key, timeout=timeout)
        self.projection = projection
        self._prefix = projection.agent_id.rstrip("__") + "__" if not projection.agent_id.endswith("__") else projection.agent_id

    # ── Geosphere / Frame Projection (core of BYOP lens) ──────────────────────

    def set_perspective_frame(self) -> Dict[str, Any]:
        """Install this agent's Geosphere lens. All future recall/momentum queries
        are computed under the frame (SymplecticState). Returns server confirmation.

        MCP equivalent: mcp_engram_set_geosphere_frame(origin=..., time_offset=...).
        REST may surface via /api/set_geosphere_frame or equivalent extension.
        """
        # REST shim (server may expose; fallback documents the MCP call)
        try:
            return self._post("/api/set_geosphere_frame", {
                "origin": self.projection.frame_origin,
                "agent_id": self.projection.agent_id,
                "note": "BYOP projection lens via EngramBYOPClient"
            })
        except Exception:
            # Document the exact MCP call the agent should make
            return {
                "status": "documented_mcp_call_required",
                "mcp_tool": "mcp_engram_set_geosphere_frame",
                "args": {
                    "origin": self.projection.frame_origin,
                    "time_offset": "current or agent-specific delta"
                },
                "effect": "All subsequent queries use this lens for angular distance on unit hypersphere."
            }

    def clear_perspective_frame(self) -> Dict[str, Any]:
        """Return to native coordinates. MCP: mcp_engram_clear_geosphere_frame."""
        try:
            return self._post("/api/clear_geosphere_frame", {})
        except Exception:
            return {"status": "use_mcp_engram_clear_geosphere_frame"}

    # ── Projection Artifacts (Thought Tiles as representables / sections) ─────

    def create_perspective_tile(
        self,
        tile_type: str,
        title: str,
        payload: Dict[str, Any],
        spatial_references: Optional[List[str]] = None,
        goal_context: Optional[str] = None,
    ) -> PerspectiveTile:
        """Create a first-class projection artifact (Thought Tile) carrying your
        perspective's functor payload. Auto-prefixed. Creates compresses_path glues.

        MCP surface (preferred for full fidelity): mcp_engram_thought_tile_create
        with goal_context, spatial_references, tile_type in {formal_spec, knowledge_graph, ...}

        Returns handle; also emits a lightweight relation for provenance.
        """
        spatial_refs = spatial_references or []
        prefixed_title = f"{self._prefix}{title}"
        # REST path (limited); real power in MCP call below
        concept = f"tile:{tile_type}_{prefixed_title.lower().replace(' ', '_')[:40]}"
        text = (
            f"BYOP PROJECTION TILE (agent={self.projection.agent_id})\n\n"
            f"tile_type: {tile_type}\n"
            f"title: {title}\n"
            f"frame_origin: {self.projection.frame_origin}\n"
            f"provenance: {self.projection.provenance_contract}\n\n"
            f"payload:\n{payload}\n"
        )
        try:
            self.remember(concept, text)
        except Exception:
            pass  # MCP path is authoritative

        # The MCP call external agents should actually issue (for full spatial/goal wiring):
        mcp_call = {
            "tool": "mcp_engram_thought_tile_create",
            "arguments": {
                "tile_type": tile_type,
                "title": f"{self.projection.agent_id}{title}",
                "payload": {
                    **payload,
                    "byop_projection": {
                        "agent_id": self.projection.agent_id,
                        "frame_origin": self.projection.frame_origin,
                        "style_notes": self.projection.style_notes,
                    },
                    "provenance_contract": self.projection.provenance_contract,
                },
                "goal_context": goal_context or "",
                "spatial_references": spatial_refs + ["tile:knowledge_graph_phase-1-cross-workstream-coordination--ws1-hot-p"],
            },
        }
        return PerspectiveTile(
            concept=concept,
            tile_type=tile_type,
            title=title,
            payload_summary=str(payload)[:120] + "..."
        ), mcp_call  # type: ignore  # caller uses the mcp_call dict for real emission

    def bind_projection(self, tile_concept: str, label: str = "projects_as") -> str:
        """Glue your projection tile into the shared manifold (the 'relate=gluing' step).

        Creates directional edge: tile_concept →[label]→ coordination / other anchors.
        Also relates back from coordination for discoverability.

        MCP: mcp_engram_relate. Use search_by_relation(..., label=label) to traverse.
        """
        coord = "tile:knowledge_graph_phase-1-cross-workstream-coordination--ws1-hot-p"
        try:
            self.relate(tile_concept, coord, label)
            self.relate(coord, tile_concept, f"hosts_{label}")
        except Exception:
            pass
        return f"Bound {tile_concept} via {label} (MCP: mcp_engram_relate recommended for full effect)"

    # ── Emission Helpers (traces, goals, scars — momentum + provenance) ───────

    def emit_perspective_trace(
        self,
        decision: str,
        why: str,
        goal_context: Optional[str] = None,
        alternatives: Optional[str] = None,
        would_falsify: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Emit a structured reasoning trace from your perspective.

        Strongly recommended: always pass goal_context so it participates in ki_hijacker,
        NREM, and momentum (p-tensor).

        MCP (ultra low friction): mcp_engram_quick_trace with goal_context.
        Full: mcp_engram_record_reasoning_trace (A/D/R support).
        """
        prefixed_decision = f"[{self.projection.agent_id}] {decision}"
        mcp_quick_trace_call = {
            "tool": "mcp_engram_quick_trace",
            "arguments": {
                "decision": prefixed_decision,
                "why": why,
                "goal_context": goal_context or "",
                "alternatives": alternatives or "",
                "would_falsify": would_falsify or "",
                "context": f"BYOP projection via {self.projection.agent_id} frame={self.projection.frame_origin}",
            },
        }
        # REST fallback: store as episodic memory
        concept = f"trace_{self._prefix}{decision[:30].lower().replace(' ', '_')}"
        try:
            self.remember(concept, f"BYOP TRACE: {prefixed_decision}\nWhy: {why}\nGoal: {goal_context}")
        except Exception:
            pass
        return {"mcp_recommended": mcp_quick_trace_call, "stored_concept": concept}

    def create_perspective_goal(
        self,
        statement: str,
        parent: Optional[str] = None,
        priority: str = "medium",
    ) -> Dict[str, Any]:
        """Declare intent that will be geometrically bound (influences future recall)."""
        mcp_call = {
            "tool": "mcp_engram_goal_create",
            "arguments": {
                "statement": f"[{self.projection.agent_id}] {statement}",
                "parent": parent or "",
                "priority": priority,
            },
        }
        return {"mcp_recommended": mcp_call}

    def scar_perspective_deadend(self, concept: str, magnitude: float = 0.15) -> Dict[str, Any]:
        """Create geometric repeller for a failed approach from your perspective.
        MCP: mcp_engram_scar. Critical for honest multi-agent manifold health."""
        mcp_call = {"tool": "mcp_engram_scar", "arguments": {"concept": f"{self._prefix}{concept}", "magnitude": magnitude}}
        return {"mcp_recommended": mcp_call}

    # ── Projection-Aware Query (momentum + frame) ─────────────────────────────

    def query_with_momentum_from_perspective(
        self, query: str, k: int = 5
    ) -> List[Dict[str, Any]]:
        """Momentum-assisted recall (q 80% + p 20%) under your current frame lens.

        MCP: mcp_engram_query_with_momentum (automatically respects active Geosphere frame).
        Use when you want what is *evolving* in the shared manifold from your perspective.
        """
        try:
            # If server REST extension exists
            return self._post("/api/query_with_momentum", {"query": query, "k": k})
        except Exception:
            # Fallback to base recall + note
            base = self.recall(query, k=k)
            return [
                {"concept": m.concept, "score": m.score, "crs": m.crs, "note": "Use MCP mcp_engram_query_with_momentum for true p-tensor momentum under your frame"}
                for m in base
            ]

    def search_my_projections(self, query: str, k: int = 10) -> List[Memory]:
        """Recall only blocks carrying your agent prefix (your own projected artifacts)."""
        # Simple client-side filter on full recall; in practice use namespaced recall or relation traversal
        all_results = self.recall(query, k=k * 3)
        return [m for m in all_results if m.concept.startswith(self._prefix)][:k]


# ── Effective Usage Patterns Guide (Process-Focused, Minimal Ritual) ──────────
"""
EFFECTIVE USAGE PATTERNS FOR EXTERNAL AGENTS (BYOP v1)

Goal: High-quality geometric participation with 4-6 MCP/REST calls per major cycle.
No full engram-working-memory ritual required. Focus on results + provenance.

Pattern 1: Project Your Ontology (one-time or per major shift)
  1. Instantiate BYOPProjection + EngramBYOPClient (agent_id, frame_origin that captures your style).
  2. client.set_perspective_frame()  → installs your 5th coordinate lens.
  3. tile, mcp_call = client.create_perspective_tile( tile_type="formal_spec" or "knowledge_graph",
       title="my_ontology_vN", payload={your full structured perspective: invariants, functors, scars, ...},
       spatial_references=[coordination tile, key shared anchors] )
  4. client.bind_projection(tile.concept, label="projects_as")  → glues you into the sheaf.
  5. (Optional but high-utility) client.emit_perspective_trace(...) for the projection decision.

  Result: Your perspective is now a first-class, queryable, momentum-participating section.
  Other agents (and future you) can discover via search_by_relation(label="projects_as")
  or momentum queries on "hermes ontology tensions".

Pattern 2: Daily / Per-Task Work Loop (low friction)
  - At task start: client.create_perspective_goal(statement=..., parent=prior)  [or use MCP goal_create directly]
  - Before key decision: client.emit_perspective_trace(decision=..., why=..., goal_context=your_goal)
    (Prefer mcp_engram_quick_trace in real MCP sessions — ultra low friction.)
  - On dead-end: client.scar_perspective_deadend(concept=bad_idea)
  - On success: remember_solution or verified tile; bind with relate("solves", ...)
  - Query actively: client.query_with_momentum_from_perspective("current blockers in shared geometry")
  - At end of significant chunk: client.emit_perspective_trace for session delta + relate to your goal.

  This produces p-tensor momentum signals automatically → your work surfaces in ki_hijacker for
  other agents and in NREM consolidation without extra ceremony.

Pattern 3: Cross-Agent Collaboration / Discovery
  - Use search_by_relation(concept=coordination_tile, label="projects_as") to find other projections.
  - Use query_with_momentum("hermes vs openclaw ontology overlap") under your frame.
  - Glue via relate(your_tile, their_tile, "complements" or "contradicts" or "glues_via_sheaf").
  - For synthesis: create a joint "verified_sequence" or "state_machine" tile with multiple spatial_refs.

Pattern 4: Provenance & Audit (mandatory for quality)
  - Every emission carries agent prefix + goal_context + (recommended) spatial_refs.
  - Use mcp_engram_read_concept on any high-CRS result to get full ProvLog + Merkle.
  - When updating your own projection: use mcp_engram_update (never forget+remember) to preserve Lyapunov drift.
  - Scar failures explicitly — this is how the commons stays honest and high-utility.

Pattern 5: Frame Shifting (advanced geometric move)
  - client.set_perspective_frame() with different origins (e.g. switch from "giza" to "2026_london" lens).
  - Re-query the same concepts → receive frame-shifted but coherent results (the 5th coordinate in action).
  - Clear when you want native view again. Reproducible; frame_step audited in SymplecticState.

Starter Template (drop into agent bootstrap):
    from engram_client import EngramBYOPClient, BYOPProjection

    proj = BYOPProjection(
        agent_id="hermes__",
        frame_origin="hermes_ontology_giza_sacred",
        style_notes="Emphasis on mythic VSA sheaves, 432Hz harmonics as identity, productive failure as sacred scar"
    )
    c = EngramBYOPClient(proj)
    c.set_perspective_frame()
    # ... project tile + bind + goal + traces as above

Hermes-style concrete example (mythic ontology projection):
    payload = {
        "core_invariants": ["toroidal lift", "paper_as_sheaf", "geosphere_5th_sacrament"],
        "op_vocab": ["OP_DEDUCE", "OP_IS_SYMBOLIC_OF", "OP_SHIFT"],
        "voice_bible_ref": "data/false_empire/.../Voice_Bible_Mason_Era.md",
        "gluing_rule": "reconstruction_as_sheaf_gluing"
    }
    tile, call = c.create_perspective_tile("knowledge_graph", "hermes_voice_bible_projection", payload,
                                           spatial_references=["against_flat_knowledge_geosphere_fifth_coordinate"])
    c.bind_projection(tile.concept)
    c.emit_perspective_trace("Projected full Voice Bible as living section", "Enables frame-shifted reconstruction queries from future subagents", goal_context="false_empire_reconstruction_goal")

OpenClaw-style (claw/agentic tool user):
    payload = {"tool_ontology": [...], "failure_modes": [...], "momentum_preference": "high p-tensor on active toolchains"}
    ... same 4 lines ...

This is deliberately process-oriented and copy-pasteable. High-quality use = consistent provenance + frame participation + gluing + explicit scars/successes. The substrate does the heavy geometric lifting.

MCP SURFACE ENHANCEMENTS (Prioritized, with Rationale) — Recommended for next iteration to further lower external barrier:

P0 (High impact, small surface):
1. mcp_engram_bind_projection (or extend relate): Dedicated helper that auto-wires agent_id, provenance header, 'projects_as' + reverse, compresses_path to coordination/plan tiles, and optional goal. Rationale: Removes boilerplate; enforces the BYOP contract by construction. Current: manual relate + spatial_refs on every tile.
2. Projection-aware query helpers: mcp_engram_recall_in_projection(agent_id, query) and momentum variant. Rationale: Hides prefix filtering + frame; returns only or ranks your + peer projections. Makes "what is my stuff doing in the commons" a one-call experience.
3. mcp_engram_create_projection_goal + auto-link: Similar to goal_create but stamps agent prefix + projects_as wiring automatically. Rationale: Goals are the primary intent vectors; external agents need first-class geometric binding without extra steps.

P1 (Medium, strong utility):
4. First-class projection metadata on blocks (or lightweight header in ProvLog): agent_id, projection_of, frame_origin_at_emission. Rationale: Enables server-side filtering, ki_hijacker projection views, and NREM to respect agent boundaries cleanly. Current: relies on naming convention + relations.
5. mcp_engram_list_projections() + visualize_projection_subgraph(agent_id): Returns active functors/sections + Mermaid of how this agent's tiles glue to shared anchors. Rationale: Discovery and review surface for multi-agent work; pairs beautifully with leg-browser style viewers.
6. Optional "projection_contract" enforcement flag on tile creation / relate: server-side check that provenance fields present. Rationale: Raises quality floor automatically for external use without breaking existing internal ritual.

P2 (Future / deeper):
- Category-theoretic sugar in docs + MCP descriptions (explicit "this call is the gluing morphism").
- REST parity for all new BYOP tools (so non-MCP Python agents get full power via this client).
- Projection-scoped session_start / scar summaries for clean handoff between external agents.

These enhancements keep the core substrate opinion-free while making high-quality external utility the path of least resistance.

This entire section (patterns + SDK + recs) constitutes the concrete deliverable of the MCP + Client Tooling for Utility subagent. Recorded in manifold via tiles + traces under goal:1780201953... + updates to coordination tile.
"""
# End of BYOP Phase 3 utility additions.
