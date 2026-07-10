#!/usr/bin/env python3
"""Unit tests for Glass-Box RSI schemas."""
from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from validate_dual_loop_schema import validate_dual_loop, validate_verify_packet  # noqa: E402


class TestDualLoopSchema(unittest.TestCase):
    def test_minimal_valid(self):
        doc = {
            "version": 1,
            "track_next": "G",
            "track_last": "S",
            "open_pr": None,
            "mcp_restart_required": False,
            "last_fire_goal": "goal:fire_dual_rsi_test_1",
            "last_verify": {
                "type": "substrate_local",
                "status": "pass",
                "at": "2026-07-10T00:00:00Z",
            },
            "parents": ["goal:dual_rsi_program"],
            "gemma": {"stage": "eval_gate", "sft_rows": 51},
        }
        errs = validate_dual_loop(doc)
        self.assertEqual(errs, [])

    def test_missing_track_next_fails(self):
        errs = validate_dual_loop({"version": 1})
        self.assertTrue(any("track_next" in e for e in errs))

    def test_verify_packet_pass(self):
        pkt = {
            "parent": "goal:dual_rsi_program",
            "loop": "dual_rsi",
            "track": "S",
            "intent": "grow corpus",
            "verify_type": "substrate_local",
            "verify_status": "pass",
            "verify_evidence": "data/lora-export/leg_geometry_sft.jsonl rows=51",
            "falsify": "disk export missing",
        }
        self.assertEqual(validate_verify_packet(pkt), [])

    def test_verify_status_invalid(self):
        pkt = {
            "parent": "goal:x",
            "loop": "ship_gate",
            "track": None,
            "intent": "ship",
            "verify_type": "ship_local",
            "verify_status": "maybe",
            "verify_evidence": "n/a",
            "falsify": "n/a",
        }
        errs = validate_verify_packet(pkt)
        self.assertTrue(any("verify_status" in e for e in errs))


if __name__ == "__main__":
    unittest.main()
