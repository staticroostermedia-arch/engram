#!/usr/bin/env python3
"""Committed structural test for theory-informed review packet deliverable."""
from __future__ import annotations

import re
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
DOC = REPO / "docs/plans/theory-informed-agent-memory-v1.md"

REQUIRED_OBLIGATIONS = (
    "Deterministic rehydration",
    "No silent forgetting",
    "SST",
    "A/D/R",
    "Sentinel",
    "Shock",
    "Anchor-first recall",
    "Portable",
)
TAGS = ("shipped", "partial", "gap", "reference_only")
EXTENSION_POINTS = (
    "mcp.rs",
    "store.rs",
    "wake_bundle.rs",
    "edit_fidelity.rs",
    "turn_extract.rs",
)


class TestTheoryInformedReviewPacket(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.text = DOC.read_text()

    def test_deliverable_exists(self) -> None:
        self.assertTrue(DOC.is_file())

    def test_goal_headline_and_review_gate(self) -> None:
        self.assertIn("theory_informed_agent_memory_v1", self.text)
        self.assertIn("for review before executing it as a goal as well", self.text)

    def test_scorecard_row_count(self) -> None:
        rows = re.findall(r"^\| \d+ \|", self.text, re.MULTILINE)
        self.assertGreaterEqual(len(rows), 8)

    def test_all_status_tags_present(self) -> None:
        for tag in TAGS:
            self.assertIn(tag, self.text)

    def test_obligation_keywords(self) -> None:
        lower = self.text.lower()
        for ob in REQUIRED_OBLIGATIONS:
            self.assertIn(ob.lower(), lower)

    def test_five_spikes(self) -> None:
        spikes = re.findall(r"^### Spike \d+", self.text, re.MULTILINE)
        self.assertEqual(len(spikes), 5)

    def test_spike_fields(self) -> None:
        for field in ("Falsifier", "Ritual template", "Non-goals"):
            self.assertIn(field, self.text)

    def test_extension_points_named(self) -> None:
        for ep in EXTENSION_POINTS:
            self.assertIn(ep, self.text)

    def test_substrate_files_exist(self) -> None:
        for ep in EXTENSION_POINTS:
            path = REPO / "crates/engram-server/src" / ep
            self.assertTrue(path.is_file(), msg=str(path))


if __name__ == "__main__":
    unittest.main()