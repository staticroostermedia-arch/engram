#!/usr/bin/env python3
"""Full filesystem inventory for theory corpus — NO categorization."""
from __future__ import annotations

import hashlib
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

PHONE_ARCHIVE = Path("/home/a/Documents/CodeLand/data/phone_archive")
PHONE_STAGING = Path("/home/a/Documents/CodeLand/data/phone_staging")
JOURNAL = Path("/home/a/Documents/Engram/data/theory-corpus/journal/discernment-journal.jsonl")
OUT = Path("/tmp/grok-goal-5879e8737396/implementer/corpus-inventory.jsonl")


def md5_file(p: Path) -> str:
    h = hashlib.md5()
    with p.open("rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def journaled_paths() -> set[str]:
    if not JOURNAL.exists():
        return set()
    out = set()
    for line in JOURNAL.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        try:
            out.add(json.loads(line)["old_path"])
        except (json.JSONDecodeError, KeyError):
            pass
    return out


def main() -> int:
    done = journaled_paths()
    records = []
    for base in [PHONE_ARCHIVE, PHONE_STAGING]:
        if not base.exists():
            continue
        for p in sorted(base.rglob("*")):
            if not p.is_file():
                continue
            try:
                st = p.stat()
                records.append({
                    "path": str(p),
                    "base": base.name,
                    "parent_cat": p.parent.name if p.is_relative_to(PHONE_ARCHIVE) else "staging",
                    "name": p.name,
                    "ext": p.suffix.lower(),
                    "size": st.st_size,
                    "md5": md5_file(p),
                    "journaled": str(p) in done,
                })
            except OSError:
                continue
    OUT.parent.mkdir(parents=True, exist_ok=True)
    with OUT.open("w", encoding="utf-8") as f:
        for r in records:
            f.write(json.dumps(r) + "\n")
    total = len(records)
    journaled = sum(1 for r in records if r["journaled"])
    print(json.dumps({"total": total, "journaled": journaled, "remaining": total - journaled, "out": str(OUT)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())