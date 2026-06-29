#!/usr/bin/env python3
"""Pure recall parser for Track C acceptance gate — no MCP, no mutations."""
from __future__ import annotations

import re
from dataclasses import asdict, dataclass
from typing import Any

MIN_CRS = 0.74

QUERY_TILE_SUFFIX: dict[str, str] = {
    "legominism": "legominism-cluster",
    "lawful cognition": "lawful-cognition-cluster",
    "ADR bootstrap": "adr-bootstrap-cluster",
}

HIT_BLOCK_RE = re.compile(
    r"\*\*\[(\d+)\]\s+(\S+)\*\*.*?\(.*?score:\s*([\d.]+).*?crs:\s*([\d.]+).*?dv:\s*([\d.]+)",
    re.S | re.I,
)

THEORY_TILE_RE = re.compile(r"tile:formal_spec_theory-corpus-search---[\w\-]+")


@dataclass
class RecallHit:
    rank: int
    concept: str
    score: float
    crs: float
    dv: float


@dataclass
class TheoryRecallResult:
    top_hit: str
    top_crs: float
    theory_tile_concept: str | None
    theory_tile_crs: float | None
    theory_tile_rank: int | None
    drift_tiles: list[str]
    ok: bool
    reported_pass_crs: float | None

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


def parse_hits(raw_text: str) -> list[RecallHit]:
    if not raw_text or "no memories found" in raw_text.lower():
        return []
    hits: list[RecallHit] = []
    for m in HIT_BLOCK_RE.finditer(raw_text):
        hits.append(
            RecallHit(
                rank=int(m.group(1)),
                concept=m.group(2),
                score=float(m.group(3)),
                crs=float(m.group(4)),
                dv=float(m.group(5)),
            )
        )
    return hits


def theory_tile_drift(raw_text: str) -> list[str]:
    bad: list[str] = []
    for m in HIT_BLOCK_RE.finditer(raw_text or ""):
        concept, dv = m.group(2), float(m.group(5))
        if THEORY_TILE_RE.fullmatch(concept) and dv >= 0.99:
            bad.append(concept)
    return bad


def expected_tile_concept(query: str) -> str:
    suffix = QUERY_TILE_SUFFIX[query]
    return f"tile:formal_spec_theory-corpus-search---{suffix}"


def parse_theory_recall(
    raw_text: str,
    query: str,
    *,
    min_crs: float = MIN_CRS,
) -> TheoryRecallResult:
    """Strict pass: query's theory search tile must be rank-1 with crs >= min_crs."""
    drift_tiles = theory_tile_drift(raw_text)
    hits = parse_hits(raw_text)

    top_hit = hits[0].concept if hits else "unknown"
    top_crs = hits[0].crs if hits else 0.0

    want = expected_tile_concept(query)
    theory_hit = next((h for h in hits if h.concept == want), None)

    if theory_hit is None:
        return TheoryRecallResult(
            top_hit=top_hit,
            top_crs=top_crs,
            theory_tile_concept=None,
            theory_tile_crs=None,
            theory_tile_rank=None,
            drift_tiles=drift_tiles,
            ok=False,
            reported_pass_crs=None,
        )

    ok = (
        theory_hit.rank == 1
        and theory_hit.crs >= min_crs
        and want not in drift_tiles
    )
    return TheoryRecallResult(
        top_hit=top_hit,
        top_crs=top_crs,
        theory_tile_concept=theory_hit.concept,
        theory_tile_crs=theory_hit.crs,
        theory_tile_rank=theory_hit.rank,
        drift_tiles=drift_tiles,
        ok=ok,
        reported_pass_crs=theory_hit.crs if ok else None,
    )