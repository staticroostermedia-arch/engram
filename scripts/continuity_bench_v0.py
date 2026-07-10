#!/usr/bin/env python3
"""Continuity bench v0 — offline evidence gates (no chat packs, no PEFT).

Checks process-contract artifacts that dual RSI / glassbox rely on:
  - dual_loop_state + fire_verify JSON schemas validate
  - glassbox fixture well-formed
  - PEFT eval_gate metrics present (if path exists)
  - SFT export non-empty row count
  - optional: parents list non-empty in fixture dual_loop

Writes: data/lora-export/continuity_bench_v0.json
Exit 0 iff all required checks pass.

Usage (repo root):
  python3 scripts/continuity_bench_v0.py
"""
from __future__ import annotations

import json
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "data" / "lora-export" / "continuity_bench_v0.json"


def load_json(p: Path):
    with p.open() as f:
        return json.load(f)


def main() -> int:
    checks = []
    required_fail = 0

    def add(name: str, ok: bool, detail: str, required: bool = True):
        nonlocal required_fail
        checks.append({"name": name, "ok": ok, "detail": detail, "required": required})
        if required and not ok:
            required_fail += 1

    # 1) glassbox schema unit tests (import validators if present)
    try:
        sys.path.insert(0, str(ROOT / "scripts"))
        # Prefer existing test module
        import test_glassbox_schemas as tgs  # type: ignore

        # Run the four unittest cases via loader
        import unittest

        suite = unittest.defaultTestLoader.loadTestsFromModule(tgs)
        result = unittest.TextTestRunner(verbosity=0, stream=open("/dev/null", "w")).run(suite)
        add(
            "glassbox_schemas_unittest",
            result.wasSuccessful(),
            f"tests={result.testsRun} fail={len(result.failures)} err={len(result.errors)}",
        )
    except Exception as e:
        add("glassbox_schemas_unittest", False, f"{type(e).__name__}: {e}")

    # 2) schema files exist + parse
    for rel in (
        "docs/schemas/dual_loop_state_v1.json",
        "docs/schemas/fire_verify_packet_v1.json",
    ):
        p = ROOT / rel
        try:
            d = load_json(p)
            add(f"schema_parse:{p.name}", isinstance(d, dict) and "properties" in d, f"keys={list(d)[:6]}")
        except Exception as e:
            add(f"schema_parse:{p.name}", False, str(e))

    # 3) glassbox fixture
    fix = ROOT / "tools" / "leg-browser" / "fixtures" / "glassbox-sample.json"
    try:
        d = load_json(fix)
        parents = d.get("parents") or []
        dl = d.get("dual_loop") or {}
        ok = (
            isinstance(parents, list)
            and len(parents) >= 3
            and dl.get("version") == 1
            and "track_next" in dl
            and "parents" in dl
        )
        add("glassbox_fixture", ok, f"parents={len(parents)} track_next={dl.get('track_next')}")
    except Exception as e:
        add("glassbox_fixture", False, str(e))

    # 4) eval_gate metrics (optional if missing — warn only)
    em = ROOT / "data" / "lora-export" / "eval_gate_metrics.json"
    if em.exists():
        try:
            d = load_json(em)
            ok = bool(d.get("passed") or d.get("status") == "ok")
            add(
                "eval_gate_metrics",
                ok,
                f"passed={d.get('passed')} wins={d.get('wins_adapter_ge_base')}",
                required=True,
            )
        except Exception as e:
            add("eval_gate_metrics", False, str(e))
    else:
        add("eval_gate_metrics", False, "missing", required=False)

    # 5) SFT rows
    sft = ROOT / "data" / "lora-export" / "leg_geometry_sft.jsonl"
    if sft.exists():
        n = sum(1 for _ in sft.open())
        add("sft_rows", n >= 1, f"rows={n}", required=True)
    else:
        add("sft_rows", False, "missing", required=True)

    # 6) adapter dir pointer
    adapter = ROOT / "data" / "lora-export" / "adapters" / "leg_geometry_lora_v1"
    add("adapter_on_disk", adapter.is_dir(), str(adapter.relative_to(ROOT)), required=False)

    passed = required_fail == 0
    report = {
        "version": 1,
        "bench": "continuity_bench_v0",
        "status": "pass" if passed else "fail",
        "passed": passed,
        "at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "required_failures": required_fail,
        "checks": checks,
        "note": "Offline process-contract + PEFT path evidence; not full MCP kill/rehydrate (see continuity-demo.sh for that).",
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"status": report["status"], "passed": passed, "out": str(OUT), "n_checks": len(checks)}))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
