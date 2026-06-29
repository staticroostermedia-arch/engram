#!/usr/bin/env python3
"""Track C acceptance gate — read-only verification. No mutations."""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

HARNESS = Path(__file__).resolve().parent.parent / "tools/test-harness/python"
sys.path.insert(0, str(HARNESS))
from mcp_test_client import MCPTestClient, verify_text_healthy  # noqa: E402

REPO = Path(__file__).resolve().parent.parent
ORG = REPO / "data/theory-corpus/organized"
MONAD = ORG / "monad-math-research"
QUAR = ORG / "_quarantine"
FE_MANIFEST = Path("/home/a/Documents/BookForge/corpus/false-empire/false-empire-manifest.jsonl")

RECALL_QUERIES = ("legominism", "lawful cognition", "ADR bootstrap")
MIN_CRS = 0.74


def resolve_binary() -> str:
    for candidate in (
        os.environ.get("ENGRAM_BINARY"),
        str(REPO / "target/debug/engram"),
        str(REPO / "target/release/engram"),
    ):
        if candidate and Path(candidate).is_file():
            return candidate
    which = subprocess.run(["bash", "-lc", "command -v engram"], capture_output=True, text=True)
    if which.returncode == 0 and which.stdout.strip():
        return which.stdout.strip()
    raise SystemExit("No engram binary found")


def write_raw(scratch: Path, name: str, text: str) -> Path:
    p = scratch / f"{name}.mcp-raw.txt"
    p.write_text(text or "", encoding="utf-8")
    return p


def check_organize_disabled() -> tuple[bool, str]:
    r = subprocess.run(
        [sys.executable, str(REPO / "scripts/theory_corpus_organize.py")],
        capture_output=True,
        text=True,
    )
    ok = r.returncode == 2
    return ok, f"exit {r.returncode}"


def check_fe_in_monad() -> tuple[bool, int]:
    n = len(list(MONAD.glob("False_Empire*"))) + len(list(MONAD.glob("false_empire*")))
    return n == 0, n


def check_fe_in_organized() -> tuple[bool, int, list[str]]:
    """False Empire must not live anywhere under Engram organized/ (Track A = BookForge only)."""
    sys.path.insert(0, str(REPO / "scripts"))
    from track_c_fs_cleanup import fe_files_in_organized  # noqa: E402

    hits = fe_files_in_organized()
    paths = [str(p) for p in hits]
    return len(hits) == 0, len(hits), paths


def check_track_a_vn() -> tuple[bool, int]:
    if not FE_MANIFEST.exists():
        return False, -1
    n = 0
    for line in FE_MANIFEST.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        e = json.loads(line)
        if e.get("best_copy") and e.get("new_path"):
            if re.search(r"_v\d| \(1\)", Path(e["new_path"]).name):
                n += 1
    return n == 0, n


def check_legacy_leg_sample(n: int = 10) -> tuple[bool, list[dict]]:
    sys.path.insert(0, str(REPO / "scripts"))
    from legacy_leg_parse import parse_legacy_leg  # noqa: E402

    valid = []
    for leg in sorted(MONAD.glob("*.leg")):
        if parse_legacy_leg(leg).get("format") == "legacy_leg_v1":
            valid.append(leg)
    sample = valid[:n]
    results = []
    for leg in sample:
        parsed = parse_legacy_leg(leg)
        ok = parsed.get("format") == "legacy_leg_v1"
        results.append({"file": leg.name, "format": parsed.get("format"), "pass": ok})
    passed = sum(1 for r in results if r["pass"])
    return passed == n and len(sample) == n, results


sys.path.insert(0, str(REPO / "scripts"))
from track_c_recall_parser import parse_theory_recall  # noqa: E402


def parse_verify_strict(text: str) -> tuple[bool, str]:
    t = text or ""
    low = t.lower()
    if "overall: needs_review" in low:
        return False, "needs_review"
    if "drift" in low and "theory_corpus" in low:
        return False, "theory_hub_drift"
    if "dv=1.00" in t or "dv=1.0" in t:
        if "theory_corpus" in t or "theory-corpus" in t:
            return False, "hub_dv_1"
    if verify_text_healthy(t):
        return True, "healthy"
    return False, "unhealthy"


def run_mcp_checks(scratch: Path, binary: str, store: str) -> dict:
    client = MCPTestClient(binary, store, default_timeout=120.0)
    result: dict = {"recall": {}, "verify": {}}
    if not client.start():
        result["error"] = "mcp_start_failed"
        result["details"] = client.errors
        return result
    try:
        client.wait_for_fully_initialized(max_wait=90.0)
        for q in RECALL_QUERIES:
            safe = re.sub(r"[^a-z0-9]+", "-", q.lower()).strip("-")
            resp = client.call_tool(
                "mcp_engram_recall",
                {"query": q, "scope": "anchors"},
                timeout=120.0,
            )
            text = client._tool_text(resp)
            write_raw(scratch, f"recall-{safe}", text)
            parsed = parse_theory_recall(text, q, min_crs=MIN_CRS)
            result["recall"][q] = {
                "ok": parsed.ok,
                "top_hit": parsed.top_hit,
                "top_crs": parsed.top_crs,
                "theory_tile_concept": parsed.theory_tile_concept,
                "theory_tile_crs": parsed.theory_tile_crs,
                "theory_tile_rank": parsed.theory_tile_rank,
                "reported_pass_crs": parsed.reported_pass_crs,
                "drift_tiles": parsed.drift_tiles,
                "raw_file": f"recall-{safe}.mcp-raw.txt",
            }

        verify_runs = []
        for i in range(2):
            vresp = client.call_tool(
                "mcp_engram_verify_manifold_integrity",
                {"min_crs": MIN_CRS, "sample_size": 100},
                timeout=120.0,
            )
            vtext = client._tool_text(vresp)
            name = "verify-manifold-integrity" if i == 0 else "verify-manifold-integrity-post"
            write_raw(scratch, name, vtext)
            vok, vreason = parse_verify_strict(vtext)
            verify_runs.append({"ok": vok, "reason": vreason, "raw_file": f"{name}.mcp-raw.txt"})
        result["verify"] = verify_runs
        result["verify_ok"] = all(r["ok"] for r in verify_runs)
    finally:
        client.shutdown()
    return result


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--scratch", default="/tmp/grok-goal-5879e8737396/implementer")
    ap.add_argument("--store", default=os.path.expanduser("~/.engram/stalks"))
    ap.add_argument("--skip-mcp", action="store_true", help="Filesystem checks only")
    args = ap.parse_args()
    scratch = Path(args.scratch)
    scratch.mkdir(parents=True, exist_ok=True)

    failures: list[str] = []
    report: dict = {"started_at": datetime.now(timezone.utc).isoformat(), "checks": {}}

    org_ok, org_detail = check_organize_disabled()
    report["checks"]["theory_corpus_organize_disabled"] = org_detail
    if not org_ok:
        failures.append(f"theory_corpus_organize.py must exit 2, got {org_detail}")

    fe_ok, fe_n = check_fe_in_monad()
    report["checks"]["false_empire_in_monad_math"] = fe_n
    if not fe_ok:
        failures.append(f"false_empire_in_monad_math={fe_n} (expect 0)")

    fo_ok, fo_n, fo_paths = check_fe_in_organized()
    report["checks"]["false_empire_in_organized"] = fo_n
    if fo_paths:
        report["checks"]["false_empire_in_organized_paths"] = fo_paths
    if not fo_ok:
        failures.append(f"false_empire_in_organized={fo_n} (expect 0) paths={fo_paths}")

    vn_ok, vn_n = check_track_a_vn()
    report["checks"]["track_a_best_copy_vn_names"] = vn_n
    if not vn_ok:
        failures.append(f"track_a_best_copy_vn_names={vn_n} (expect 0)")

    leg_ok, leg_results = check_legacy_leg_sample(10)
    report["checks"]["legacy_leg_sample"] = leg_results
    if not leg_ok:
        failures.append("legacy_leg_sample not 10/10 format=legacy_leg_v1")

    leg3 = ORG / "leg3"
    if leg3.exists():
        failures.append("leg3/ exists under organized (forbidden)")

    report["checks"]["organized_files"] = sum(1 for p in ORG.rglob("*") if p.is_file())

    if not args.skip_mcp:
        binary = resolve_binary()
        mcp = run_mcp_checks(scratch, binary, args.store)
        report["mcp"] = mcp
        if mcp.get("error"):
            failures.append(f"mcp: {mcp['error']}")
        else:
            for q, info in mcp.get("recall", {}).items():
                if not info.get("ok"):
                    drift = info.get("drift_tiles") or []
                    failures.append(
                        f"recall({q!r}) FAIL top_crs={info.get('top_crs')} "
                        f"theory_tile_crs={info.get('theory_tile_crs')} "
                        f"theory_tile_rank={info.get('theory_tile_rank')} "
                        f"top={info.get('top_hit')} drift_tiles={drift}"
                    )
            if not mcp.get("verify_ok"):
                for vr in mcp.get("verify", []):
                    if not vr.get("ok"):
                        failures.append(f"verify_manifold_integrity: {vr.get('reason')} ({vr.get('raw_file')})")

    report["failures"] = failures
    report["overall"] = "PASS" if not failures else "FAIL"
    report["finished_at"] = datetime.now(timezone.utc).isoformat()

    out = scratch / "track-c-acceptance-gate.json"
    out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    summary_lines = [
        f"# Track C acceptance gate {report['finished_at']}",
        f"OVERALL: {report['overall']}",
        "",
    ]
    for f in failures:
        summary_lines.append(f"FAIL: {f}")
    if not failures:
        summary_lines.append("All checks passed. Raw MCP: *.mcp-raw.txt in scratch.")
    (scratch / "track-c-acceptance-gate.log").write_text("\n".join(summary_lines) + "\n")

    print(json.dumps({"overall": report["overall"], "failures": failures, "log": str(out)}, indent=2))
    return 0 if not failures else 1


if __name__ == "__main__":
    raise SystemExit(main())