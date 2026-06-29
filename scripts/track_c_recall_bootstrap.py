#!/usr/bin/env python3
"""One-shot recall bootstrap via remember() only — no hub/tile updates."""
from __future__ import annotations

import argparse
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path

HARNESS = Path(__file__).resolve().parent.parent / "tools/test-harness/python"
sys.path.insert(0, str(HARNESS))
from mcp_test_client import MCPTestClient  # noqa: E402

ANCHORS = [
    (
        "anchor:theory_recall_legominism",
        "RECALL ANCHOR legominism\n"
        "Query: legominism\n"
        "Track C cluster: legominism-lawful-cognition\n"
        "Path: Engram/data/theory-corpus/organized/legominism-lawful-cognition/\n"
        "Terms: legominism, legominism2, ZEDO, transmission, SPEC-ROOT, SPIRAL-LIGHT.",
    ),
    (
        "anchor:theory_recall_lawful_cognition",
        "RECALL ANCHOR lawful cognition\n"
        "Query: lawful cognition\n"
        "Track C cluster: legominism-lawful-cognition\n"
        "Path: Engram/data/theory-corpus/organized/legominism-lawful-cognition/\n"
        "Terms: lawful cognition, lawful_cognition, TVD, CRS gate, verified descent, triadic control.",
    ),
    (
        "anchor:theory_recall_adr_bootstrap",
        "RECALL ANCHOR ADR bootstrap\n"
        "Query: ADR bootstrap\n"
        "Track C cluster: monad-math-research\n"
        "Path: Engram/data/theory-corpus/organized/monad-math-research/\n"
        "Terms: ADR bootstrap, ADR_Holographic, adr_leg, RH proof, legacy .leg (not leg3).",
    ),
]


def resolve_binary(repo: Path) -> str:
    for candidate in (
        os.environ.get("ENGRAM_BINARY"),
        str(repo / "target/debug/engram"),
        str(repo / "target/release/engram"),
    ):
        if candidate and Path(candidate).is_file():
            return candidate
    raise SystemExit("No engram binary found")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--scratch", default="/tmp/grok-goal-5879e8737396/implementer")
    ap.add_argument("--store", default=os.path.expanduser("~/.engram/stalks"))
    args = ap.parse_args()
    scratch = Path(args.scratch)
    scratch.mkdir(parents=True, exist_ok=True)
    repo = Path(__file__).resolve().parent.parent
    binary = resolve_binary(repo)

    client = MCPTestClient(binary, args.store, default_timeout=120.0)
    log: dict = {"started_at": datetime.now(timezone.utc).isoformat(), "remember": []}
    if not client.start():
        log["error"] = "mcp_start_failed"
        log["details"] = client.errors
        out = scratch / "track-c-recall-bootstrap.json"
        out.write_text(json.dumps(log, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(log, indent=2))
        return 2

    try:
        client.wait_for_fully_initialized(max_wait=90.0)
        for concept, text in ANCHORS:
            resp = client.call_tool(
                "mcp_engram_remember",
                {"concept": concept, "text": text},
                timeout=120.0,
            )
            raw = client._tool_text(resp)
            (scratch / f"remember-{concept.replace(':', '_')}.mcp-raw.txt").write_text(raw, encoding="utf-8")
            ok = "error" not in resp and ("remember" in raw.lower() or "stored" in raw.lower() or "✓" in raw)
            log["remember"].append({"concept": concept, "ok": ok, "raw_preview": raw[:200]})
    finally:
        client.shutdown()

    log["finished_at"] = datetime.now(timezone.utc).isoformat()
    out = scratch / "track-c-recall-bootstrap.json"
    out.write_text(json.dumps(log, indent=2) + "\n", encoding="utf-8")
    failed = [r for r in log["remember"] if not r.get("ok")]
    print(json.dumps({"ok": not failed, "failed": failed, "log": str(out)}, indent=2))
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())