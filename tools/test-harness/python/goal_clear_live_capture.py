#!/usr/bin/env python3
"""Live-store goal-clear capture for manage_resume — requires exclusive MCP lock."""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from datetime import datetime, timezone
from typing import Any, Dict, List

sys.path.insert(0, os.path.dirname(__file__))
from mcp_test_client import MCPTestClient  # noqa: E402

GOAL = "goal:manage_resume_019ec286"
PRIMARY_RESTORE = "goal:engram_mvp_v1"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True)
    ap.add_argument("--store", default=os.path.expanduser("~/.engram/stalks"))
    ap.add_argument("--scratch", required=True)
    args = ap.parse_args()

    scratch = os.path.abspath(args.scratch)
    os.makedirs(scratch, exist_ok=True)
    out_path = os.path.join(scratch, "resume-clear-verify.json")

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
        with open(out_path, "w") as f:
            json.dump(result, f, indent=2)
        print(json.dumps(result, indent=2))
        return 2

    try:
        client.wait_for_fully_initialized(max_wait=90.0)

        # --- Pre-clear: wire serves + observe ---
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

        # --- Clear: update_status auto-restores marker to parent; demote archives ---
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

        # --- Post-clear observe (marker restored BEFORE session_start) ---
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
        snap("get_backend_readiness", "mcp_engram_get_backend_readiness", {})
        snap("verify_manifold", "mcp_engram_verify_manifold_integrity", {
            "min_crs": 0.74,
            "sample_size": 20,
        })

        def cont_of(entry: Dict[str, Any]) -> Dict[str, Any]:
            parsed = entry.get("parsed") or {}
            return parsed.get("continuation") or parsed

        pre_cont = cont_of(ss_pre)
        post1_cont = cont_of(ss_post1)
        post2_cont = cont_of(ss_post2)

        result = {
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
            "transcript": transcript,
        }
        with open(out_path, "w") as f:
            json.dump(result, f, indent=2)
        print(json.dumps({
            "ok": True,
            "out": out_path,
            "pre_primary": pre_cont.get("primary_goal"),
            "post1_primary": post1_cont.get("primary_goal"),
            "post2_primary": post2_cont.get("primary_goal"),
        }, indent=2))
        return 0
    finally:
        client.shutdown()


if __name__ == "__main__":
    raise SystemExit(main())