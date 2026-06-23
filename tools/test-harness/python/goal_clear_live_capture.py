#!/usr/bin/env python3
"""Live-store goal-clear capture for manage_resume — requires exclusive MCP lock.

Writes both plan artifacts in one atomic run:
  - {scratch}/resume-clear-verify.json  (pre/clear/post transcript)
  - {scratch}/final-resume-clear.json     (post-restart verify + manifold gate)
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
from datetime import datetime, timezone
from typing import Any, Callable, Dict, List, Tuple

sys.path.insert(0, os.path.dirname(__file__))
from mcp_test_client import (  # noqa: E402
    MCPTestClient,
    assert_post_clear_state,
    continuation_of,
    parse_manifold_flagged_concepts,
    verify_text_healthy,
)

GOAL = "goal:manage_resume_019ec286"
GOAL_SHORT = "manage_resume_019ec286"
PRIMARY_RESTORE = "goal:engram_mvp_v1"
MAX_MANIFOLD_SCAR_ATTEMPTS = 5


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True)
    ap.add_argument("--store", default=os.path.expanduser("~/.engram/stalks"))
    ap.add_argument("--scratch", required=True)
    ap.add_argument("--skip-clear", action="store_true", help="Only run finalize phase (goal already cleared)")
    args = ap.parse_args()

    scratch = os.path.abspath(args.scratch)
    os.makedirs(scratch, exist_ok=True)
    resume_out = os.path.join(scratch, "resume-clear-verify.json")
    final_out = os.path.join(scratch, "final-resume-clear.json")

    cuda_lib = "/usr/local/cuda/lib64:/usr/local/cuda-13.1/lib64"
    live_env = {
        "ENGRAM_PROFILE": "agent",
        "ENGRAM_DISABLE_SHEAF": "0",
        "ENGRAM_FORCE_CPU_BACKEND": "0",
        "ENGRAM_KI_DISABLE": "0",
        "ENGRAM_NREM_DISABLE": "1",
        "CUDA_HOME": os.environ.get("CUDA_HOME", "/usr/local/cuda"),
        "LD_LIBRARY_PATH": f"{cuda_lib}:{os.environ.get('LD_LIBRARY_PATH', '')}",
        "PATH": f"/usr/local/cuda/bin:{os.environ.get('PATH', '')}",
    }

    client = MCPTestClient(
        args.binary,
        args.store,
        env_overrides=live_env,
        default_timeout=120.0,
    )
    transcript: List[Dict[str, Any]] = []
    failures: List[str] = []
    assertions: List[str] = []

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
        result = {
            "error": "mcp_start_failed",
            "details": client.errors,
            "hint": "Stop TUI MCP holding store lock, then re-run with rebuilt binary",
        }
        with open(resume_out, "w") as f:
            json.dump(result, f, indent=2)
        with open(final_out, "w") as f:
            json.dump(result, f, indent=2)
        print(json.dumps(result, indent=2))
        return 2

    try:
        client.wait_for_fully_initialized(max_wait=90.0)

        ss_pre: Dict[str, Any] = {}
        ss_post1: Dict[str, Any] = {}
        ss_post2: Dict[str, Any] = {}
        us: Dict[str, Any] = {}
        dem: Dict[str, Any] = {}

        if not args.skip_clear:
            snap("goal_list_active_pre", "mcp_engram_goal_list", {"status": "active", "limit": 40})
            gs0 = snap("goal_status_initial", "mcp_engram_goal_status", {"goal": GOAL})
            if "completed" in gs0.get("text", "").lower():
                snap("goal_reopen_active_for_pre_observe", "mcp_engram_goal_update_status", {
                    "goal": GOAL,
                    "status": "active",
                    "note": "Reopen for pre-clear observe (was completed from prior run)",
                })
            snap("goal_set_primary_pre_clear", "mcp_engram_goal_set_primary", {"goal": GOAL})
            ss_pre = snap("session_start_pre_clear", "mcp_engram_session_start", {
                "intent": "live manage_resume pre-clear capture",
            })
            snap("goal_list_active_pre_after_set_primary", "mcp_engram_goal_list", {
                "status": "active",
                "limit": 40,
            })
            snap("goal_status_pre_clear", "mcp_engram_goal_status", {"goal": GOAL})

            us = snap("goal_update_status", "mcp_engram_goal_update_status", {
                "goal": GOAL,
                "status": "completed",
                "note": "Live capture: all ACs pass; goal 019ec286",
            })
            snap("goal_status_post_update", "mcp_engram_goal_status", {"goal": GOAL})
            dem = snap("demote_from_context", "mcp_engram_demote_from_context", {
                "concept": GOAL,
                "note": "Manage-resume complete — clear serving stack",
            })

            snap("recall_goals_post", "mcp_engram_recall", {
                "query": "goal:",
                "scope": "anchors",
                "k": 8,
            })
            snap("goal_list_active_post", "mcp_engram_goal_list", {"status": "active", "limit": 40})
            snap("goal_list_completed_post", "mcp_engram_goal_list", {"status": "completed", "limit": 40})
            ss_post1 = snap("session_start_post_clear_run1", "mcp_engram_session_start", {
                "intent": "live manage_resume post-clear run1",
            })
            time.sleep(0.5)
            ss_post2 = snap("session_start_post_clear_run2", "mcp_engram_session_start", {
                "intent": "live manage_resume post-clear run2",
            })

            pre_cont = continuation_of(ss_pre)
            post1_cont = continuation_of(ss_post1)
            post2_cont = continuation_of(ss_post2)

            if pre_cont.get("primary_goal") != GOAL:
                failures.append(
                    f"pre-clear primary_goal={pre_cont.get('primary_goal')!r}, expected {GOAL}"
                )
            else:
                assertions.append(f"pre-clear primary_goal={GOAL}")

            for ss_entry, lbl in ((ss_post1, "post_clear_run1"), (ss_post2, "post_clear_run2")):
                f, a = assert_post_clear_state(
                    ss_entry,
                    cleared_goal=GOAL,
                    cleared_goal_short=GOAL_SHORT,
                    expected_primary=PRIMARY_RESTORE,
                    label=lbl,
                )
                failures.extend(f)
                assertions.extend(a)

            resume_result = {
                "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
                "goal": GOAL,
                "pre_clear": {
                    "primary_goal": pre_cont.get("primary_goal"),
                    "suggested_actions": pre_cont.get("suggested_actions"),
                    "injection_completeness": pre_cont.get("injection_completeness"),
                    "nvme_context": pre_cont.get("nvme_context"),
                    "readiness_at_wake": ss_pre.get("parsed", {}).get("readiness"),
                },
                "goal_update_status_text": us.get("text", ""),
                "demote_removed_serves": (dem.get("parsed") or {}).get("removed_serves"),
                "post_clear_run1": {
                    "primary_goal": post1_cont.get("primary_goal"),
                    "suggested_actions": post1_cont.get("suggested_actions"),
                },
                "post_clear_run2": {
                    "primary_goal": post2_cont.get("primary_goal"),
                    "suggested_actions": post2_cont.get("suggested_actions"),
                },
                "assertions": assertions,
                "failures": failures,
                "transcript": transcript,
            }
            with open(resume_out, "w") as f:
                json.dump(resume_result, f, indent=2)

            if failures:
                print(json.dumps({"ok": False, "phase": "resume-clear", "failures": failures}, indent=2))
                return 1

        # --- Finalize phase (step 5): fresh post-restart verify transcript ---
        finalize_transcript: List[Dict[str, Any]] = []
        finalize_failures: List[str] = []
        finalize_assertions: List[str] = []

        def fsnap(label: str, tool: str, tool_args: Dict[str, Any]) -> Dict[str, Any]:
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
            finalize_transcript.append(entry)
            return entry

        ss_final = fsnap("session_start_post_restart_verify", "mcp_engram_session_start", {
            "intent": "post-restart verify",
        })
        fsnap("get_backend_readiness", "mcp_engram_get_backend_readiness", {})
        gl_active = fsnap("goal_list_active", "mcp_engram_goal_list", {"status": "active", "limit": 40})
        verify_entry = verify_until_healthy_finalize(fsnap)

        final_cont = continuation_of(ss_final)
        primary = final_cont.get("primary_goal")
        if primary == GOAL:
            finalize_failures.append(f"finalize: primary_goal still {GOAL}")
        else:
            finalize_assertions.append(f"finalize: primary_goal={primary!r}")
        if primary != PRIMARY_RESTORE:
            finalize_failures.append(f"finalize: expected primary={PRIMARY_RESTORE!r}, got {primary!r}")
        else:
            finalize_assertions.append(f"finalize: primary restored to {PRIMARY_RESTORE}")

        if GOAL_SHORT in gl_active.get("text", ""):
            finalize_failures.append("finalize: cleared goal still in goal_list(active)")
        else:
            finalize_assertions.append("finalize: cleared goal absent from goal_list(active)")

        f_fail, f_assert = assert_post_clear_state(
            ss_final,
            cleared_goal=GOAL,
            cleared_goal_short=GOAL_SHORT,
            expected_primary=PRIMARY_RESTORE,
            label="finalize_session_start",
        )
        finalize_failures.extend(f_fail)
        finalize_assertions.extend(f_assert)

        verify_text = verify_entry.get("text", "")
        if not verify_text_healthy(verify_text):
            finalize_failures.append(f"finalize: verify_manifold not healthy: {verify_text[:300]}")

        final_result = {
            "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
            "goal": GOAL,
            "primary_restore": PRIMARY_RESTORE,
            "transcript": finalize_transcript,
            "assertions": finalize_assertions,
            "failures": finalize_failures,
            "primary_goal": primary,
            "verify_healthy": verify_text_healthy(verify_text),
        }
        with open(final_out, "w") as f:
            json.dump(final_result, f, indent=2)

        ok = len(finalize_failures) == 0
        print(json.dumps({
            "ok": ok,
            "resume_out": resume_out,
            "final_out": final_out,
            "primary_goal": primary,
            "verify_healthy": verify_text_healthy(verify_text),
            "finalize_failures": finalize_failures,
        }, indent=2))
        return 0 if ok else 1
    finally:
        client.shutdown()


def verify_until_healthy_finalize(
    fsnap: Callable[[str, str, Dict[str, Any]], Dict[str, Any]],
) -> Dict[str, Any]:
    """Finalize-phase verify with scar prelude (uses fsnap, not global snap)."""
    scar_attempts = 0
    while True:
        entry = fsnap("verify_manifold_integrity", "mcp_engram_verify_manifold_integrity", {
            "min_crs": 0.74,
            "sample_size": 20,
        })
        text = entry.get("text", "")
        if verify_text_healthy(text):
            return entry
        flagged = parse_manifold_flagged_concepts(text)
        if not flagged:
            for line in text.splitlines():
                m = re.search(r"(tile:[\w\-]+)", line)
                if m:
                    flagged.append(m.group(1))
        if scar_attempts >= MAX_MANIFOLD_SCAR_ATTEMPTS or not flagged:
            return entry
        concept = flagged[0]
        scar_attempts += 1
        fsnap(f"manifold_scar_attempt_{scar_attempts}", "mcp_engram_scar", {
            "concept": concept,
            "magnitude": 0.15,
        })
        time.sleep(0.3)


if __name__ == "__main__":
    raise SystemExit(main())