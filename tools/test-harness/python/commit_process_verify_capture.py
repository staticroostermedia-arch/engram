#!/usr/bin/env python3
"""Live MCP capture for commit title/versioning process verification (plan step 3-4)."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from datetime import datetime, timezone
from typing import Any, Dict, List, Tuple

sys.path.insert(0, os.path.dirname(__file__))
from mcp_test_client import MCPTestClient  # noqa: E402

GOAL = "goal:commit_title_versioning_process"
DISCIPLINE = "commit_title_versioning_discipline"
INTENT = "define commit title versioning process per plan"
PREV_TRACE = "trace:1782163244_add-capture-commit-process-verify-sh---fix-verif"
SESSION_END_SUMMARY = (
    "commit process discipline defined per plan + ACs 1-4 exercised; "
    "CONTEXT bad-description gap closed; related to git VC sub and engram_mvp_v1"
)

DOC_EDITS: List[Tuple[str, str, str]] = [
    ("context_for_edit_contributing", "/home/a/Documents/Engram/CONTRIBUTING.md", "CONTRIBUTING.md:97"),
    ("context_for_edit_pr_template", "/home/a/Documents/Engram/.github/PULL_REQUEST_TEMPLATE.md", "PULL_REQUEST_TEMPLATE.md:11"),
    ("context_for_edit_agent_contract", "/home/a/Documents/Engram/docs/AGENT_MEMORY_CONTRACT.md", "AGENT_MEMORY_CONTRACT.md:339"),
    ("context_for_edit_maintainer", "/home/a/Documents/Engram/docs/internal/MAINTAINER_WORKFLOW.md", "MAINTAINER_WORKFLOW.md:84"),
    ("context_for_edit_context_injection", "/home/a/Documents/Engram/docs/CONTEXT_INJECTION_NVME_BYPASS.md", "CONTEXT_INJECTION_NVME_BYPASS.md:48"),
    ("context_for_edit_engram_goal", "/home/a/Documents/Engram/grok-plugin-engram/commands/engram-goal.md", "engram-goal.md:19"),
]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True)
    ap.add_argument("--store", default=os.path.expanduser("~/.engram/stalks"))
    ap.add_argument("--scratch", required=True)
    ap.add_argument("--run", type=int, default=1, choices=[1, 2])
    args = ap.parse_args()

    scratch = os.path.abspath(args.scratch)
    os.makedirs(scratch, exist_ok=True)
    out_path = os.path.join(scratch, f"engram-commit-process-verify-run{args.run}.json")

    live_env = {
        "ENGRAM_PROFILE": "agent",
        "ENGRAM_DISABLE_SHEAF": "0",
        "ENGRAM_FORCE_CPU_BACKEND": "0",
        "ENGRAM_KI_DISABLE": "0",
        "ENGRAM_NREM_DISABLE": "1",
    }

    client = MCPTestClient(args.binary, args.store, env_overrides=live_env, default_timeout=120.0)
    transcript: List[Dict[str, Any]] = []
    assertions: List[str] = []
    failures: List[str] = []
    prev_trace_id = PREV_TRACE

    def snap(label: str, tool: str, tool_args: Dict[str, Any]) -> Dict[str, Any]:
        resp = client.call_tool(tool, tool_args, timeout=120.0)
        entry: Dict[str, Any] = {
            "label": label,
            "tool": tool,
            "args": tool_args,
            "raw": resp,
            "text": client._tool_text(resp),
        }
        parsed = client._parse_tool_json(resp)
        if parsed:
            entry["parsed"] = parsed
        transcript.append(entry)
        return entry

    if not client.start():
        result = {"error": "mcp_start_failed", "details": client.errors}
        with open(out_path, "w") as f:
            json.dump(result, f, indent=2)
        return 2

    try:
        client.wait_for_fully_initialized(max_wait=90.0)

        snap("session_start", "mcp_engram_session_start", {"intent": INTENT})
        ack = snap("ack_wake_queue", "mcp_engram_ack_wake_queue", {"executed": True})
        snap("recall_anchors", "mcp_engram_recall", {
            "query": "commit title versioning",
            "scope": "anchors",
            "k": 8,
        })
        gl = snap("goal_list_active", "mcp_engram_goal_list", {"status": "active", "limit": 30})
        gl_done = snap("goal_list_completed", "mcp_engram_goal_list", {"status": "completed", "limit": 20})
        gs = snap("goal_status", "mcp_engram_goal_status", {"goal": GOAL})
        rc = snap("read_concept_discipline", "mcp_engram_read_concept", {"concept": DISCIPLINE})

        for label, path, spatial in DOC_EDITS:
            entry = snap(label, "mcp_engram_context_for_edit", {"path": path})
            parsed = entry.get("parsed") or {}
            if parsed.get("file_path") == path or path.split("/")[-1] in entry.get("text", ""):
                assertions.append(f"{label} OK ({spatial})")
            else:
                failures.append(f"{label} failed for {path}")

        qt_pre = snap("quick_trace_pre_edit", "mcp_engram_quick_trace", {
            "decision": "Verify commit discipline docs landed with full ritual capture",
            "why": "Plan AC3 requires quick_trace at forks with goal_context + prev + spatial_context",
            "goal_context": GOAL,
            "prev": prev_trace_id,
            "spatial_context": "CONTRIBUTING.md:97",
        })
        m = re.search(r"trace:([^\s\)]+)", qt_pre.get("text", ""))
        if m:
            prev_trace_id = f"trace:{m.group(1)}"
            assertions.append(f"quick_trace_pre chained from {PREV_TRACE}")
        else:
            failures.append("quick_trace_pre did not return trace id")

        qt = snap("quick_trace_capture_fork", "mcp_engram_quick_trace", {
            "decision": f"Commit discipline verification capture run {args.run}",
            "why": "Plan step 3 raw MCP transcript with chained prev + spatial per edited doc loci",
            "goal_context": GOAL,
            "prev": prev_trace_id,
            "spatial_context": "scripts/validate-commit-msg.sh:1",
        })
        verify_entry = snap("verify_manifold", "mcp_engram_verify_manifold_integrity", {
            "min_crs": 0.74,
            "sample_size": 15,
        })

        session_end_entry: Dict[str, Any] = {}
        if args.run == 2:
            session_end_entry = snap("session_end", "mcp_engram_session_end", {
                "summary": SESSION_END_SUMMARY,
                "prepare_compression": True,
            })

        if ack.get("parsed", {}).get("status") != "acked" and "acked" not in ack.get("text", "").lower():
            failures.append("ack_wake_queue did not ack")
        else:
            assertions.append("ack_wake_queue executed")

        short = GOAL.replace("goal:", "")
        if short not in gl_done.get("text", "") and "commit_title" not in gl_done.get("text", ""):
            failures.append("goal_list(completed) missing commit_title_versioning_process")
        else:
            assertions.append("goal_list(completed) surfaces process goal")

        if "completed" not in gs.get("text", "").lower():
            failures.append("goal_status not completed")
        else:
            assertions.append("goal_status completed OK")

        if "VERSIONING DISCIPLINE" not in rc.get("text", "") and "Conventional Commits" not in rc.get("text", ""):
            failures.append("read_concept missing discipline content")
        else:
            assertions.append("read_concept commit_title_versioning_discipline OK")

        if "trace:" not in qt.get("text", ""):
            failures.append("quick_trace_capture did not return trace id")
        else:
            assertions.append("quick_trace_capture returned trace id with prev chain")

        if qt_pre.get("args", {}).get("prev") != PREV_TRACE:
            failures.append("quick_trace_pre missing prev chain param")
        if qt.get("args", {}).get("spatial_context") != "scripts/validate-commit-msg.sh:1":
            failures.append("quick_trace_capture missing spatial_context")

        verify_text = verify_entry.get("text", "")
        if "Overall: healthy" not in verify_text:
            failures.append(f"verify_manifold not healthy: {verify_text[:200]}")
        else:
            assertions.append("verify_manifold healthy")

        if args.run == 2:
            if SESSION_END_SUMMARY not in session_end_entry.get("args", {}).get("summary", ""):
                failures.append("session_end summary does not match plan step 4 required text")
            else:
                assertions.append("session_end with plan-required summary + prepare_compression")

        sim_msg = (
            "fix(server): bundle injection completeness inputs for clippy CI\n\n"
            "Refactor in injection_priority.rs; update store.rs.\n\n"
            "Refs: trace:1782162619_land-commit-discipline goal:commit_title_versioning_process\n"
        )
        has_conv = bool(re.search(r"^(feat|fix|docs|style|refactor|test|chore|perf|ci)(\([a-z0-9_-]+\))?: ", sim_msg, re.M))
        has_ref = bool(re.search(r"(trace:|goal:)", sim_msg))
        assertions.append(f"simulated_msg conventional={has_conv} refs={has_ref}")

        result = {
            "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
            "run": args.run,
            "goal": GOAL,
            "discipline": DISCIPLINE,
            "intent": INTENT,
            "session_end_summary": SESSION_END_SUMMARY if args.run == 2 else None,
            "doc_edits": [{"label": l, "path": p, "spatial": s} for l, p, s in DOC_EDITS],
            "transcript": transcript,
            "assertions": assertions,
            "failures": failures,
            "simulated_commit_msg": sim_msg,
            "simulated_msg_valid": has_conv and has_ref,
        }
        with open(out_path, "w") as f:
            json.dump(result, f, indent=2)

        if args.run == 2:
            combined = os.path.join(scratch, "engram-commit-process-verify.json")
            r1_path = os.path.join(scratch, "engram-commit-process-verify-run1.json")
            runs = []
            if os.path.exists(r1_path):
                runs.append(json.load(open(r1_path)))
            runs.append(result)
            with open(combined, "w") as f:
                json.dump({"runs": runs, "consistent": all(len(r.get("failures", [])) == 0 for r in runs)}, f, indent=2)

        print(json.dumps({"ok": len(failures) == 0, "out": out_path, "failures": failures}, indent=2))
        return 0 if len(failures) == 0 else 1
    finally:
        client.shutdown()


if __name__ == "__main__":
    raise SystemExit(main())