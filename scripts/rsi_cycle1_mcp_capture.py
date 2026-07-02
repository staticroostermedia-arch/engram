#!/usr/bin/env python3
"""Live MCP capture for RSI Cycle 1 AC3 — quick_trace + thought_tile_create."""

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
TILE_TITLE = "RSI Cycle 1 — Surprise-aware sentinel v0.7.0-beta.6"


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


def write_session_call(session_mcp_dir: str, label: str, entry: Dict[str, Any]) -> str:
    os.makedirs(session_mcp_dir, exist_ok=True)
    call_id = str(uuid.uuid4())
    path = os.path.join(session_mcp_dir, f"call-{call_id}-rsi_cycle1_{label}.json")
    payload = {
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
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


def derive_mcp_txt(
    transcript: List[Dict[str, Any]],
    session_mcp_dir: str,
    trace_id: Optional[str],
    tile_id: Optional[str],
) -> str:
    lines = [
        "# RSI Cycle 1 MCP evidence — derived from live stdio MCP capture",
        f"# generated_at: {datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')}",
        f"# session_mcp_dir: {session_mcp_dir}",
        "",
    ]
    if trace_id:
        lines.append(f"TRACE_ID={trace_id}")
    if tile_id:
        lines.append(f"TILE_ID={tile_id}")
    lines.append("")
    for entry in transcript:
        lines.append(f"## {entry.get('label')} ({entry.get('tool')})")
        lines.append(f"args: {json.dumps(entry.get('args', {}), ensure_ascii=False)}")
        lines.append(f"text: {entry.get('text', '')[:2000]}")
        if entry.get("parsed"):
            lines.append(f"parsed: {json.dumps(entry.get('parsed'), ensure_ascii=False)[:2000]}")
        lines.append("")
    if trace_id:
        lines.append("## session_mcp grep backing")
        for root, _, files in os.walk(session_mcp_dir):
            for name in sorted(files):
                if not name.endswith(".json"):
                    continue
                path = os.path.join(root, name)
                try:
                    with open(path, encoding="utf-8") as f:
                        body = f.read()
                except OSError:
                    continue
                if trace_id in body or (tile_id and tile_id in body):
                    lines.append(f"match: {path}")
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--binary", required=True)
    ap.add_argument("--store", default=os.path.expanduser("~/.engram/stalks"))
    ap.add_argument("--scratch", required=True)
    ap.add_argument("--session-mcp-dir", default="")
    args = ap.parse_args()

    scratch = os.path.abspath(args.scratch)
    os.makedirs(scratch, exist_ok=True)
    session_mcp_dir = os.path.abspath(args.session_mcp_dir) if args.session_mcp_dir else scratch

    client = MCPTestClient(
        args.binary,
        args.store,
        env_overrides={
            "ENGRAM_PROFILE": "agent",
            "ENGRAM_NREM_DISABLE": "1",
        },
        default_timeout=180.0,
    )
    transcript: List[Dict[str, Any]] = []
    failures: List[str] = []
    session_call_paths: List[str] = []

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
        session_call_paths.append(write_session_call(session_mcp_dir, label, entry))
        return entry

    if not client.start():
        out = {"error": "mcp_start_failed", "details": client.errors}
        with open(os.path.join(scratch, "rsi-cycle1-mcp-capture.json"), "w") as f:
            json.dump(out, f, indent=2)
        return 2

    try:
        ready = client.wait_for_fully_initialized(max_wait=180.0)
        snap("backend_readiness", "mcp_engram_get_backend_readiness", {})
        if not ready:
            failures.append("backend not fully_initialized before MCP ritual")

        if ready:
            snap(
                "quick_trace",
                "mcp_engram_quick_trace",
                {
                    "decision": "RSI Cycle 1 verification: surprise-aware sentinel with residual wiring",
                    "why": "AC3 MCP ritual — record Cycle 1 deliverable via live stdio capture",
                    "goal_context": GOAL,
                    "spatial_context": "scripts/rsi_cycle1_verify.sh:1",
                    "alternatives": "Hand-authored mcp.txt without session call backing",
                    "deny": "Inventing trace/tile IDs without tool responses",
                    "reconcile": "mcp_test_client capture → session mcp JSON → grep-backed rsi-cycle1-mcp.txt",
                },
            )
            snap(
                "thought_tile_create",
                "mcp_engram_thought_tile_create",
                {
                    "tile_type": "formal_spec",
                    "title": TILE_TITLE,
                    "goal_context": GOAL,
                    "payload": {
                        "cycle": 1,
                        "version": "0.7.0-beta.6",
                        "hypothesis": "hub-anchor l2_norm_residual → surprise_pressure → effective_max_turns",
                        "sources": ["arXiv:2508.05766", "arXiv:2504.09301"],
                        "commits": ["5f1dd4ef", "c06c88c8"],
                        "verification": "scripts/rsi_cycle1_verify.sh",
                    },
                },
            )

        trace_id = None
        tile_id = None
        for entry in transcript:
            if entry["tool"] == "mcp_engram_quick_trace":
                trace_id = extract_trace_id(entry.get("text", ""))
            if entry["tool"] == "mcp_engram_thought_tile_create":
                tile_id = extract_tile_id(entry.get("parsed"), entry.get("text", ""))

        if not trace_id:
            failures.append("quick_trace did not return trace id")
        if not tile_id:
            failures.append("thought_tile_create did not return tile id")

        capture_path = os.path.join(scratch, "rsi-cycle1-mcp-capture.json")
        result = {
            "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
            "trace_id": trace_id,
            "tile_id": tile_id,
            "session_call_paths": session_call_paths,
            "session_mcp_dir": session_mcp_dir,
            "transcript": transcript,
            "failures": failures,
        }
        with open(capture_path, "w", encoding="utf-8") as f:
            json.dump(result, f, indent=2)

        mcp_txt = derive_mcp_txt(transcript, session_mcp_dir, trace_id, tile_id)
        with open(os.path.join(scratch, "rsi-cycle1-mcp.txt"), "w", encoding="utf-8") as f:
            f.write(mcp_txt)

        print(
            json.dumps(
                {
                    "ok": len(failures) == 0,
                    "trace_id": trace_id,
                    "tile_id": tile_id,
                    "capture": capture_path,
                    "failures": failures,
                },
                indent=2,
            )
        )
        return 0 if len(failures) == 0 else 1
    finally:
        client.shutdown()


if __name__ == "__main__":
    raise SystemExit(main())