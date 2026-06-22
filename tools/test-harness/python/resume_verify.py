#!/usr/bin/env python3
"""
Resume verification for large live store — simulates post-TUI-restart agent path.

1. wait-ready --json x2 (store load, no MCP lock) → launch evidence
2. Fresh MCP client against live store (if lock free) OR document lock-held + require live MCP poll
3. session_start → rebuild_bvh → poll until full_bvh_gpu (up to 120s)
4. Write {SCRATCH}/resume-verify.json

Usage:
  python3 resume_verify.py --binary target/debug/engram --store ~/.engram/stalks \\
    --scratch /tmp/grok-goal-XXX/implementer
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

sys.path.insert(0, os.path.dirname(__file__))
from mcp_test_client import MCPTestClient  # noqa: E402


def run_wait_ready(binary: str, store: str, timeout: int) -> Dict[str, Any]:
    cmd = [binary, "--store", store, "wait-ready", "--timeout", str(timeout), "--json"]
    proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout + 30)
    out = proc.stdout.strip()
    if out:
        try:
            return json.loads(out.splitlines()[-1])
        except json.JSONDecodeError:
            pass
    # Fallback: parse readiness= from stderr log line
    m = re.search(r"readiness=(\{.*\})", proc.stderr)
    if m:
        try:
            return {"status": "ready", "readiness": json.loads(m.group(1))}
        except json.JSONDecodeError:
            pass
    return {"status": "error", "exit_code": proc.returncode, "stderr_tail": proc.stderr[-500:]}


def poll_readiness(client: MCPTestClient, max_wait: float, interval: float) -> List[Dict[str, Any]]:
    polls: List[Dict[str, Any]] = []
    deadline = time.time() + max_wait
    while time.time() < deadline:
        t0 = time.time()
        resp = client.call_tool("mcp_engram_get_backend_readiness", {}, timeout=60.0)
        data = client._parse_tool_json(resp) or {}
        elapsed = time.time() - t0
        entry = {
            "t_s": round(time.time() - (deadline - max_wait), 1),
            "elapsed_ms": round(elapsed * 1000, 1),
            "readiness": data,
        }
        polls.append(entry)
        rm = data.get("recall_mode")
        if data.get("bvh_ready") and rm == "full_bvh_gpu":
            break
        time.sleep(interval)
    return polls


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True)
    ap.add_argument("--store", default=os.path.expanduser("~/.engram/stalks"))
    ap.add_argument("--scratch", required=True)
    ap.add_argument("--bvh-poll-sec", type=float, default=180.0)
    ap.add_argument("--wait-ready-timeout", type=int, default=180)
    args = ap.parse_args()

    scratch = os.path.abspath(args.scratch)
    os.makedirs(scratch, exist_ok=True)
    launch_log = os.path.join(scratch, "launch-resume.log")
    out_path = os.path.join(scratch, "resume-verify.json")

    result: Dict[str, Any] = {
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "binary": args.binary,
        "store": args.store,
        "simulation": "fresh_mcp_client_against_live_store",
    }

    # --- launch-resume: wait-ready x2 (PRIMARY OBSERVABLE: fully_initialized in readiness JSON) ---
    with open(launch_log, "w") as lf:
        lf.write(f"# launch-resume evidence {result['generated_at']}\n")
        for run in (1, 2):
            wr = run_wait_ready(args.binary, args.store, args.wait_ready_timeout)
            line = json.dumps({"run": run, "wait_ready": wr})
            lf.write(line + "\n")
            result[f"wait_ready_run{run}"] = wr
            ri = (wr.get("readiness") or {}) if isinstance(wr.get("readiness"), dict) else {}
            if run == 1:
                result["launch_primary_observable"] = {
                    "fully_initialized": ri.get("fully_initialized"),
                    "leg_block_count": ri.get("leg_block_count"),
                    "recall_mode": ri.get("recall_mode"),
                }

    # --- fresh MCP (restart sim); mcp_lock may block if TUI holds store ---
    cuda_lib = "/usr/local/cuda/lib64:/usr/local/cuda-13.1/lib64"
    live_env = {
        "ENGRAM_PROFILE": "agent",
        "ENGRAM_DISABLE_SHEAF": "0",
        "ENGRAM_FORCE_CPU_BACKEND": "0",
        "ENGRAM_KI_DISABLE": "0",
        "ENGRAM_NREM_DISABLE": "1",
        "CUDA_HOME": os.environ.get("CUDA_HOME", "/usr/local/cuda"),
        "CUDA_PATH": os.environ.get("CUDA_PATH", "/usr/local/cuda"),
        "LD_LIBRARY_PATH": f"{cuda_lib}:{os.environ.get('LD_LIBRARY_PATH', '')}",
        "PATH": f"/usr/local/cuda/bin:/usr/local/cuda-13.1/bin:{os.environ.get('PATH', '')}",
    }
    client = MCPTestClient(
        args.binary,
        args.store,
        env_overrides=live_env,
        default_timeout=120.0,
        verbose=False,
    )
    # Refuse to steal lock from live TUI MCP — resume on locked store uses live MCP poll path.
    lock_held = False
    lock_path_guess = os.path.expanduser("~/.engram/locks")
    if not client.start():
        err = " ".join(client.errors)
        if "already running" in err.lower() or "mcp lock" in err.lower() or "Another engram MCP" in err:
            lock_held = True
            result["mcp_lock_held"] = True
            result["mcp_lock_note"] = (
                "TUI MCP holds exclusive lock on live store; resume after restart uses that single "
                "process — poll get_backend_readiness + rebuild_bvh via live MCP (not second stdio client)."
            )
            with open(out_path, "w") as f:
                json.dump(result, f, indent=2)
            print(json.dumps(result, indent=2))
            return 2  # signal: complete BVH poll via live MCP
        print(json.dumps({"error": client.errors}, indent=2), file=sys.stderr)
        return 1

    result["mcp_lock_held"] = False
    result["fresh_mcp_pid"] = client.proc.pid if client.proc else None

    if not client.wait_for_fully_initialized(max_wait=90.0):
        result["warning"] = "fully_initialized not confirmed within 90s on fresh MCP"

    ss = client.call_tool(
        "mcp_engram_session_start",
        {"intent": "post-restart verify — resume_verify.py fresh client"},
        timeout=120.0,
    )
    ss_data = client._parse_tool_json(ss) or {}
    cont = ss_data.get("continuation") or {}
    result["session_start"] = {
        "injection_completeness": cont.get("injection_completeness"),
        "nvme_context": cont.get("nvme_context"),
        "suggested_actions_injection_rank": (
            (cont.get("suggested_actions") or [{}])[0].get("injection_rank")
        ),
        "readiness_at_wake": ss_data.get("readiness"),
    }

    rb = client.call_tool("mcp_engram_rebuild_bvh", {}, timeout=30.0)
    result["rebuild_bvh"] = client._parse_tool_json(rb) or {"raw": client._tool_text(rb)[:200]}

    polls = poll_readiness(client, args.bvh_poll_sec, 5.0)
    result["readiness_polls"] = polls
    if polls:
        final = polls[-1].get("readiness") or {}
        result["final_readiness"] = final
        result["full_bvh_gpu_reached"] = (
            final.get("recall_mode") == "full_bvh_gpu" and final.get("bvh_ready") is True
        )

    bundle_resp = client.call_tool("mcp_engram_get_continuation_bundle", {}, timeout=120.0)
    text = client._tool_text(bundle_resp)
    m = re.search(r"\{[\s\S]*\}\s*$", text)
    if m:
        try:
            bundle = json.loads(m.group(0))
            result["full_bundle"] = {
                "injection_completeness": bundle.get("injection_completeness"),
                "nvme_context": bundle.get("nvme_context"),
            }
        except json.JSONDecodeError:
            pass

    client.shutdown()
    with open(out_path, "w") as f:
        json.dump(result, f, indent=2)
    print(json.dumps(result, indent=2))
    return 0 if result.get("full_bvh_gpu_reached") else 3


if __name__ == "__main__":
    raise SystemExit(main())