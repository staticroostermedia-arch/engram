#!/usr/bin/env python3
"""experience_pack_v1 export — curated concepts only (no whole-stalk dump).

Usage:
  python3 scripts/export_experience_pack_v1.py --out /path/to/pack \\
    --concepts goal:engram_local_primary_critical_path_v1,helper:session_handoff_latest

Does not read ~/.engram binary .leg directly (geometry stays in Engram);
this writer materializes a pack from stdin JSON or --concepts placeholders
for harness dry-run. Live export should prefer MCP scrub_export / read_concept.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import time
from pathlib import Path


SCHEMA = "experience_pack_v1"
MIN_CRS = 0.74


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--concepts", default="", help="comma-separated concept ids")
    ap.add_argument("--min-crs", type=float, default=MIN_CRS)
    args = ap.parse_args()
    out: Path = args.out
    out.mkdir(parents=True, exist_ok=True)
    (out / "trajectories").mkdir(exist_ok=True)
    (out / "preferences").mkdir(exist_ok=True)
    (out / "negatives").mkdir(exist_ok=True)
    (out / "harness").mkdir(exist_ok=True)
    (out / "reference").mkdir(exist_ok=True)

    concepts = [c.strip() for c in args.concepts.split(",") if c.strip()]
    entries = [
        {
            "concept": c,
            "role": "reference" if c.startswith(("goal:", "helper:", "tile:")) else "trajectory",
            "min_crs": args.min_crs,
            "note": "placeholder — fill via MCP read_concept/scrub_export for live packs",
        }
        for c in concepts
    ]
    body = json.dumps(entries, sort_keys=True, indent=2)
    pack_hash = hashlib.blake2b(body.encode(), digest_size=16).hexdigest()
    manifest = {
        "schema_version": SCHEMA,
        "pack_hash": pack_hash,
        "created_at": int(time.time()),
        "filters": {
            "min_crs": args.min_crs,
            "unfiltered_forbidden": True,
            "held_out_required": True,
        },
        "concept_count": len(entries),
        "doctrine": "Unfiltered self-train forbidden; receipt required for positives",
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    (out / "reference" / "concepts.json").write_text(body + "\n")
    print(json.dumps({"ok": True, "out": str(out), "pack_hash": pack_hash}, indent=2))


if __name__ == "__main__":
    main()
