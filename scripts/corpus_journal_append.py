#!/usr/bin/env python3
"""Append discernment journal entries (agent-authored JSON lines)."""
from __future__ import annotations

import json
import sys
from pathlib import Path

JOURNAL = Path("/home/a/Documents/Engram/data/theory-corpus/journal/discernment-journal.jsonl")


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: corpus_journal_append.py '<json_line>' OR @file.jsonl", file=sys.stderr)
        return 1
    arg = sys.argv[1]
    JOURNAL.parent.mkdir(parents=True, exist_ok=True)
    if arg.startswith("@"):
        lines = Path(arg[1:]).read_text(encoding="utf-8").splitlines()
    else:
        lines = [arg]
    with JOURNAL.open("a", encoding="utf-8") as f:
        for line in lines:
            line = line.strip()
            if not line:
                continue
            json.loads(line)  # validate
            f.write(line + "\n")
    print(f"appended {len([l for l in lines if l.strip()])} to {JOURNAL}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())