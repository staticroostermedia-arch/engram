#!/usr/bin/env python3
"""Capture git evidence via git-eng MCP (plan step 2) + shell cross-check."""

from __future__ import annotations

import argparse
import json
import os
import queue
import re
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional

REPO = "/home/a/Documents/Engram"
CONTEXT_SHAS = ["cb5a7541", "58283e64"]
COMPLIANT_SHAS = ["eb4c247b", "1c624578"]
SESSION_END_SUMMARY = (
    "commit process discipline defined per plan + ACs 1-4 exercised; "
    "CONTEXT bad-description gap closed; related to git VC sub and engram_mvp_v1"
)
PROTOCOL_VERSION = "2024-11-05"


class GitEngMcpClient:
    """Minimal stdio MCP client for uvx mcp-server-git."""

    def __init__(self, repo_path: str) -> None:
        self.repo_path = repo_path
        self.proc: Optional[subprocess.Popen] = None
        self.stdout_queue: queue.Queue = queue.Queue()
        self._next_id = 1
        self.errors: List[str] = []

    def start(self) -> bool:
        cmd = ["uvx", "mcp-server-git", "--repository", self.repo_path]
        try:
            self.proc = subprocess.Popen(
                cmd,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                bufsize=0,
            )
        except Exception as e:
            self.errors.append(f"spawn failed: {e}")
            return False

        threading.Thread(target=self._read_stdout, daemon=True).start()
        time.sleep(0.5)
        if self.proc.poll() is not None:
            self.errors.append(f"git-eng exited immediately rc={self.proc.returncode}")
            return False

        init = self._request(
            "initialize",
            {
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {"tools": {}},
                "clientInfo": {"name": "git-eng-evidence-capture", "version": "0.1.0"},
            },
            timeout=20.0,
        )
        if "error" in init:
            self.errors.append(f"initialize: {init['error']}")
            return False
        self._notify("notifications/initialized", {})
        time.sleep(0.2)
        return True

    def shutdown(self) -> None:
        if self.proc and self.proc.poll() is None:
            try:
                self.proc.terminate()
                self.proc.wait(timeout=3)
            except Exception:
                self.proc.kill()

    def _read_stdout(self) -> None:
        assert self.proc and self.proc.stdout
        for line in self.proc.stdout:
            line = line.decode("utf-8", errors="replace").strip()
            if not line:
                continue
            try:
                self.stdout_queue.put(json.loads(line))
            except json.JSONDecodeError:
                pass

    def _notify(self, method: str, params: Dict[str, Any]) -> None:
        assert self.proc and self.proc.stdin
        msg = {"jsonrpc": "2.0", "method": method, "params": params}
        self.proc.stdin.write((json.dumps(msg) + "\n").encode())
        self.proc.stdin.flush()

    def _request(self, method: str, params: Dict[str, Any], timeout: float = 30.0) -> Dict[str, Any]:
        assert self.proc and self.proc.stdin
        rid = self._next_id
        self._next_id += 1
        msg = {"jsonrpc": "2.0", "id": rid, "method": method, "params": params}
        self.proc.stdin.write((json.dumps(msg) + "\n").encode())
        self.proc.stdin.flush()
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                resp = self.stdout_queue.get(timeout=0.2)
            except queue.Empty:
                continue
            if resp.get("id") == rid:
                return resp
        return {"error": {"message": f"timeout on {method}"}}

    def call_tool(self, name: str, arguments: Dict[str, Any]) -> Dict[str, Any]:
        return self._request("tools/call", {"name": name, "arguments": arguments})

    @staticmethod
    def tool_text(resp: Dict[str, Any]) -> str:
        result = resp.get("result") or {}
        content = result.get("content") or []
        parts: List[str] = []
        for item in content:
            if isinstance(item, dict) and item.get("type") == "text":
                parts.append(item.get("text", ""))
        return "\n".join(parts)


def shell_git(args: List[str]) -> str:
    r = subprocess.run(
        ["git", "-C", REPO] + args,
        capture_output=True,
        text=True,
        check=False,
    )
    return (r.stdout or "") + (r.stderr or "")


def has_refs(msg: str) -> bool:
    return bool(re.search(r"(trace:|goal:)", msg))


def commit_message_from_show(text: str) -> str:
    """Extract commit message from git_show MCP text (ignore diff hunks)."""
    m = re.search(r"Message:\s*'((?:\\'|[^'])*)'", text, re.DOTALL)
    if m:
        return m.group(1).replace("\\'", "'")
    m = re.search(r"Message:\s*(.+?)(?:\n\n---|\n\nCommit:|\Z)", text, re.DOTALL)
    return m.group(1).strip() if m else text.split("\n\n---", 1)[0]


def validate_msg(msg: str) -> bool:
    script = os.path.join(REPO, "scripts/validate-commit-msg.sh")
    r = subprocess.run(
        [script, "-"],
        input=msg,
        capture_output=True,
        text=True,
        check=False,
    )
    return r.returncode == 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--scratch", required=True)
    args = ap.parse_args()

    scratch = os.path.abspath(args.scratch)
    os.makedirs(scratch, exist_ok=True)
    out_log = os.path.join(scratch, "git-commit-evidence.log")
    out_json = os.path.join(scratch, "git-eng-mcp-evidence.json")

    lines: List[str] = []
    mcp_transcript: List[Dict[str, Any]] = []
    assertions: List[str] = []
    failures: List[str] = []

    def log(s: str = "") -> None:
        lines.append(s)

    log("=== PLAN STEP 2: git evidence (git-eng MCP + shell cross-check) ===")
    log("")
    log("HONEST ASSERTION POLICY:")
    log("- CONTEXT shas cb5a7541/58283e64: conventional title+body, NO trace/goal refs")
    log("  (non-compliant per new discipline; history NOT rewritten per plan non-goals)")
    log("- Compliant reference commits: eb4c247b, 1c624578 (have Refs: trace + goal)")
    log("- Simulated fix(server) message validates via validate-commit-msg.sh")
    log("")

    client = GitEngMcpClient(REPO)
    mcp_ok = client.start()
    if not mcp_ok:
        failures.append(f"git-eng MCP start failed: {client.errors}")
        log(f"git-eng MCP start FAILED: {client.errors}")
    else:
        log("=== git-eng MCP git_log (raw) ===")
        log_resp = client.call_tool("git_log", {"repo_path": REPO, "max_count": 10})
        log_text = client.tool_text(log_resp)
        log(log_text)
        mcp_transcript.append({
            "tool": "git_log",
            "args": {"repo_path": REPO, "max_count": 10},
            "raw": log_resp,
            "text": log_text,
        })
        if "cb5a7541" in log_text and "eb4c247b" in log_text:
            assertions.append("git_log MCP surfaces CONTEXT + compliant shas")
        else:
            failures.append("git_log MCP missing expected shas")
        log("")

        for sha in CONTEXT_SHAS + COMPLIANT_SHAS:
            log(f"=== git-eng MCP git_show {sha} (raw) ===")
            show_resp = client.call_tool("git_show", {"repo_path": REPO, "revision": sha})
            show_text = client.tool_text(show_resp)
            log(show_text[:4000] + ("..." if len(show_text) > 4000 else ""))
            mcp_transcript.append({
                "tool": "git_show",
                "args": {"repo_path": REPO, "revision": sha},
                "raw": show_resp,
                "text": show_text,
            })
            msg_only = commit_message_from_show(show_text)
            refs = has_refs(msg_only)
            log(f"--- MCP ref check (message only): HAS_REFS={'yes' if refs else 'no'} ---")
            if sha in CONTEXT_SHAS:
                if refs:
                    failures.append(f"{sha}: unexpected refs (CONTEXT shas should lack refs)")
                else:
                    assertions.append(f"{sha}: CONTEXT sha correctly lacks refs (MCP)")
            else:
                if refs:
                    assertions.append(f"{sha}: compliant commit has trace/goal refs (MCP)")
                else:
                    failures.append(f"{sha}: compliant sha missing refs (MCP)")
            log("")
        client.shutdown()

    log("=== shell cross-check: git log --oneline -10 ===")
    log(shell_git(["log", "--oneline", "-10", "--decorate"]))
    log("")

    for sha in CONTEXT_SHAS + COMPLIANT_SHAS:
        log(f"=== shell git show {sha} (cross-check) ===")
        msg = shell_git(["show", sha, "--format=%B", "--no-patch"])
        refs = has_refs(msg)
        log(f"HAS_REFS={'yes' if refs else 'no'}")
        if sha in CONTEXT_SHAS:
            if not refs:
                assertions.append(f"{sha}: shell cross-check lacks refs (expected)")
        else:
            if refs:
                assertions.append(f"{sha}: shell cross-check has refs")
        log("")

    log("=== 1c624578 stat (capture-manage-resume-clear.sh revert honesty) ===")
    stat_out = shell_git(["show", "1c624578", "--stat"])
    log(stat_out)
    if "capture-manage-resume-clear.sh" in stat_out:
        assertions.append("1c624578 includes capture-manage-resume-clear.sh in stat")
    else:
        failures.append("1c624578 stat missing capture-manage-resume-clear.sh")
    log("")

    log("=== Cargo.toml version unchanged across CONTEXT + discipline commits ===")
    for sha in ["cb5a7541", "58283e64", "eb4c247b"]:
        toml = shell_git([f"show", f"{sha}:Cargo.toml"])
        ver = toml.split("version")[1][:30] if "version" in toml else "?"
        log(f"{sha} workspace version snippet: {ver.strip()}")
    log("")

    log("=== CHANGELOG [Unreleased] ===")
    with open(os.path.join(REPO, "CHANGELOG.md")) as f:
        log("".join(f.readlines()[:12]))
    log("")

    eb4_msg = shell_git(["show", "eb4c247b", "--format=%B", "--no-patch"])
    log("=== validate-commit-msg eb4c247b ===")
    log("OK" if validate_msg(eb4_msg) else "FAIL")
    log("")

    sim_msg = (
        "fix(server): bundle injection completeness inputs for clippy CI\n\n"
        "Refactor compute_injection_completeness in injection_priority.rs; update store.rs.\n\n"
        "Refs: trace:1782162619_land-commit-discipline goal:commit_title_versioning_process\n"
    )
    log("=== simulated compliant fix(server) message ===")
    log("OK" if validate_msg(sim_msg) else "FAIL")
    log("")

    with open(out_log, "w") as f:
        f.write("\n".join(lines) + "\n")

    result = {
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "repo_path": REPO,
        "git_eng_mcp_tools": ["git_log", "git_show"],
        "mcp_server": "uvx mcp-server-git",
        "mcp_transcript": mcp_transcript,
        "mcp_start_ok": mcp_ok,
        "assertions": assertions,
        "failures": failures,
        "session_end_summary_required": SESSION_END_SUMMARY,
    }
    with open(out_json, "w") as f:
        json.dump(result, f, indent=2)

    print(json.dumps({"ok": len(failures) == 0, "log": out_log, "failures": failures}, indent=2))
    return 0 if len(failures) == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())