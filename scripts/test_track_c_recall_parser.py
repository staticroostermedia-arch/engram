#!/usr/bin/env python3
"""Unit tests for track_c_recall_parser — fixtures from scratch evidence."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from track_c_recall_parser import MIN_CRS, parse_theory_recall  # noqa: E402

FIXTURES = Path(__file__).resolve().parent / "fixtures" / "recall"


def _load(name: str) -> str:
    return (FIXTURES / name).read_text(encoding="utf-8")


class TestTrackCRecallParser(unittest.TestCase):
    def test_lawful_cognition_fixture_fails_subthreshold_crs(self):
        raw = _load("recall-lawful-cognition.mcp-raw.txt")
        r = parse_theory_recall(raw, "lawful cognition")
        self.assertEqual(
            r.theory_tile_concept,
            "tile:formal_spec_theory-corpus-search---lawful-cognition-cluster",
        )
        self.assertEqual(r.theory_tile_rank, 1)
        self.assertAlmostEqual(r.theory_tile_crs, 0.723)
        self.assertAlmostEqual(r.top_crs, 0.723)
        self.assertFalse(r.ok)
        self.assertIsNone(r.reported_pass_crs)

    def test_adr_bootstrap_fixture_passes(self):
        raw = _load("recall-adr-bootstrap.mcp-raw.txt")
        r = parse_theory_recall(raw, "ADR bootstrap")
        self.assertEqual(
            r.theory_tile_concept,
            "tile:formal_spec_theory-corpus-search---adr-bootstrap-cluster",
        )
        self.assertEqual(r.theory_tile_rank, 1)
        self.assertAlmostEqual(r.theory_tile_crs, 0.88)
        self.assertTrue(r.ok)
        self.assertAlmostEqual(r.reported_pass_crs, 0.88)

    def test_legominism_scratch_fixture_fails_not_rank_one(self):
        raw = _load("recall-legominism.mcp-raw.txt")
        r = parse_theory_recall(raw, "legominism")
        self.assertEqual(
            r.theory_tile_concept,
            "tile:formal_spec_theory-corpus-search---legominism-cluster",
        )
        self.assertEqual(r.theory_tile_rank, 4)
        self.assertAlmostEqual(r.theory_tile_crs, 0.88)
        self.assertFalse(r.ok)

    def test_legominism_rank_one_passes(self):
        raw = _load("recall-legominism-rank1-pass.mcp-raw.txt")
        r = parse_theory_recall(raw, "legominism")
        self.assertEqual(r.theory_tile_rank, 1)
        self.assertGreaterEqual(r.theory_tile_crs, MIN_CRS)
        self.assertTrue(r.ok)

    def test_gate_json_fields_align_with_parser(self):
        raw = _load("recall-lawful-cognition.mcp-raw.txt")
        r = parse_theory_recall(raw, "lawful cognition")
        self.assertTrue(r.top_hit.startswith("tile:formal_spec_theory-corpus-search---lawful-cognition"))
        self.assertIsNone(r.reported_pass_crs)


if __name__ == "__main__":
    unittest.main()