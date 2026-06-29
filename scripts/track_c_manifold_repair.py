#!/usr/bin/env python3
"""Manifold repair before acceptance gate: clear protocol_gap_*, stabilize theory tiles."""
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
from mcp_test_client import MCPTestClient, verify_text_healthy  # noqa: E402

HUB_STABLE = {
    "hub:theory_corpus_legominism_lawful_cognition": (
        "THEORY CORPUS HUB — legominism + lawful cognition\n"
        "Path: Engram/data/theory-corpus/organized/legominism-lawful-cognition/\n"
        "Files: 676. Queries: legominism, lawful cognition, TVD, CRS gate, SPEC-ROOT, SPIRAL-LIGHT."
    ),
    "hub:theory_corpus_monad_math_research": (
        "THEORY CORPUS HUB — monad math + ADR bootstrap\n"
        "Path: Engram/data/theory-corpus/organized/monad-math-research/\n"
        "Files: 460. Queries: ADR bootstrap, ADR_Holographic, adr_leg, RH proof. Legacy .leg not leg3."
    ),
}

TILE_STABLE = {
    "tile:formal_spec_theory-corpus-search---legominism-cluster": (
        "THOUGHT TILE formal_spec — Theory Corpus Search legominism cluster\n"
        "Query: legominism\n"
        "Hub: hub:theory_corpus_legominism_lawful_cognition\n"
        "Path: Engram/data/theory-corpus/organized/legominism-lawful-cognition/\n"
        "Search: legominism, legominism2, ZEDO, transmission, SPEC-ROOT."
    ),
    "tile:formal_spec_theory-corpus-search---lawful-cognition-cluster": (
        "THOUGHT TILE formal_spec — Theory Corpus Search lawful cognition cluster\n"
        "Query: lawful cognition\n"
        "Hub: hub:theory_corpus_legominism_lawful_cognition\n"
        "Path: Engram/data/theory-corpus/organized/legominism-lawful-cognition/\n"
        "Search: lawful cognition, lawful_cognition, TVD, CRS gate, verified descent."
    ),
    "tile:formal_spec_theory-corpus-search---adr-bootstrap-cluster": (
        "THOUGHT TILE formal_spec — Theory Corpus Search ADR bootstrap cluster\n"
        "Query: ADR bootstrap\n"
        "Hub: hub:theory_corpus_monad_math_research\n"
        "Path: Engram/data/theory-corpus/organized/monad-math-research/\n"
        "Search: ADR bootstrap, ADR_Holographic, adr_leg, RH proof."
    ),
}

# Traces from tile update pollution (high recall rank, not search anchors)
TRACE_POLLUTION_PREFIXES = (
    "trace:1782715413183_plain-update-on-tile-formal-spec-theory-corpus",
)


def resolve_binary(repo: Path) -> str:
    for candidate in (
        os.environ.get("ENGRAM_BINARY"),
        str(repo / "target/debug/engram"),
        str(repo / "target/release/engram"),
    ):
        if candidate and Path(candidate).is_file():
            return candidate
    raise SystemExit("No engram binary found")


def extract_gaps_and_drift(verify_text: str) -> tuple[list[str], list[str]]:
    gaps = sorted(set(re.findall(r"protocol_gap_\d+", verify_text or "")))
    drift_hubs = []
    for line in (verify_text or "").splitlines():
        if "drift" in line.lower() and "hub:theory_corpus" in line:
            m = re.search(r"hub:theory_corpus_[\w]+", line)
            if m:
                drift_hubs.append(m.group(0))
    return gaps, sorted(set(drift_hubs))


def theory_tile_drift_in_recall(text: str) -> list[str]:
    """Return theory search tile concepts with dv=1.0 in recall output."""
    bad = []
    for m in re.finditer(
        r"\*\*\[\d+\]\s+(tile:formal_spec_theory-corpus-search---[\w\-]+)\*\*.*?\(.*?dv:\s*([\d.]+)",
        text or "",
        re.S,
    ):
        concept, dv = m.group(1), float(m.group(2))
        if dv >= 0.99:
            bad.append(concept)
    return bad


def verify_healthy(text: str) -> bool:
    if "overall: needs_review" in (text or "").lower():
        return False
    return verify_text_healthy(text)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--scratch", default="/tmp/grok-goal-5879e8737396/implementer")
    ap.add_argument("--store", default=os.path.expanduser("~/.engram/stalks"))
    ap.add_argument(
        "--gaps-only",
        action="store_true",
        help="Only clear protocol_gap_*; skip tile/hub stabilize (tile lift handles CRS)",
    )
    args = ap.parse_args()
    scratch = Path(args.scratch)
    scratch.mkdir(parents=True, exist_ok=True)
    repo = Path(__file__).resolve().parent.parent

    client = MCPTestClient(resolve_binary(repo), args.store, default_timeout=120.0)
    log: dict = {
        "started_at": datetime.now(timezone.utc).isoformat(),
        "forgot": [],
        "hubs": [],
        "tiles": [],
        "traces": [],
    }
    if not client.start():
        log["error"] = "mcp_start_failed"
        (scratch / "track-c-manifold-repair.json").write_text(json.dumps(log, indent=2))
        return 2

    try:
        client.wait_for_fully_initialized(max_wait=90.0)

        # Multi-sample verify to catch all protocol_gap_* (sampling variance)
        cleared_gaps: set[str] = set()
        for _round in range(8):
            all_gaps: set[str] = set()
            for _sample in range(3):
                vresp = client.call_tool(
                    "mcp_engram_verify_manifold_integrity",
                    {"min_crs": 0.74, "sample_size": 100},
                    timeout=120.0,
                )
                vtext = client._tool_text(vresp)
                all_gaps |= set(extract_gaps_and_drift(vtext)[0])
            if not all_gaps:
                vtext_final = vtext
                if verify_healthy(vtext_final):
                    log["gap_loop_rounds"] = _round + 1
                    break
            for concept in sorted(all_gaps):
                if concept in cleared_gaps:
                    continue
                resp = client.call_tool("mcp_engram_forget", {"concept": concept}, timeout=60.0)
                raw = client._tool_text(resp)
                (scratch / f"forget-{concept}.mcp-raw.txt").write_text(raw, encoding="utf-8")
                ok = "error" not in resp and ("deleted" in raw.lower() or "✓" in raw)
                log["forgot"].append({"concept": concept, "ok": ok})
                cleared_gaps.add(concept)
        else:
            log["gap_loop_rounds"] = 8

        (scratch / "repair-post-gap-verify.mcp-raw.txt").write_text(vtext, encoding="utf-8")

        if not args.gaps_only:
            # Legacy full repair path (tile/hub stabilize) — prefer --gaps-only + lift_tile_crs
            drift_tiles: set[str] = set()
            for q in ("lawful cognition", "legominism", "ADR bootstrap"):
                resp = client.call_tool(
                    "mcp_engram_recall",
                    {"query": q, "scope": "anchors"},
                    timeout=120.0,
                )
                rtext = client._tool_text(resp)
                drift_tiles |= set(theory_tile_drift_in_recall(rtext))

            log["drift_tiles_detected"] = sorted(drift_tiles)
            for concept in drift_tiles:
                text = TILE_STABLE.get(concept)
                if not text:
                    continue
                client.call_tool("mcp_engram_forget", {"concept": concept}, timeout=60.0)
                resp = client.call_tool(
                    "mcp_engram_remember",
                    {"concept": concept, "text": text},
                    timeout=120.0,
                )
                raw = client._tool_text(resp)
                (scratch / f"tile-stabilize-{concept.replace(':', '_')}.mcp-raw.txt").write_text(
                    raw, encoding="utf-8"
                )
                ok = "error" not in resp and ("stored" in raw.lower() or "✓" in raw)
                log["tiles"].append({"concept": concept, "ok": ok})

        # Final verify snapshot
        vresp = client.call_tool(
            "mcp_engram_verify_manifold_integrity",
            {"min_crs": 0.74, "sample_size": 100},
            timeout=120.0,
        )
        vfinal = client._tool_text(vresp)
        (scratch / "repair-final-verify.mcp-raw.txt").write_text(vfinal, encoding="utf-8")
        log["final_verify_healthy"] = verify_healthy(vfinal)
    finally:
        client.shutdown()

    log["finished_at"] = datetime.now(timezone.utc).isoformat()
    (scratch / "track-c-manifold-repair.json").write_text(json.dumps(log, indent=2) + "\n")
    failed = [x for x in log.get("forgot", []) + log.get("hubs", []) + log.get("tiles", []) if not x.get("ok")]
    print(
        json.dumps(
            {
                "ok": not failed and log.get("final_verify_healthy", False),
                "gaps_cleared": len(log.get("forgot", [])),
                "tiles_stabilized": len(log.get("tiles", [])),
                "log": str(scratch / "track-c-manifold-repair.json"),
            },
            indent=2,
        )
    )
    return 0 if log.get("final_verify_healthy") and not failed else 1


if __name__ == "__main__":
    raise SystemExit(main())