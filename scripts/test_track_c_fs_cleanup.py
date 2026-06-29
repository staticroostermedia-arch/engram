#!/usr/bin/env python3
"""Unit tests for track_c_fs_cleanup False Empire detection."""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from track_c_fs_cleanup import is_false_empire_name  # noqa: E402


class TestFalseEmpireDetection(unittest.TestCase):
    def test_detects_false_empire_prefix(self):
        self.assertTrue(is_false_empire_name("False_Empire_Glossary_v1.md"))
        self.assertTrue(is_false_empire_name("False_Empire_Citation_Protocol_and_Style_Guide_v1.md"))
        self.assertTrue(is_false_empire_name("false_empire_index.md"))

    def test_rejects_non_fe(self):
        self.assertFalse(is_false_empire_name("Lawful_Cognitive_Stack_Plan.md"))
        self.assertFalse(is_false_empire_name("ADR-H1-HB-identity.leg"))


if __name__ == "__main__":
    unittest.main()