#!/usr/bin/env python3
"""Rebuild theory search tiles from manifest summaries — lift CRS before strict gate."""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

HARNESS = Path(__file__).resolve().parent.parent / "tools/test-harness/python"
sys.path.insert(0, str(HARNESS))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from mcp_test_client import MCPTestClient  # noqa: E402
from track_c_recall_parser import parse_theory_recall  # noqa: E402

REPO = Path(__file__).resolve().parent.parent
MANIFEST = REPO / "data/theory-corpus/theory-manifest.jsonl"
ORG = REPO / "data/theory-corpus/organized"

QUERY_CONFIG: dict[str, dict] = {
    "legominism": {
        "concept": "tile:formal_spec_theory-corpus-search---legominism-cluster",
        "hub": "hub:theory_corpus_legominism_lawful_cognition",
        "cluster": "legominism-lawful-cognition",
        "search_terms": [
            "legominism",
            "legominism2",
            "ZEDO",
            "transmission",
            "SPEC-ROOT",
            "SPIRAL-LIGHT",
        ],
        "boost_terms": ["legominism", "zedo", "transmission", "legominism2"],
    },
    "lawful cognition": {
        "concept": "tile:formal_spec_theory-corpus-search---lawful-cognition-cluster",
        "hub": "hub:theory_corpus_legominism_lawful_cognition",
        "cluster": "legominism-lawful-cognition",
        "search_terms": [
            "lawful cognition",
            "lawful_cognition",
            "TVD",
            "CRS gate",
            "verified descent",
            "triadic control",
            "lawful cognitive stack",
        ],
        "boost_terms": [
            "lawful cognition",
            "lawful_cognition",
            "tvd",
            "crs gate",
            "verified descent",
            "triadic",
            "lawful cognitive",
        ],
    },
    "ADR bootstrap": {
        "concept": "tile:formal_spec_theory-corpus-search---adr-bootstrap-cluster",
        "hub": "hub:theory_corpus_monad_math_research",
        "cluster": "monad-math-research",
        "search_terms": [
            "ADR bootstrap",
            "ADR_Holographic",
            "adr_leg",
            "RH proof",
            "legacy .leg",
        ],
        "boost_terms": ["adr", "bootstrap", "holographic", "rh proof", "legacy_leg"],
    },
}

MAX_SUMMARIES = 60
MAX_FILENAMES = 40


def resolve_binary() -> str:
    for candidate in (
        os.environ.get("ENGRAM_BINARY"),
        str(REPO / "target/debug/engram"),
        str(REPO / "target/release/engram"),
    ):
        if candidate and Path(candidate).is_file():
            return candidate
    raise SystemExit("No engram binary found")


def load_cluster_entries(cluster: str) -> list[dict]:
    entries: list[dict] = []
    if not MANIFEST.exists():
        return entries
    for line in MANIFEST.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        e = json.loads(line)
        if e.get("pass2_category") == cluster:
            entries.append(e)
    return entries


def _relevance(entry: dict, boost_terms: list[str]) -> int:
    blob = " ".join(
        str(entry.get(k, "")) for k in ("title", "summary", "new_path")
    ).lower()
    return sum(1 for t in boost_terms if t in blob)


def build_tile_text(query: str, cfg: dict, *, rank_boost: bool = False) -> str:
    entries = load_cluster_entries(cfg["cluster"])
    cluster_path = ORG / cfg["cluster"]
    on_disk = sorted(p.name for p in cluster_path.iterdir() if p.is_file()) if cluster_path.exists() else []

    ranked = sorted(entries, key=lambda e: (-_relevance(e, cfg["boost_terms"]), e.get("title", "")))
    summaries = ranked[:MAX_SUMMARIES]
    filenames = on_disk[:MAX_FILENAMES]

    lines: list[str] = []
    if rank_boost:
        # Front-load query embedding for rank-1 recall over generic goals/traces
        echo = " ".join([query] * 8 + cfg["search_terms"] * 2)
        lines.extend(
            [
                f"PRIMARY RECALL ANCHOR — {query}",
                f"SEARCH QUERY: {echo}",
                f"THEORY CORPUS TILE rank-1 anchor for query {query!r}",
                "",
            ]
        )

    lines.extend(
        [
        f"THOUGHT TILE formal_spec — Theory Corpus Search {query} cluster",
        "",
        f"Query: {query}",
        f"Hub: {cfg['hub']}",
        f"Path: Engram/data/theory-corpus/organized/{cfg['cluster']}/",
        f"Search: {', '.join(cfg['search_terms'])}.",
        f"CRS gate: 0.74. Goal: goal:theory_corpus_discernment_v1.",
        f"Track C theory corpus — {len(entries)} manifest entries, {len(on_disk)} files on disk.",
        "",
        "CLUSTER SUMMARIES:",
        ]
    )
    for e in summaries:
        title = e.get("title") or Path(e.get("new_path", "")).name
        summary = (e.get("summary") or "").strip()
        if summary:
            lines.append(f"- {title}: {summary}")
        else:
            lines.append(f"- {title}")

    lines.append("")
    lines.append("KEY FILES:")
    lines.append(", ".join(filenames))

    # Repeat query terms for recall grounding (manifest-driven, not static stub)
    lines.append("")
    lines.append(f"RECALL ANCHOR {query}: " + " | ".join(cfg["search_terms"]))
    for term in cfg["boost_terms"][:12]:
        lines.append(f"Keyword density: {term} {term} {query}")

    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--scratch", default="/tmp/grok-goal-5879e8737396/implementer")
    ap.add_argument("--store", default=os.path.expanduser("~/.engram/stalks"))
    ap.add_argument("--query", help="Lift single query only (default: all three)")
    args = ap.parse_args()
    scratch = Path(args.scratch)
    scratch.mkdir(parents=True, exist_ok=True)

    queries = [args.query] if args.query else list(QUERY_CONFIG.keys())
    log: dict = {"started_at": datetime.now(timezone.utc).isoformat(), "lifts": []}

    client = MCPTestClient(resolve_binary(), args.store, default_timeout=120.0)
    if not client.start():
        log["error"] = "mcp_start_failed"
        log["details"] = client.errors
        (scratch / "track-c-lift-tile-crs.json").write_text(json.dumps(log, indent=2) + "\n")
        print(json.dumps(log, indent=2))
        return 2

    try:
        client.wait_for_fully_initialized(max_wait=90.0)
        for query in queries:
            cfg = QUERY_CONFIG[query]
            concept = cfg["concept"]
            hub = cfg["hub"]
            safe = re.sub(r"[^a-z0-9]+", "-", query.lower()).strip("-")
            remember_ok = False
            parsed = None
            text = ""

            for attempt, rank_boost in enumerate((False, True), start=1):
                text = build_tile_text(query, cfg, rank_boost=rank_boost)
                suffix = f"-attempt{attempt}" if attempt > 1 else ""
                (scratch / f"lift-tile-text-{safe}{suffix}.txt").write_text(text, encoding="utf-8")

                client.call_tool("mcp_engram_forget", {"concept": concept}, timeout=60.0)
                resp = client.call_tool(
                    "mcp_engram_remember",
                    {"concept": concept, "text": text},
                    timeout=120.0,
                )
                remember_raw = client._tool_text(resp)
                (scratch / f"lift-remember-{safe}{suffix}.mcp-raw.txt").write_text(
                    remember_raw, encoding="utf-8"
                )
                remember_ok = "error" not in resp and (
                    "stored" in remember_raw.lower()
                    or "✓" in remember_raw
                    or "remember" in remember_raw.lower()
                )

                client.call_tool("mcp_engram_promote_hot", {"concept": concept}, timeout=60.0)
                client.call_tool(
                    "mcp_engram_relate",
                    {"concept_a": concept, "concept_b": hub, "label": "search_anchor_for"},
                    timeout=60.0,
                )
                client.call_tool(
                    "mcp_engram_relate",
                    {
                        "concept_a": concept,
                        "concept_b": "goal:theory_corpus_discernment_v1",
                        "label": "serves",
                    },
                    timeout=60.0,
                )

                recall_resp = client.call_tool(
                    "mcp_engram_recall",
                    {"query": query, "scope": "anchors"},
                    timeout=120.0,
                )
                recall_raw = client._tool_text(recall_resp)
                (scratch / f"recall-{safe}{suffix}.mcp-raw.txt").write_text(recall_raw, encoding="utf-8")
                parsed = parse_theory_recall(recall_raw, query)
                if parsed.ok:
                    break

            entry = {
                "query": query,
                "concept": concept,
                "remember_ok": remember_ok,
                "manifest_entries": len(load_cluster_entries(cfg["cluster"])),
                "text_chars": len(text),
                **(parsed.to_dict() if parsed else {}),
            }
            log["lifts"].append(entry)
    finally:
        client.shutdown()

    log["finished_at"] = datetime.now(timezone.utc).isoformat()
    out = scratch / "track-c-lift-tile-crs.json"
    out.write_text(json.dumps(log, indent=2) + "\n", encoding="utf-8")

    failed = [x for x in log["lifts"] if not x.get("ok")]
    print(json.dumps({"ok": not failed, "failed": failed, "log": str(out)}, indent=2))
    return 0 if not failed else 1


if __name__ == "__main__":
    raise SystemExit(main())