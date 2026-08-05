#!/usr/bin/env python3
"""LoRA loop dry-run: pack → mock metrics → decision receipt (no weight train)."""
from __future__ import annotations

import argparse
import hashlib
import json
import time
from pathlib import Path


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--pack", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    args = ap.parse_args()
    manifest_path = args.pack / "manifest.json"
    if not manifest_path.exists():
        raise SystemExit(f"missing pack manifest: {manifest_path}")
    manifest = json.loads(manifest_path.read_text())
    pack_hash = manifest.get("pack_hash", "unknown")
    # Dry-run metrics: structural checks only
    ok = manifest.get("schema_version") == "experience_pack_v1"
    receipt = {
        "schema": "lora_improvement_receipt_v1",
        "pack_hash": pack_hash,
        "adapter_id": "dry_run",
        "before": {"csf_median": 0.94, "harness_pass": True},
        "after": {"csf_median": 0.94, "harness_pass": ok},
        "decision": "scar" if not ok else "hold_for_human",
        "reason": "dry_run only — no weight update; pipeline structural pass"
        if ok
        else "pack schema invalid",
        "created_at": int(time.time()),
        "eval_harness": [
            "agent-memory",
            "format/encode",
            "protocol_live",
            "held_out_goal_replay",
        ],
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    main()
