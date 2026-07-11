#!/usr/bin/env python3
"""Validate dual_loop / fire verify packets (stdlib only)."""
from __future__ import annotations

from typing import Any

VERIFY_TYPES = {
    "substrate_local",
    "gemma_stage",
    "meta_policy",
    "ship_local",
    "ship_skip",
    "ci_status",
    "binary_vs_proc",
    "metrics_atom",
}
VERIFY_STATUS = {"pending", "pass", "fail"}
TRACKS = {"S", "G", "M", None}


def validate_dual_loop(doc: dict[str, Any]) -> list[str]:
    errs: list[str] = []
    if not isinstance(doc, dict):
        return ["root must be object"]
    if doc.get("version") != 1:
        errs.append("version must be 1")
    if doc.get("track_next") not in ("S", "G", "M"):
        errs.append("track_next must be S|G|M")
    if "mcp_restart_required" in doc and not isinstance(doc["mcp_restart_required"], bool):
        errs.append("mcp_restart_required must be bool")
    if "parents" in doc and not isinstance(doc["parents"], list):
        errs.append("parents must be array")
    lv = doc.get("last_verify")
    if lv is not None:
        if not isinstance(lv, dict):
            errs.append("last_verify must be object")
        else:
            if lv.get("status") not in VERIFY_STATUS:
                errs.append("last_verify.status invalid")
            if "type" not in lv:
                errs.append("last_verify.type required")
    return errs


def validate_verify_packet(doc: dict[str, Any]) -> list[str]:
    errs: list[str] = []
    for k in (
        "parent",
        "loop",
        "intent",
        "verify_type",
        "verify_status",
        "verify_evidence",
        "falsify",
    ):
        if k not in doc:
            errs.append(f"missing {k}")
    if doc.get("verify_type") not in VERIFY_TYPES:
        errs.append("verify_type invalid")
    if doc.get("verify_status") not in VERIFY_STATUS:
        errs.append("verify_status invalid")
    if "track" in doc and doc["track"] not in TRACKS:
        errs.append("track must be S|G|M|null")
    return errs


if __name__ == "__main__":
    import json
    import sys
    from pathlib import Path

    path = Path(sys.argv[1]) if len(sys.argv) > 1 else None
    if not path:
        print("usage: validate_dual_loop_schema.py <jsonfile> [dual_loop|verify]")
        sys.exit(2)
    doc = json.loads(path.read_text())
    mode = sys.argv[2] if len(sys.argv) > 2 else "dual_loop"
    errs = validate_dual_loop(doc) if mode == "dual_loop" else validate_verify_packet(doc)
    if errs:
        print("FAIL", errs)
        sys.exit(1)
    print("OK")
