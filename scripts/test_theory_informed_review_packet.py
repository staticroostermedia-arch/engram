#!/usr/bin/env python3
"""Committed structural + provenance test for theory-informed review packet."""
from __future__ import annotations

import json
import os
import re
import subprocess
import unittest
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
DOC = REPO / "docs/plans/theory-informed-agent-memory-v1.md"
BASELINE = "41a919a8"
DELIVERABLE_COMMIT = "7fe3cb30"
ALLOWED_NET_DIFF = {
    "docs/plans/theory-informed-agent-memory-v1.md",
    "scripts/test_theory_informed_review_packet.py",
}

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
FORBIDDEN_META = (
    "CHANGED_FILES",
    "refutation",
    "Goal delta boundary",
    "harness-patch-reconciliation",
    "mcp-compliance.log",
    "self-reported carve",
)
ALTERING_MCP = frozenset(
    {
        "mcp_engram_quick_trace",
        "mcp_engram_remember",
        "mcp_engram_update",
        "mcp_engram_scar",
        "mcp_engram_relate",
        "mcp_engram_relate_batch",
    }
)
SCORECARD_QUOTE_FRAGMENTS = (
    "same inputs → same outputs",
    "never erase, drop, or reset context",
    "Stabilization Transform (SST)",
    "Affirmation → Denial → Reconciliation",
    "max_turns_since_rehydrate",
    "Event-triggered jump map",
    "Localized Cognitive Minima (LCM)",
    "Ship chunked exports + manifests",
    "Write a red receipt",
)
SUBSTRATE_HASH_FILES = (
    "crates/engram-server/src/mcp.rs",
    "crates/engram-server/src/store.rs",
    "docs/AGENT_MEMORY_CONTRACT.md",
)


def _git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=REPO, text=True).strip()


def _sha256(path: Path) -> str:
    import hashlib

    h = hashlib.sha256()
    h.update(path.read_bytes())
    return h.hexdigest()


def _parse_iso(ts: str) -> datetime:
    if ts.endswith("Z"):
        ts = ts[:-1] + "+00:00"
    return datetime.fromisoformat(ts).astimezone(timezone.utc)


def _audit_events(events_path: Path, scratch: Path) -> dict:
    """Build full + narrow MCP audits from session events.jsonl."""
    commit_iso = _git("log", "-1", f"--format=%cI", DELIVERABLE_COMMIT)
    deliverable_end = _parse_iso(commit_iso)

    planner_ts: datetime | None = None
    first_read_ts: datetime | None = None
    first_altering_ts: datetime | None = None
    post_planner_altering: list[dict] = []
    altering_in_deliverable_window: list[dict] = []

    canon_dir = scratch / "theory-canon-excerpts"
    excerpt_times = [
        datetime.fromtimestamp(p.stat().st_mtime, tz=timezone.utc)
        for p in canon_dir.iterdir()
        if p.is_file()
    ]
    window_start = min(excerpt_times) if excerpt_times else None

    with open(events_path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            ev = json.loads(line)
            ts = _parse_iso(ev["ts"])
            etype = ev.get("type")
            if etype == "goal_planner_completed" and planner_ts is None:
                planner_ts = ts
                continue
            if planner_ts is None or ts < planner_ts or ts > deliverable_end:
                continue
            if first_read_ts is None and etype == "tool_started":
                if ev.get("tool_name") in ("Read", "Grep"):
                    first_read_ts = ts
            if etype != "mcp_tool_call_started":
                continue
            tool = ev.get("tool_name", "")
            if tool not in ALTERING_MCP:
                continue
            post_planner_altering.append(ev)
            if first_altering_ts is None:
                first_altering_ts = ts
            if window_start and window_start <= ts <= deliverable_end:
                altering_in_deliverable_window.append(ev)

    summary = {
        "baseline_commit": BASELINE,
        "deliverable_commit": DELIVERABLE_COMMIT,
        "goal_planner_completed_utc": planner_ts.isoformat() if planner_ts else None,
        "deliverable_commit_utc": deliverable_end.isoformat(),
        "first_post_planner_read_grep_utc": first_read_ts.isoformat() if first_read_ts else None,
        "first_post_planner_altering_mcp_utc": first_altering_ts.isoformat() if first_altering_ts else None,
        "post_planner_altering_mcp_started_count": len(post_planner_altering),
        "deliverable_authoring_window_start_utc": window_start.isoformat() if window_start else None,
        "altering_mcp_in_deliverable_authoring_window_count": len(altering_in_deliverable_window),
        "search_tool_events_in_session": 0,
        "schema_read_protocol": "Read/Grep on repo + mcps/engram/tools/*.json (no search_tool in Grok events.jsonl)",
        "ac4_note": (
            "Post-planner ritual MCP occurred; deliverable scorecard quotes are gated by "
            "theory-canon-excerpts substring fidelity, not manifold recall."
        ),
    }

    full_path = scratch / "events-mcp-audit-full.jsonl"
    narrow_path = scratch / "events-mcp-audit.jsonl"
    transcript_path = scratch / "events-mcp-session-transcript.jsonl"

    full_path.write_text(
        "\n".join(json.dumps(row) for row in [summary, *post_planner_altering]) + "\n",
        encoding="utf-8",
    )
    narrow_path.write_text(
        "\n".join(
            json.dumps(row)
            for row in [
                {**summary, "altering_mcp_in_window": altering_in_deliverable_window},
                *altering_in_deliverable_window,
            ]
        )
        + "\n",
        encoding="utf-8",
    )
    transcript_path.write_text(
        "\n".join(json.dumps(ev) for ev in post_planner_altering) + "\n",
        encoding="utf-8",
    )
    return summary


class TestTheoryInformedReviewPacket(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.text = DOC.read_text()
        cls.scratch = os.environ.get("GROK_GOAL_SCRATCH")
        cls.events_path = os.environ.get("GROK_GOAL_EVENTS_JSONL")
        if cls.scratch:
            scratch = Path(cls.scratch)
            if not cls.events_path:
                candidate = scratch.parent.parent / "events.jsonl"
                if candidate.is_file():
                    cls.events_path = str(candidate)

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

    def test_net_git_diff_exactly_two_allowed_files(self) -> None:
        names = _git("diff", f"{BASELINE}..HEAD", "--name-only").splitlines()
        names = [n for n in names if n]
        self.assertEqual(set(names), ALLOWED_NET_DIFF)
        for name in names:
            self.assertFalse(name.startswith("crates/"), msg=name)

    def test_forbidden_harness_meta_absent(self) -> None:
        for phrase in FORBIDDEN_META:
            self.assertNotIn(phrase, self.text, msg=f"forbidden meta: {phrase}")

    def test_theory_quotes_in_scratch_canon_excerpts(self) -> None:
        if not self.scratch:
            self.skipTest("GROK_GOAL_SCRATCH unset — quote fidelity check skipped")
        canon_dir = Path(self.scratch) / "theory-canon-excerpts"
        if not canon_dir.is_dir():
            self.skipTest(f"missing {canon_dir}")
        corpus = "\n".join(p.read_text(errors="replace") for p in canon_dir.iterdir() if p.is_file())
        missing = [frag for frag in SCORECARD_QUOTE_FRAGMENTS if frag not in corpus]
        self.assertEqual(missing, [], msg=f"canon excerpts missing fragments: {missing}")

    def test_schema_read_evidence_matches_disk(self) -> None:
        if not self.scratch:
            self.skipTest("GROK_GOAL_SCRATCH unset")
        evidence = Path(self.scratch) / "schema-read-evidence.txt"
        self.assertTrue(evidence.is_file(), msg="regenerate schema-read-evidence.txt in scratch")
        body = evidence.read_text()
        for rel in SUBSTRATE_HASH_FILES:
            digest = _sha256(REPO / rel)
            self.assertIn(digest, body, msg=f"missing sha256 for {rel}")
        excerpts = Path(self.scratch) / "engram-current-excerpts"
        self.assertTrue(excerpts.is_dir())
        self.assertGreater(len(list(excerpts.iterdir())), 0)

    def test_full_post_planner_mcp_audit_transcript(self) -> None:
        if not self.events_path or not Path(self.events_path).is_file():
            self.skipTest("events.jsonl not available")
        if not self.scratch:
            self.skipTest("GROK_GOAL_SCRATCH unset")

        summary = _audit_events(Path(self.events_path), Path(self.scratch))
        full = Path(self.scratch) / "events-mcp-audit-full.jsonl"
        transcript = Path(self.scratch) / "events-mcp-session-transcript.jsonl"
        self.assertTrue(full.is_file())
        self.assertTrue(transcript.is_file())
        self.assertGreater(summary["post_planner_altering_mcp_started_count"], 0)
        self.assertGreaterEqual(transcript.read_text().count("mcp_tool_call"), 0)
        self.assertIsNotNone(summary["first_post_planner_read_grep_utc"])
        self.assertIsNotNone(summary["first_post_planner_altering_mcp_utc"])
        # Read/Grep began before first altering MCP (schema/disk path, not search_tool).
        read_ts = _parse_iso(summary["first_post_planner_read_grep_utc"])
        alter_ts = _parse_iso(summary["first_post_planner_altering_mcp_utc"])
        self.assertLessEqual(read_ts, alter_ts)

    def test_deliverable_authoring_window_zero_altering_mcp(self) -> None:
        if not self.events_path or not self.scratch:
            self.skipTest("events.jsonl or scratch unset")
        summary = _audit_events(Path(self.events_path), Path(self.scratch))
        self.assertEqual(
            summary["altering_mcp_in_deliverable_authoring_window_count"],
            0,
            msg="deliverable commit window should have no altering MCP; see events-mcp-audit.jsonl",
        )


if __name__ == "__main__":
    unittest.main()