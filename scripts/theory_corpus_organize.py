#!/usr/bin/env python3
"""
DISABLED — keyword-based corpus organizer.

This script was abandoned per DISCERNMENT_WORKFLOW.md. Category assignment
must be agent discernment only (journal + two-pass reads).

Use instead:
  scripts/corpus_inventory.py      — inventory + md5 only
  scripts/legacy_leg_parse.py      — legacy .leg header parse (not leg3)
  scripts/corpus_journal_append.py — append discernment journal lines
  data/theory-corpus/DISCERNMENT_WORKFLOW.md

Invalid prior output quarantine:
  data/theory-legacy-KEYWORD-DRAFT-INVALID/
"""
from __future__ import annotations

import sys


def main() -> int:
    print(
        "theory_corpus_organize.py is DISABLED (keyword classification rejected).\n"
        "See data/theory-corpus/DISCERNMENT_WORKFLOW.md and scripts/corpus_inventory.py.",
        file=sys.stderr,
    )
    return 2


if __name__ == "__main__":
    raise SystemExit(main())