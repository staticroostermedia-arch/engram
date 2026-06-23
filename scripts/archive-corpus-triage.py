#!/usr/bin/env python3
"""
Archive bulk corpus shards (_txt_part*) from active Engram stalks to corpus_archive/.

Moves only — never deletes. Writes manifest.jsonl for rollback.

Usage:
  ./scripts/archive-corpus-triage.py --dry-run
  ./scripts/archive-corpus-triage.py --execute
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path

PRIMARY = Path.home() / ".engram" / "stalks"
DEFAULT_STALK = PRIMARY / "default"
ARCHIVE_ROOT = Path.home() / ".engram" / "corpus_archive" / "triage-2026-06-22"


def is_leg_file(path: Path) -> bool:
    return path.suffix in (".leg", ".leg3")


def should_archive(name: str) -> bool:
    """Bulk book/corpus shards only — never spatial AST or ritual prefixes."""
    stem = name
    if stem.endswith(".leg"):
        stem = stem[:-4]
    elif stem.endswith(".leg3"):
        stem = stem[:-5]
    if "_txt_part" not in stem:
        return False
    return True


def collect(src_dir: Path) -> list[Path]:
    out: list[Path] = []
    if not src_dir.is_dir():
        return out
    for entry in os.scandir(src_dir):
        if not entry.is_file():
            continue
        p = Path(entry.path)
        if not is_leg_file(p):
            continue
        if should_archive(p.name):
            out.append(p)
    return sorted(out)


def main() -> int:
    parser = argparse.ArgumentParser(description="Archive corpus _txt_part blocks")
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--dry-run", action="store_true")
    group.add_argument("--execute", action="store_true")
    args = parser.parse_args()

    batches = [
        ("primary", PRIMARY, ARCHIVE_ROOT / "primary"),
        ("default", DEFAULT_STALK, ARCHIVE_ROOT / "default"),
    ]

    manifest_path = ARCHIVE_ROOT / "manifest.jsonl"
    total = 0
    total_bytes = 0

    for label, src, dest in batches:
        files = collect(src)
        print(f"[{label}] archive candidates: {len(files)} from {src}")
        for p in files:
            total += 1
            try:
                total_bytes += p.stat().st_size
            except OSError:
                pass
            rel = p.name
            target = dest / rel
            record = {
                "ts": datetime.now(timezone.utc).isoformat(),
                "stalk": label,
                "src": str(p),
                "dest": str(target),
                "bytes": 262144,
            }
            if args.dry_run:
                if total <= 5 or total % 20000 == 0:
                    print(f"  would move: {p} -> {target}")
            else:
                dest.mkdir(parents=True, exist_ok=True)
                if target.exists():
                    print(f"  skip exists: {target}", file=sys.stderr)
                    continue
                shutil.move(str(p), str(target))
                with open(manifest_path, "a", encoding="utf-8") as mf:
                    mf.write(json.dumps(record) + "\n")

    print(f"\nTotal: {total} files (~{total_bytes / (1024**3):.2f} GiB)")
    if args.dry_run:
        print("Dry run complete — re-run with --execute to move.")
    else:
        print(f"Manifest: {manifest_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())