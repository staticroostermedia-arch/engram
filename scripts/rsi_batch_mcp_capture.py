#!/usr/bin/env python3
"""Parameterized RSI batch MCP capture — quick_trace + thought_tile per cycle."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import uuid
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "tools", "test-harness", "python"))
from mcp_test_client import MCPTestClient  # noqa: E402

GOAL = "goal:engram_mvp_v1"


def extract_trace_id(text: str) -> Optional[str]:
    m = re.search(r"trace:([^\s\)\]]+)", text)
    return f"trace:{m.group(1)}" if m else None


def extract_tile_id(parsed: Optional[Dict[str, Any]], text: str) -> Optional[str]:
    if parsed:
        key = parsed.get("tile_key")
        if isinstance(key, str) and key.startswith("tile:"):
            return key
    m = re.search(r"tile:(formal_spec_[^\s\"']+)", text)
    return f"tile:{m.group(1)}" if m else None


def write_session_call(session_mcp_dir: str, cycle: int, label: str, entry: Dict[str, Any]) -> str:
    os.makedirs(session_mcp_dir, exist_ok=True)
    call_id = str(uuid.uuid4())
    path = os.path.join(
        session_mcp_dir, f"call-{call_id}-rsi_cycle{cycle}_{label}.json"
    )
    payload = {
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "rsi_cycle": cycle,
        "label": label,
        "tool": entry.get("tool"),
        "args": entry.get("args"),
        "raw": entry.get("raw"),
        "text": entry.get("text"),
        "parsed": entry.get("parsed"),
    }
    with open(path, "w", encoding="utf-8") as f:
        json.dump(payload, f, indent=2)
    return path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True)
    ap.add_argument("--store", default=os.path.expanduser("~/.engram/stalks"))
    ap.add_argument("--scratch", required=True)
    ap.add_argument("--session-mcp-dir", default="")
    ap.add_argument("--cycle", type=int, required=True)
    ap.add_argument("--decision", required=True)
    ap.add_argument("--title", required=True)
    args = ap.parse_args()

    scratch = os.path.abspath(args.scratch)
    os.makedirs(scratch, exist_ok=True)
    session_mcp_dir = os.path.abspath(args.session_mcp_dir) if args.session_mcp_dir else scratch

    client = MCPTestClient(
        args.binary,
        args.store,
        env_overrides={"ENGRAM_PROFILE": "agent", "ENGRAM_NREM_DISABLE": "1"},
        default_timeout=180.0,
    )
    transcript: List[Dict[str, Any]] = []
    failures: List[str] = []

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
        write_session_call(session_mcp_dir, args.cycle, label, entry)
        return entry

    if not client.start():
        return 2

    try:
        if not client.wait_for_fully_initialized(max_wait=180.0):
            failures.append("backend not fully_initialized")
        else:
            snap("quick_trace", "mcp_engram_quick_trace", {
                "decision": args.decision,
                "why": f"RSI Cycle {args.cycle} batch MCP ritual",
                "goal_context": GOAL,
                "spatial_context": "scripts/rsi_batch_verify.sh:1",
            })
            snap("thought_tile_create", "mcp_engram_thought_tile_create", {
                "tile_type": "formal_spec",
                "title": args.title,
                "goal_context": GOAL,
                "payload": {"cycle": args.cycle, "decision": args.decision},
            })

        trace_id = tile_id = None
        for entry in transcript:
            if entry["tool"] == "mcp_engram_quick_trace":
                trace_id = extract_trace_id(entry.get("text", ""))
            if entry["tool"] == "mcp_engram_thought_tile_create":
                tile_id = extract_tile_id(entry.get("parsed"), entry.get("text", ""))

        if not trace_id:
            failures.append("quick_trace missing trace id")
        if not tile_id:
            failures.append("thought_tile_create missing tile id")

        out = os.path.join(scratch, f"rsi-cycle{args.cycle}-mcp-capture.json")
        result = {
            "cycle": args.cycle,
            "trace_id": trace_id,
            "tile_id": tile_id,
            "transcript": transcript,
            "failures": failures,
        }
        with open(out, "w", encoding="utf-8") as f:
            json.dump(result, f, indent=2)

        mcp_txt = os.path.join(scratch, f"rsi-cycle{args.cycle}-mcp.txt")
        with open(mcp_txt, "w", encoding="utf-8") as f:
            f.write(f"TRACE_ID={trace_id}\nTILE_ID={tile_id}\n")
            for e in transcript:
                f.write(f"\n## {e['label']}\n{e.get('text', '')[:1500]}\n")

        print(json.dumps({"ok": len(failures) == 0, "trace_id": trace_id, "tile_id": tile_id}))
        return 0 if len(failures) == 0 else 1
    finally:
        client.shutdown()


if __name__ == "__main__":
    raise SystemExit(main())