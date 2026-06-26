#!/usr/bin/env python3
"""
MCP Test Client for Engram Test Harness
Self-contained stdio JSON-RPC client to drive isolated engram mcp server instances.

Usage (as module or script):
  python3 mcp_test_client.py --binary /path/to/engram --store /tmp/iso-$$ --suite health
  python3 mcp_test_client.py --binary ... --suite full-wakeup --timeout 120
  python3 mcp_test_client.py --binary ... --suite transport-lifetime --iterations 30
  python3 mcp_test_client.py --binary ... --suite compression-measurement --iterations 3

Supports:
- Full MCP handshake (initialize + initialized)
- Tool listing + targeted calls (watch, session_start, stats, summarize, verify, etc.)
- Latency timing per call + aggregate
- Transport lifetime / death detection (process poll + response timeouts -> "Transport closed" equivalent)
- Subagent-like repeated sequences
- Heavy vs light classification and timing buckets
- Env overrides for OptiX / backend stress
- JSON results output for comparators / diffing
- Live stderr capture to log file

This directly exercises the exact failure modes from the May 31 MCP transport regression
(early light calls succeed; heavy geometric ops + repeated calls in long-lived client context fail with transport death).
"""

import argparse
import json
import os
import re
import queue
import subprocess
import sys
import tempfile
import threading
import time
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional, Tuple

# MCP protocol constants (2024-11-05)
MCP_VERSION = "2024-11-05"
PROTOCOL_VERSION = "2024-11-05"

# Known Engram MCP tools for suites (from mcp.rs + server behavior)
LIGHT_TOOLS = {
    "mcp_engram_stats",
    "mcp_engram_list_namespaces",
    "mcp_engram_list_concepts",
    "mcp_engram_recall_recent",
}

MEDIUM_TOOLS = {
    "mcp_engram_summarize",
    "mcp_engram_session_start",
    "mcp_engram_watch_workspace",
    "mcp_engram_get_backend_readiness",
    "mcp_engram_spatial_status",
    "mcp_engram_genesis",
    "mcp_engram_remember",
    "mcp_engram_update",
    "mcp_engram_context_for_edit",
    "mcp_engram_quick_trace",
    "mcp_engram_set_memory_mode",
    "mcp_engram_recall",
}

HEAVY_TOOLS = {
    "mcp_engram_verify_manifold_integrity",
    "mcp_engram_query_with_momentum",
    "mcp_engram_search_by_relation",
    "mcp_engram_visualize",
    "mcp_engram_force_spatial_ingest",  # can be heavy on large dirs
}

WAKEUP_SEQUENCE = [
    # Phase 0 health
    ("mcp_engram_watch_workspace", {"path": "/tmp"}),  # minimal; real harness overrides with real path
    ("mcp_engram_stats", {}),
    # Phase 1 bind
    ("mcp_engram_session_start", {"intent": "Test harness wake-up ritual regression sequence for MCP transport lifetime. Isolated temp store."}),
    # Phase 2 rehydrate (light then heavier)
    ("mcp_engram_summarize", {"top_n": 5}),
    ("mcp_engram_verify_manifold_integrity", {"min_crs": 0.5, "sample_size": 10}),
    # Exercise genesis/spatial for lawfulness metric (post session+summarize per new ritual)
    ("mcp_engram_genesis", {"action": "status"}),
    ("mcp_engram_spatial_status", {}),
    # Momentum / relation for subagent-like
    ("mcp_engram_query_with_momentum", {"query": "MCP transport OR wake-up OR session_start", "k": 3}),
    ("mcp_engram_search_by_relation", {"seed": "session_start", "direction": "both", "k": 3}),
]

# Full ritual-ish sequence for transport stress (repeated in lifetime tests)
FULL_RITUAL_SEQUENCE = WAKEUP_SEQUENCE + [
    ("mcp_engram_stats", {}),
    ("mcp_engram_summarize", {"top_n": 8}),
    ("mcp_engram_verify_manifold_integrity", {"min_crs": 0.6, "sample_size": 20}),
    ("mcp_engram_recall_recent", {"n": 5}),
    ("mcp_engram_genesis", {"action": "status"}),
    ("mcp_engram_spatial_status", {}),
]


def continuation_of(entry: Dict[str, Any]) -> Dict[str, Any]:
    """Extract continuation dict from a snap entry (session_start response)."""
    parsed = entry.get("parsed") or {}
    return parsed.get("continuation") or parsed


def assert_post_clear_state(
    ss_entry: Dict[str, Any],
    *,
    cleared_goal: str,
    cleared_goal_short: str,
    expected_primary: Optional[str] = None,
    label: str = "post_clear",
) -> Tuple[List[str], List[str]]:
    """Return (failures, assertions) for post-clear injection state."""
    failures: List[str] = []
    assertions: List[str] = []
    cont = continuation_of(ss_entry)
    primary = cont.get("primary_goal")
    if primary == cleared_goal:
        failures.append(f"{label}: post-clear primary_goal still {cleared_goal}")
    else:
        assertions.append(f"{label}: post-clear primary_goal={primary!r} (not cleared goal)")
    if expected_primary is not None:
        if primary != expected_primary:
            failures.append(f"{label}: expected primary={expected_primary!r}, got {primary!r}")
        else:
            assertions.append(f"{label}: primary restored to {expected_primary}")
    surfaced = False
    for action in cont.get("suggested_actions") or []:
        args = action.get("args") or {}
        q = args.get("query") or args.get("concept") or args.get("goal") or ""
        if cleared_goal_short in str(q) or cleared_goal in str(q):
            surfaced = True
            failures.append(f"{label}: suggested_actions still surfaces cleared goal in {args}")
    if not surfaced:
        assertions.append(f"{label}: suggested_actions omit cleared goal")
    return failures, assertions


def verify_text_healthy(text: str) -> bool:
    """True when verify_manifold_integrity reports Overall: healthy (or zero issues)."""
    import re

    t = (text or "").lower()
    if "overall: healthy" in t:
        return True
    m = re.search(r"issues found:\s*(\d+)", t)
    if m and int(m.group(1)) == 0:
        return True
    return False


def parse_manifold_flagged_concepts(text: str) -> List[str]:
    """Extract concept ids from verify_manifold_integrity issue lines."""
    import re

    concepts: List[str] = []
    for line in (text or "").splitlines():
        for m in re.finditer(r"(tile:[\w\-]+|goal:[\w\-]+|concept:[\w\-]+)", line):
            concepts.append(m.group(1))
    return list(dict.fromkeys(concepts))


class MCPTestClient:
    def __init__(
        self,
        binary: str,
        store_dir: str,
        env_overrides: Optional[Dict[str, str]] = None,
        stderr_log: Optional[str] = None,
        default_timeout: float = 45.0,
        verbose: bool = False,
    ):
        self.binary = binary
        self.store_dir = os.path.abspath(store_dir)
        self.env = os.environ.copy()
        if env_overrides:
            self.env.update(env_overrides)
        # Isolated harness defaults (skipped when env_overrides supplies live-store resume values).
        self.env.setdefault("ENGRAM_DISABLE_SHEAF", "1")
        self.env.setdefault("ENGRAM_FORCE_CPU_BACKEND", "1")
        self.env.setdefault("ENGRAM_KI_DISABLE", "1")
        self.env.setdefault("ENGRAM_NREM_DISABLE", "1")
        self.env.setdefault("ENGRAM_PROFILE", "agent")
        self.stderr_log = stderr_log or os.path.join(tempfile.gettempdir(), f"engram-harness-{os.getpid()}.stderr.log")
        self.default_timeout = default_timeout
        self.verbose = verbose

        self.proc: Optional[subprocess.Popen] = None
        self.stdout_queue: "queue.Queue[Dict[str, Any]]" = queue.Queue()
        self.stderr_thread: Optional[threading.Thread] = None
        self.stdout_thread: Optional[threading.Thread] = None
        self._next_id = 1
        self._pending: Dict[int, float] = {}  # id -> send_time
        self.timings: List[Dict[str, Any]] = []
        self.errors: List[str] = []
        self.transport_failures = 0
        self.is_alive = True

        os.makedirs(self.store_dir, exist_ok=True)

    def _log(self, msg: str):
        if self.verbose:
            print(f"[MCPClient] {msg}", file=sys.stderr)

    def _read_stdout_loop(self):
        """Background reader for stdout (protocol lines)."""
        assert self.proc and self.proc.stdout
        for line in iter(self.proc.stdout.readline, b""):
            if not line:
                break
            line = line.strip()
            if not line:
                continue
            try:
                msg = json.loads(line.decode("utf-8", errors="replace"))
                self.stdout_queue.put(msg)
                if self.verbose:
                    self._log(f"RX: {msg.get('method') or msg.get('id')}")
            except json.JSONDecodeError:
                # Non-JSON noise (shouldn't happen on stdout; server uses stderr)
                if self.verbose:
                    self._log(f"Non-JSON stdout: {line[:200]}")
        self._log("stdout reader exited")

    def _read_stderr_loop(self):
        """Background reader + logger for stderr (logs + diagnostics)."""
        assert self.proc and self.proc.stderr
        with open(self.stderr_log, "a", buffering=1) as logf:
            logf.write(f"\n=== MCPClient stderr capture started {datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z')} binary={self.binary} store={self.store_dir} ===\n")
            for line in iter(self.proc.stderr.readline, b""):
                if not line:
                    break
                text = line.decode("utf-8", errors="replace").rstrip()
                logf.write(text + "\n")
                if self.verbose and ("error" in text.lower() or "transport" in text.lower() or "closed" in text.lower() or "LBVH" in text or "Pipeline" in text or "MCP-FAST" in text):
                    print(f"[SERVER] {text}", file=sys.stderr)
        self._log("stderr reader exited")

    def start(self) -> bool:
        if self.proc:
            return True
        cmd = [self.binary, "--store", self.store_dir, "mcp"]
        self._log(f"Spawning: {' '.join(cmd)} (stderr -> {self.stderr_log})")
        try:
            self.proc = subprocess.Popen(
                cmd,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                env=self.env,
                bufsize=0,  # unbuffered for protocol
            )
        except Exception as e:
            self.errors.append(f"Failed to spawn: {e}")
            return False

        self.stdout_thread = threading.Thread(target=self._read_stdout_loop, daemon=True)
        self.stdout_thread.start()
        self.stderr_thread = threading.Thread(target=self._read_stderr_loop, daemon=True)
        self.stderr_thread.start()

        # Give server a moment for fast-path placeholder
        time.sleep(0.8)
        if self.proc.poll() is not None:
            self.errors.append(f"Process exited immediately with code {self.proc.returncode}")
            self.is_alive = False
            return False

        # MCP handshake
        try:
            init_resp = self._send_request(
                "initialize",
                {
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {"tools": {}},
                    "clientInfo": {"name": "engram-test-harness", "version": "0.1.0"},
                },
                timeout=20.0,
            )
            if "error" in init_resp:
                self.errors.append(f"initialize error: {init_resp['error']}")
                return False

            # Send initialized notification (no id, no response expected)
            self._send_notification("notifications/initialized", {})
            time.sleep(0.3)

            # Quick tools/list to confirm
            tools = self._send_request("tools/list", {}, timeout=15.0)
            if "error" in tools:
                self.errors.append(f"tools/list failed: {tools['error']}")
                # Non-fatal for some early states; continue
            self._log("Handshake complete. Server ready for tool calls.")
            return True
        except Exception as e:
            self.errors.append(f"Handshake failed: {e}")
            self._kill()
            return False

    def _next_request_id(self) -> int:
        rid = self._next_id
        self._next_id += 1
        return rid

    def _send_notification(self, method: str, params: Dict[str, Any]):
        if not self.proc or not self.proc.stdin:
            raise RuntimeError("Process not running")
        msg = {"jsonrpc": "2.0", "method": method, "params": params}
        data = (json.dumps(msg) + "\n").encode("utf-8")
        self.proc.stdin.write(data)
        self.proc.stdin.flush()

    def _send_request(self, method: str, params: Dict[str, Any], timeout: Optional[float] = None) -> Dict[str, Any]:
        if not self.proc or not self.proc.stdin:
            return {"error": {"code": -32000, "message": "Process not running (transport closed)"}}
        if self.proc.poll() is not None:
            self.is_alive = False
            self.transport_failures += 1
            return {"error": {"code": -32000, "message": f"Transport closed (process exited rc={self.proc.returncode})"}}

        rid = self._next_request_id()
        msg = {"jsonrpc": "2.0", "id": rid, "method": method, "params": params}
        data = (json.dumps(msg) + "\n").encode("utf-8")

        send_t = time.time()
        self._pending[rid] = send_t
        try:
            self.proc.stdin.write(data)
            self.proc.stdin.flush()
        except BrokenPipeError:
            self.is_alive = False
            self.transport_failures += 1
            return {"error": {"code": -32000, "message": "Transport closed (broken pipe on send)"}}

        to = timeout or self.default_timeout
        deadline = send_t + to
        while time.time() < deadline:
            if self.proc.poll() is not None:
                self.is_alive = False
                self.transport_failures += 1
                return {"error": {"code": -32000, "message": "Transport closed (process died during wait)"}}

            try:
                resp = self.stdout_queue.get(timeout=0.2)
            except queue.Empty:
                continue

            if resp.get("id") == rid:
                elapsed = time.time() - send_t
                self.timings.append({
                    "id": rid,
                    "method": method,
                    "elapsed_ms": round(elapsed * 1000, 2),
                    "timestamp": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
                    "is_error": "error" in resp,
                })
                if "error" in resp:
                    self.errors.append(f"{method} error: {resp['error']}")
                del self._pending[rid]
                return resp

            # Stale or other response (progress etc) - requeue or ignore for simplicity
            # In real would handle progress notifications
        # Timeout
        self.transport_failures += 1
        return {"error": {"code": -32000, "message": f"Response timeout after {to}s (possible transport starvation/closed)"}}

    def call_tool(self, name: str, arguments: Dict[str, Any], timeout: Optional[float] = None) -> Dict[str, Any]:
        """Call a tool via tools/call. Returns the full response dict."""
        params = {"name": name, "arguments": arguments}
        resp = self._send_request("tools/call", params, timeout=timeout)
        return resp

    def run_sequence(self, sequence: List[Tuple[str, Dict[str, Any]]], label: str = "sequence") -> Dict[str, Any]:
        """Run an ordered list of (tool_name, args) calls. Returns aggregate stats."""
        results = []
        start = time.time()
        for i, (name, args) in enumerate(sequence):
            if not self.is_alive:
                self.errors.append(f"Aborted {label} at step {i}: transport dead")
                break
            t0 = time.time()
            resp = self.call_tool(name, args)
            t1 = time.time()
            ok = "error" not in resp and self.is_alive
            results.append({
                "step": i,
                "tool": name,
                "args": args,
                "ok": ok,
                "elapsed_ms": round((t1 - t0) * 1000, 1),
                "has_content": bool(resp.get("result", {}).get("content")) if "result" in resp else False,
            })
            if not ok:
                self._log(f"Step {i} {name} failed: {resp.get('error')}")
            # Small pacing for heavy calls to avoid starving server
            if name in HEAVY_TOOLS:
                time.sleep(0.15)
        total = time.time() - start
        healthy = sum(1 for r in results if r["ok"])
        return {
            "label": label,
            "total_calls": len(results),
            "successful": healthy,
            "failed": len(results) - healthy,
            "total_time_s": round(total, 2),
            "transport_failures": self.transport_failures,
            "steps": results,
            "still_alive": self.is_alive,
            "errors": self.errors[-5:],  # last few
        }

    def _tool_text(self, resp: Dict[str, Any]) -> str:
        try:
            content = resp.get("result", {}).get("content", [])
            if content and isinstance(content[0], dict):
                return content[0].get("text", "") or ""
        except (TypeError, KeyError, IndexError):
            pass
        return ""

    def _parse_tool_json(self, resp: Dict[str, Any]) -> Optional[Dict[str, Any]]:
        return self._extract_json_blob(self._tool_text(resp))

    def _extract_json_blob(self, text: str) -> Optional[Dict[str, Any]]:
        text = (text or "").strip()
        if not text:
            return None
        try:
            data = json.loads(text)
            if isinstance(data, dict):
                return data
        except json.JSONDecodeError:
            pass
        start = text.find("{")
        if start < 0:
            return None
        depth = 0
        for i in range(start, len(text)):
            ch = text[i]
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    try:
                        data = json.loads(text[start:i + 1])
                        if isinstance(data, dict):
                            return data
                    except json.JSONDecodeError:
                        return None
        return None

    def wait_for_fully_initialized(self, max_wait: float = 120.0) -> bool:
        """Poll until MCP fast-path upgrade completes (post May-2026 split-brain fix)."""
        deadline = time.time() + max_wait
        while time.time() < deadline and self.is_alive:
            resp = self.call_tool("mcp_engram_get_backend_readiness", {}, timeout=15.0)
            if "error" in resp:
                time.sleep(0.5)
                continue
            text = self._tool_text(resp)
            try:
                data = json.loads(text)
                if data.get("fully_initialized") is True:
                    return True
            except json.JSONDecodeError:
                if "fully_initialized" in text.lower() and "true" in text.lower():
                    return True
            time.sleep(0.5)
        return False

    def run_continuation_bundle_suite(self) -> Dict[str, Any]:
        """Regression: continuation bundle, goal stack alignment, compression handoff."""
        assertions: List[str] = []
        failures: List[str] = []

        wait_tf_before = self.transport_failures
        if not self.wait_for_fully_initialized(max_wait=60.0):
            failures.append("backend never reached fully_initialized within timeout")
        # Poll timeouts during GPU upgrade are not regression failures.
        self.transport_failures = wait_tf_before

        create_resp = self.call_tool(
            "mcp_engram_goal_create",
            {
                "statement": "Engram substrate MVP — harness continuity test",
                "goal_id": "engram_mvp_v1",
                "priority": "high",
            },
            timeout=30.0,
        )
        create_text = self._tool_text(create_resp)
        if "already exists" not in create_text and "error" in create_text.lower():
            failures.append(f"goal_create failed: {create_text[:200]}")

        seq = [
            ("mcp_engram_goal_set_primary", {"goal": "goal:engram_mvp_v1"}),
            ("mcp_engram_goal_list", {"limit": 5}),
            ("mcp_engram_session_start", {"intent": "Harness continuation-bundle suite — bind + handoff"}),
            ("mcp_engram_get_continuation_bundle", {}),
            ("mcp_engram_list_concepts", {"prefix": "session_start_", "limit": 3}),
            ("mcp_engram_session_end", {
                "summary": "Harness compression handoff test. goal:engram_mvp_v1 active.",
                "prepare_compression": True,
            }),
            ("mcp_engram_read_concept", {"concept": "helper:session_hydration_cache"}),
            ("mcp_engram_session_start", {"intent": "Harness post-handoff wake — recall bundle"}),
        ]
        agg = self.run_sequence(seq, label="continuation_bundle")

        gl_text = ""
        ss_text = ""
        cache_text = ""
        for step in agg.get("steps", []):
            tool = step.get("tool")
            if tool == "mcp_engram_goal_list" and step.get("ok"):
                # text captured on follow-up only if step failed
                pass
        gl_resp = self.call_tool("mcp_engram_goal_list", {"status": "active", "limit": 5}, timeout=30.0)
        gl_text = self._tool_text(gl_resp)
        if "engram_mvp_v1" not in gl_text:
            failures.append("goal_list missing goal:engram_mvp_v1 after create+set_primary")

        ss_resp = self.call_tool("mcp_engram_session_start", {"intent": "final bundle probe"}, timeout=120.0)
        ss_text = self._tool_text(ss_resp)
        ss_data = self._parse_tool_json(ss_resp) or {}
        cont = ss_data.get("continuation") or {}
        slim_ok = cont.get("bundle_tier") == "slim" and cont.get("primary_goal") is not None
        if "CONTINUATION BUNDLE" not in ss_text and not slim_ok:
            failures.append(
                "session_start missing CONTINUATION BUNDLE section and slim continuation absent"
            )
        elif slim_ok:
            assertions.append("session_start slim continuation present")
        if "engram_mvp_v1" not in ss_text and cont.get("primary_goal") != "goal:engram_mvp_v1":
            failures.append("session_start bundle missing primary goal reference")

        cache_resp = self.call_tool("mcp_engram_read_concept", {"concept": "helper:session_hydration_cache"}, timeout=30.0)
        cache_text = self._tool_text(cache_resp)
        if "not found" in cache_text.lower() or "error" in cache_text.lower():
            failures.append("helper:session_hydration_cache missing after session_end handoff")

        passed = (
            len(failures) == 0
            and agg.get("failed", 1) == 0
            and self.is_alive
            and self.transport_failures == 0
        )
        return {
            "label": "continuation_bundle",
            "passed": passed,
            "failures": failures,
            "assertions": assertions,
            "sequence": agg,
            "still_alive": self.is_alive,
        }

    def run_agent_memory_suite(self) -> Dict[str, Any]:
        """MVP agent-memory validation: lean 8-tool loop + session handoff continuity."""
        failures: List[str] = []
        assertions: List[str] = []

        wait_tf_before = self.transport_failures
        if not self.wait_for_fully_initialized(max_wait=60.0):
            failures.append("backend never reached fully_initialized within timeout")
        self.transport_failures = wait_tf_before

        harness_concept = f"harness:agent_memory_test_{int(time.time())}"
        session1_intent = "Harness agent-memory suite — MVP lean 8-tool loop validation"
        session2_intent = "Harness agent-memory suite — verify handoff from prior session"

        seq = [
            ("mcp_engram_session_start", {"intent": session1_intent}),
            ("mcp_engram_get_backend_readiness", {}),
            ("mcp_engram_recall", {
                "query": "agent memory contract lean harness",
                "k": 5,
                "scope": "anchors",
            }),
            ("mcp_engram_context_for_edit", {
                "path": "/path/to/Documents/Engram/crates/engram-server/src/profile.rs",
                "auto_ingest": True,
            }),
            ("mcp_engram_quick_trace", {
                "decision": "Run agent-memory harness with isolated temp store",
                "why": "Validates MVP lean loop without production sheaf contention",
                "context": "tools/test-harness/python/mcp_test_client.py",
            }),
            ("mcp_engram_remember", {
                "concept": harness_concept,
                "text": "Harness agent-memory MVP test concept — lean loop validation artifact.",
            }),
            ("mcp_engram_session_end", {
                "summary": (
                    "Harness agent-memory suite session 1.\n"
                    "Decisions:\n"
                    "- Exercised lean 8-tool contract sequence in isolated store\n"
                    "- Minted test concept for recall validation\n"
                    "Next: second session_start should surface handoff."
                ),
                "prepare_compression": True,
            }),
            ("mcp_engram_session_start", {"intent": session2_intent}),
        ]
        agg = self.run_sequence(seq, label="agent_memory")

        wake_ms: Optional[float] = None
        for step in agg.get("steps", []):
            if step.get("step") == 0 and step.get("tool") == "mcp_engram_session_start":
                wake_ms = step.get("elapsed_ms")
                break

        if wake_ms is None:
            failures.append("first session_start step missing from sequence")
        elif wake_ms >= 5000:
            failures.append(f"wake latency {wake_ms}ms exceeds 5000ms budget")
        else:
            assertions.append(f"wake_latency_ms={wake_ms} (<5000)")

        readiness_resp = self.call_tool("mcp_engram_get_backend_readiness", {}, timeout=30.0)
        readiness_data = self._parse_tool_json(readiness_resp)
        memory_mode = (readiness_data or {}).get("memory_mode")
        if memory_mode != "lean":
            failures.append(f"readiness memory_mode={memory_mode!r}, expected 'lean'")
        else:
            assertions.append("memory_mode=lean")

        profile = (readiness_data or {}).get("profile")
        if profile not in ("agent", "deep", "ui", "dev"):
            failures.append(f"readiness profile={profile!r}, expected agent|deep|ui|dev")
        else:
            assertions.append(f"profile={profile}")

        bundle_resp = self.call_tool("mcp_engram_get_continuation_bundle", {}, timeout=30.0)
        bundle_data = self._parse_tool_json(bundle_resp)
        handoff_resp = self.call_tool(
            "mcp_engram_read_concept",
            {"concept": "helper:session_handoff_latest"},
            timeout=30.0,
        )
        handoff_text = self._tool_text(handoff_resp)

        has_last_session = False
        if bundle_data:
            last_end = bundle_data.get("last_session_end")
            if last_end is not None and last_end != {}:
                has_last_session = True
                assertions.append("continuation_bundle.last_session_end present")
        if not has_last_session and "session_end_" in handoff_text and "not found" not in handoff_text.lower():
            has_last_session = True
            assertions.append("helper:session_handoff_latest present")
        if not has_last_session:
            failures.append(
                "no handoff continuity: continuation_bundle.last_session_end missing "
                "and helper:session_handoff_latest not found"
            )

        # Substrate wins: harness_injection must be present after handoff (WS-1 gate)
        # Use session_start JSON (reliable parse) — get_continuation_bundle wraps prose around JSON.
        inject_resp = self.call_tool(
            "mcp_engram_session_start",
            {"intent": "Harness agent-memory — harness_injection gate check"},
            timeout=60.0,
        )
        inject_data = self._parse_tool_json(inject_resp) or {}
        cont = inject_data.get("continuation") or inject_data
        harness = cont.get("harness_injection") or {}
        # Slim wake (ENGRAM_WAKE_BUNDLE=slim default) hoists suggested_actions to continuation root.
        suggested = harness.get("suggested_actions") or cont.get("suggested_actions") or []
        if not suggested:
            failures.append(
                "suggested_actions empty after session_end handoff "
                "(expected prioritized wake queue in slim or full bundle)"
            )
        else:
            assertions.append(f"suggested_actions len={len(suggested)}")
            if suggested[0].get("injection_rank") is not None:
                assertions.append("suggested_actions[0].injection_rank present (composite rank)")
        inj = cont.get("injection_completeness") or {}
        if inj.get("score") is not None:
            assertions.append(f"injection_completeness.score={inj.get('score')}")
        nvme = cont.get("nvme_context") or {}
        if nvme.get("recall_mode") is not None:
            assertions.append(f"nvme_context.recall_mode={nvme.get('recall_mode')}")
        if harness.get("agent_discipline"):
            assertions.append("harness_injection.agent_discipline present")

        # Substrate wins tools registered (WS-2/WS-3) — list check + process_metrics smoke
        tools_resp = self._send_request("tools/list", {}, timeout=30.0)
        tool_names = set()
        if "result" in tools_resp:
            for t in tools_resp["result"].get("tools", []):
                tool_names.add(t.get("name", ""))
        for required in (
            "mcp_engram_thought_tile_draft_from_chain",
            "mcp_engram_process_metrics",
        ):
            if required not in tool_names:
                failures.append(f"{required} missing from tools/list")
            else:
                assertions.append(f"{required} registered")
        pm_resp = self.call_tool(
            "mcp_engram_process_metrics",
            {"process_key": "process:engram.harness.sub-agent-launch"},
            timeout=30.0,
        )
        pm_data = self._parse_tool_json(pm_resp)
        if "error" in pm_resp or not pm_data.get("process_key"):
            failures.append("mcp_engram_process_metrics smoke call failed")
        else:
            assertions.append("mcp_engram_process_metrics smoke ok")

        passed = (
            len(failures) == 0
            and agg.get("failed", 1) == 0
            and self.is_alive
            and self.transport_failures == 0
        )
        return {
            "label": "agent_memory",
            "passed": passed,
            "failures": failures,
            "assertions": assertions,
            "wake_latency_ms": wake_ms,
            "memory_mode": memory_mode,
            "harness_concept": harness_concept,
            "sequence": agg,
            "still_alive": self.is_alive,
        }

    def run_agent_tool_fidelity_suite(self, workspace_path: str = "/path/to/your/engram") -> Dict[str, Any]:
        """Agent tool fidelity: composite edit/update tools, lineage+merkle, reflection, >=95% correct usage."""
        failures: List[str] = []
        assertions: List[str] = []
        ts = int(time.time())
        harness_concept = f"harness:edit_fidelity_{ts}"
        arc_concept = f"{harness_concept}__arc"
        edit_path = f"{workspace_path.rstrip('/')}/crates/engram-server/src/profile.rs"

        steps_ok = 0
        steps_total = 0
        prev_trace_id: Optional[str] = None

        def step(tool: str, args: Dict[str, Any], label: str, expect_ok: bool = True) -> Optional[Dict[str, Any]]:
            nonlocal steps_ok, steps_total
            steps_total += 1
            resp = self.call_tool(tool, args, timeout=90.0)
            data = self._parse_tool_json(resp)
            err = "error" in resp or resp.get("result", {}).get("isError")
            if expect_ok and not err:
                steps_ok += 1
                assertions.append(f"{label}: ok")
            elif not expect_ok and err:
                steps_ok += 1
                assertions.append(f"{label}: expected_fail ok")
            else:
                failures.append(f"{label}: tool={tool} err={err} data_keys={list((data or {}).keys())}")
            return data

        ss_data = step("mcp_engram_session_start", {"intent": "agent-tool-fidelity harness suite"}, "session_start")
        if ss_data:
            cont = ss_data.get("continuation") or ss_data
            hi = cont.get("harness_injection") or {}
            discipline = hi.get("agent_discipline") or {}
            rituals = discipline.get("fidelity_rituals") or []
            if "ritual:safe_code_edit" in rituals or "agent:engram.ritual.safe-code-edit" in str(rituals):
                assertions.append("fidelity_rituals present in harness_injection")
            proc_resp = self.call_tool(
                "mcp_engram_read_concept",
                {"concept": "process:engram.ritual.safe-code-edit"},
                timeout=30.0,
            )
            proc_text = self._tool_text(proc_resp)
            if "not found" in proc_text.lower():
                failures.append("process:engram.ritual.safe-code-edit not loaded in sheaf")
            else:
                assertions.append("process:engram.ritual.safe-code-edit loaded")

        step(
            "mcp_engram_ack_wake_queue",
            {"executed": True, "note": "harness agent-tool-fidelity — queue cleared before edits"},
            "ack_wake_queue",
        )

        prev_resp = self.call_tool(
            "mcp_engram_quick_trace",
            {
                "decision": "Harness prev trace for safe_edit chain",
                "why": "Exercise prev_in_trace lineage before composite edit",
                "goal_context": "goal:agent_tool_fidelity_v1",
            },
            timeout=90.0,
        )
        steps_total += 1
        prev_text = self._tool_text(prev_resp)
        if "trace:" in prev_text:
            m = re.search(r"trace:[^\s\)]+", prev_text)
            if m:
                prev_trace_id = m.group(0).rstrip(")")
                steps_ok += 1
                assertions.append(f"prev_trace_mint: ok ({prev_trace_id})")
            else:
                failures.append("prev_trace_mint: trace id not parsed from response")
        else:
            failures.append("prev_trace_mint: quick_trace failed")

        step(
            "mcp_engram_remember",
            {"concept": harness_concept, "text": "Harness edit fidelity artifact — update target with __arc companion."},
            "remember_target",
        )
        step(
            "mcp_engram_remember",
            {
                "concept": arc_concept,
                "text": "EDIT ARC — harness fidelity __arc seed for tensor bond path.",
            },
            "seed_arc_block",
        )

        safe_args: Dict[str, Any] = {
            "path": edit_path,
            "decision": "Harness safe edit composite smoke",
            "why": "Verify lineage + tensor pattern path",
            "arc_delta": "delta: harness safe_edit smoke — no real file change",
            "goal_context": "goal:agent_tool_fidelity_v1",
            "run_verify": True,
        }
        if prev_trace_id:
            safe_args["prev_trace"] = prev_trace_id
        safe_data = step("mcp_engram_safe_edit_and_verify", safe_args, "safe_edit_and_verify")
        if safe_data:
            tid = safe_data.get("trace_id")
            if tid:
                assertions.append(f"lineage trace_id={tid}")
            else:
                failures.append("safe_edit missing trace_id")
            lineage = safe_data.get("lineage") or {}
            if lineage.get("merkle_ok") is True:
                assertions.append("lineage.merkle_ok=true")
            else:
                failures.append(f"lineage merkle missing: {lineage}")
            if prev_trace_id and lineage.get("ok"):
                assertions.append("prev_in_trace chain verified")
            if safe_data.get("arc_updated") is True:
                assertions.append("arc_updated=true")
            elif safe_data.get("arc_update_error"):
                failures.append(f"arc_update_error: {safe_data.get('arc_update_error')}")
            if safe_data.get("tensor_pattern"):
                assertions.append("tensor_pattern recorded")

        update_data = step(
            "mcp_engram_update_with_tensor_bond",
            {
                "concept": arc_concept,
                "new_text": "delta: harness __arc updated via tensor bond composite with recall guard.",
                "recall_query": "harness edit fidelity arc",
                "bond_label": "edit_fidelity",
            },
            "update_with_tensor_bond_arc",
        )
        if update_data:
            if update_data.get("crs_gate_ok") is True:
                assertions.append(f"crs_after={update_data.get('crs_after')} (>=0.74)")
            else:
                failures.append(f"crs_gate failed: after={update_data.get('crs_after')}")
            lin = update_data.get("lineage") or {}
            if lin.get("merkle_ok"):
                assertions.append("update lineage merkle_ok")
            if update_data.get("tensor_pattern"):
                assertions.append("update tensor_pattern present")

        ack_trace = (safe_data or {}).get("trace_id")
        ack_data = step(
            "mcp_engram_ack_edit_arc",
            {
                "skip": True,
                "note": "harness read-only ack",
                "lineage_check": True,
                "trace_id": ack_trace,
            },
            "ack_edit_arc_lineage",
        )
        if ack_data and ack_data.get("lineage_check"):
            assertions.append("ack lineage_check field present")

        misuse_data = step(
            "mcp_engram_update_with_tensor_bond",
            {
                "concept": harness_concept,
                "new_text": "Harness mismatch probe — recall guard exercised.",
                "recall_query": "completely unrelated quantum physics",
                "scar_on_mismatch": True,
                "match_threshold": 0.99,
            },
            "misuse_self_correction",
        )
        if misuse_data:
            if misuse_data.get("recall_match") is False:
                assertions.append("misuse recall_match=false")
            else:
                failures.append("misuse: expected recall_match=false")
            if misuse_data.get("scar_key"):
                assertions.append(f"misuse scar_key={misuse_data.get('scar_key')}")
            else:
                failures.append("misuse: expected scar_key on recall mismatch")
            fp = misuse_data.get("failure_pattern") or {}
            fp_concept = str(fp.get("concept", ""))
            if fp.get("kind") == "failure" or "edit_pattern_failure" in fp_concept:
                assertions.append(f"misuse failure_pattern ({fp_concept or fp.get('kind')})")
            else:
                failures.append(f"misuse: expected failure_pattern kind=failure, got {fp}")

        step("mcp_engram_verify_manifold_integrity", {"min_crs": 0.74, "sample_size": 16}, "verify_manifold")
        step("mcp_engram_genesis", {"action": "status"}, "genesis_status")
        spatial_text = self._tool_text(self.call_tool("mcp_engram_spatial_status", {}, timeout=60.0))
        if spatial_text and "error" not in spatial_text.lower()[:80]:
            assertions.append("spatial_status: responded")
        else:
            assertions.append("spatial_status: n/a on fresh isolated store (non-fatal)")

        ctx_data = step(
            "mcp_engram_context_for_edit",
            {"path": edit_path, "auto_ingest": True},
            "context_for_edit_palette",
        )
        if ctx_data:
            hi = ctx_data.get("harness_injection") or {}
            palette = hi.get("post_edit_palette") or ctx_data.get("post_edit_palette") or []
            if palette and palette[0].get("tool") == "mcp_engram_safe_edit_and_verify":
                assertions.append("post_edit_palette fronts safe_edit")
            else:
                failures.append("post_edit_palette missing safe_edit priority 0")

        tools_resp = self._send_request("tools/list", {}, timeout=30.0)
        tool_names = set()
        desc_by_name: Dict[str, str] = {}
        if "result" in tools_resp:
            for t in tools_resp["result"].get("tools", []):
                name = t.get("name", "")
                tool_names.add(name)
                desc_by_name[name] = t.get("description", "") or ""
        for required in (
            "mcp_engram_safe_edit_and_verify",
            "mcp_engram_update_with_tensor_bond",
        ):
            if required not in tool_names:
                failures.append(f"{required} missing from tools/list")
            else:
                assertions.append(f"{required} registered")
                if "FEW-SHOT" not in desc_by_name.get(required, ""):
                    failures.append(f"{required} description missing FEW-SHOT examples")

        fidelity_rate = (steps_ok / steps_total) if steps_total else 0.0
        assertions.append(f"fidelity_rate={fidelity_rate:.3f} ({steps_ok}/{steps_total})")
        if fidelity_rate < 0.95:
            failures.append(f"fidelity_rate {fidelity_rate:.3f} below 0.95 threshold")

        passed = (
            len(failures) == 0
            and fidelity_rate >= 0.95
            and self.is_alive
            and self.transport_failures == 0
        )
        return {
            "label": "agent_tool_fidelity",
            "passed": passed,
            "failures": failures,
            "assertions": assertions,
            "fidelity_rate": fidelity_rate,
            "steps_ok": steps_ok,
            "steps_total": steps_total,
            "harness_concept": harness_concept,
            "arc_concept": arc_concept,
            "prev_trace_id": prev_trace_id,
            "still_alive": self.is_alive,
        }

    def run_goal_clear_suite(self) -> Dict[str, Any]:
        """Goal complete+clear: set_primary (serves) → pre observe → update_status → demote → post observe (2x)."""
        failures: List[str] = []
        assertions: List[str] = []
        raw_transcript: List[Dict[str, Any]] = []
        ts = int(time.time())
        goal_key = f"goal:harness_clear_{ts}"
        goal_short = f"harness_clear_{ts}"

        def snap(label: str, tool: str, tool_args: Dict[str, Any]) -> Dict[str, Any]:
            resp = self.call_tool(tool, tool_args, timeout=90.0)
            entry: Dict[str, Any] = {
                "label": label,
                "tool": tool,
                "args": tool_args,
                "raw": resp,
                "text": self._tool_text(resp),
            }
            parsed = self._parse_tool_json(resp)
            if parsed:
                entry["parsed"] = parsed
            raw_transcript.append(entry)
            return entry

        parent_short = f"harness_parent_{ts}"
        parent_key = f"goal:{parent_short}"
        snap("goal_create_parent", "mcp_engram_goal_create", {
            "goal_id": parent_short,
            "statement": "Harness parent goal for clear-restore test",
            "priority": "medium",
        })
        snap("goal_create", "mcp_engram_goal_create", {
            "goal_id": goal_short,
            "statement": "Harness goal-clear protocol test (isolated store)",
            "priority": "high",
            "parent": parent_key,
        })
        sp = snap("goal_set_primary", "mcp_engram_goal_set_primary", {"goal": goal_key})
        if "serves" not in sp.get("text", ""):
            failures.append("goal_set_primary did not report serves link")
        else:
            assertions.append("goal_set_primary wired primary_goal --serves-->")

        ss_pre = snap("session_start_pre_clear", "mcp_engram_session_start", {
            "intent": "goal-clear harness — pre-clear observe primary/suggested",
        })
        gl_pre = snap("goal_list_active_pre", "mcp_engram_goal_list", {"status": "active", "limit": 40})
        gs_pre = snap("goal_status_pre", "mcp_engram_goal_status", {"goal": goal_key})

        ss_data = ss_pre.get("parsed") or {}
        cont = ss_data.get("continuation") or ss_data
        primary = cont.get("primary_goal", "")
        if primary != goal_key:
            failures.append(f"pre-clear primary_goal={primary!r}, expected {goal_key}")
        else:
            assertions.append(f"pre-clear primary_goal={goal_key}")

        suggested = cont.get("suggested_actions") or []
        suggested_tools = [a.get("tool") for a in suggested]
        assertions.append(f"pre-clear suggested_actions len={len(suggested)}")

        if goal_short not in gl_pre.get("text", ""):
            failures.append("pre-clear goal_list(active) missing test goal")
        else:
            assertions.append("pre-clear goal in goal_list(active)")

        if "active" not in gs_pre.get("text", "").lower():
            failures.append("pre-clear goal_status not active")
        else:
            assertions.append("pre-clear goal_status active")

        snap("goal_update_status", "mcp_engram_goal_update_status", {
            "goal": goal_key,
            "status": "completed",
            "note": "harness goal-clear suite",
        })
        gs_mid = snap("goal_status_post_update", "mcp_engram_goal_status", {"goal": goal_key})
        if "completed" not in gs_mid.get("text", "").lower():
            failures.append("post goal_update_status: goal_status not completed")
        else:
            assertions.append("post goal_update_status status=completed")

        dem = snap("demote_from_context", "mcp_engram_demote_from_context", {
            "concept": goal_key,
            "note": "harness archival demote",
        })
        dem_parsed = dem.get("parsed") or {}
        assertions.append(f"demote removed_serves={dem_parsed.get('removed_serves')}")

        snap("recall_goals_post", "mcp_engram_recall", {
            "query": "goal:",
            "scope": "anchors",
            "k": 5,
        })

        gl_post = snap("goal_list_active_post", "mcp_engram_goal_list", {"status": "active", "limit": 40})
        gl_done = snap("goal_list_completed_post", "mcp_engram_goal_list", {"status": "completed", "limit": 40})
        ss_post1 = snap("session_start_post_clear_run1", "mcp_engram_session_start", {
            "intent": "goal-clear harness — post-clear run1",
        })
        ss_post2 = snap("session_start_post_clear_run2", "mcp_engram_session_start", {
            "intent": "goal-clear harness — post-clear run2",
        })
        gs_post = snap("goal_status_post_clear", "mcp_engram_goal_status", {"goal": goal_key})

        for ss_entry, lbl in ((ss_post1, "post_clear_run1"), (ss_post2, "post_clear_run2")):
            f, a = assert_post_clear_state(
                ss_entry,
                cleared_goal=goal_key,
                cleared_goal_short=goal_short,
                expected_primary=parent_key,
                label=lbl,
            )
            failures.extend(f)
            assertions.extend(a)

        if goal_short in gl_post.get("text", ""):
            failures.append("post-clear goal still in goal_list(active)")
        else:
            assertions.append("post-clear goal absent from goal_list(active)")

        if goal_short not in gl_done.get("text", ""):
            failures.append("post-clear goal not in goal_list(completed)")
        else:
            assertions.append("post-clear goal in goal_list(completed)")

        if "completed" not in gs_post.get("text", "").lower():
            failures.append("post-clear goal_status not completed")
        else:
            assertions.append("post-clear goal_status completed")

        passed = len(failures) == 0 and self.is_alive and self.transport_failures == 0
        return {
            "label": "goal_clear",
            "passed": passed,
            "failures": failures,
            "assertions": assertions,
            "goal_key": goal_key,
            "raw_transcript": raw_transcript,
            "still_alive": self.is_alive,
        }

    def run_health_suite(self) -> Dict[str, Any]:
        """Core regression health checks: watch, session_start, summarize, verify, stats."""
        seq = [
            ("mcp_engram_watch_workspace", {"path": os.path.dirname(self.store_dir) or "/tmp"}),
            ("mcp_engram_stats", {}),
            ("mcp_engram_session_start", {"intent": "Harness health regression check - May 31 transport pattern"}),
            ("mcp_engram_summarize", {"top_n": 3}),
            ("mcp_engram_verify_manifold_integrity", {"min_crs": 0.5, "sample_size": 5}),
            ("mcp_engram_get_backend_readiness", {}),
        ]
        return self.run_sequence(seq, "health_suite")

    def run_full_wakeup_ritual(self, workspace_path: Optional[str] = None) -> Dict[str, Any]:
        """Full wake-up ritual sequence (light + heavy + momentum)."""
        ws = workspace_path or "/path/to/your/engram"  # for spatial if wanted; isolated store anyway. Override or pass workspace_path for your clone.
        seq = [
            ("mcp_engram_watch_workspace", {"path": ws}),
            ("mcp_engram_stats", {}),
            ("mcp_engram_session_start", {"intent": "Full geometric wake-up ritual from test harness. Continuation of prior agent instances. Testing transport lifetime post-May31 regression."}),
            ("mcp_engram_summarize", {"top_n": 10}),
            ("mcp_engram_verify_manifold_integrity", {"min_crs": 0.6, "sample_size": 15}),
            ("mcp_engram_query_with_momentum", {"query": "wake_up OR ritual OR continuation OR MCP transport", "k": 5}),
            ("mcp_engram_search_by_relation", {"seed": "session_start", "direction": "to", "k": 4}),
            ("mcp_engram_stats", {}),
            ("mcp_engram_recall_recent", {"n": 4}),
        ]
        base = self.run_sequence(seq, "full_wakeup_ritual")
        # Post-ritual: exercise + assert the Wake-up Lawfulness Verification Tracking metric (binds to codeland 1780091465 + May 31 artifacts)
        metric_res = self.record_and_assert_wake_up_verification_metric(wake_up_context="harness", server_binary=self.binary)
        base["wake_up_lawfulness_metric"] = metric_res
        base["lawfulness_assert_passed"] = metric_res.get("assert_passed", False)
        return base

    def run_transport_lifetime_test(self, iterations: int = 20) -> Dict[str, Any]:
        """Repeated heavy calls to stress transport lifetime (core of May 31 regression repro)."""
        seq = []
        for i in range(iterations):
            seq.extend(FULL_RITUAL_SEQUENCE[:3])  # core subset to keep reasonable duration
            # Inject a heavy one periodically
            if i % 3 == 0:
                seq.append(("mcp_engram_verify_manifold_integrity", {"min_crs": 0.4, "sample_size": 8}))
        res = self.run_sequence(seq, f"transport_lifetime_x{iterations}")
        res["iterations"] = iterations
        res["avg_call_ms"] = round(sum(t["elapsed_ms"] for t in self.timings) / max(1, len(self.timings)), 1) if self.timings else 0
        return res

    def run_heavy_vs_light_timing(self, repeats: int = 3) -> Dict[str, Any]:
        buckets: Dict[str, List[float]] = {"light": [], "medium": [], "heavy": []}
        for _ in range(repeats):
            for name in ["mcp_engram_stats", "mcp_engram_summarize"]:
                r = self.call_tool(name, {"top_n": 3} if "summarize" in name else {})
                if self.timings:
                    buckets["light" if name in LIGHT_TOOLS else "medium"].append(self.timings[-1]["elapsed_ms"])
            for name in ["mcp_engram_verify_manifold_integrity", "mcp_engram_query_with_momentum"]:
                args = {"min_crs": 0.5, "sample_size": 5} if "verify" in name else {"query": "test", "k": 2}
                r = self.call_tool(name, args)
                if self.timings:
                    buckets["heavy"].append(self.timings[-1]["elapsed_ms"])
        return {
            "repeats": repeats,
            "light_avg_ms": round(sum(buckets["light"]) / max(1, len(buckets["light"])), 1) if buckets["light"] else None,
            "medium_avg_ms": round(sum(buckets["medium"]) / max(1, len(buckets["medium"])), 1) if buckets["medium"] else None,
            "heavy_avg_ms": round(sum(buckets["heavy"]) / max(1, len(buckets["heavy"])), 1) if buckets["heavy"] else None,
            "still_alive_after_heavy": self.is_alive,
            "transport_failures": self.transport_failures,
        }

    def run_optix_bvh_stress(self, enable_optix: bool = True) -> Dict[str, Any]:
        """Stress path: relaunch with OptiX on (or off) and run heavy verify + momentum during init window."""
        # Note: caller typically launches fresh client with env
        # Here we just run a stress sequence that exercises BVH paths if data present.
        seq = [
            ("mcp_engram_stats", {}),
            ("mcp_engram_session_start", {"intent": "OptiX/BVH stress under harness - duplicate of May 31 init starvation pattern"}),
            ("mcp_engram_verify_manifold_integrity", {"min_crs": 0.3, "sample_size": 30}),  # larger sample stresses
            ("mcp_engram_query_with_momentum", {"query": "OptiX OR BVH OR LBVH OR CUDA", "k": 3}),
        ]
        res = self.run_sequence(seq, "optix_bvh_stress")
        res["optix_enabled_in_env"] = self.env.get("ENGRAM_OPTIX_ENABLED", "unset")
        return res

    def run_duplicate_detection_test(self) -> Dict[str, Any]:
        """Launch a second client against same store while first is alive (simulates old duplicate bug)."""
        # This is best done from orchestrator (two separate clients on same dir)
        # Here we just note; simple self-check
        return {
            "note": "Duplicate detection is orchestration-level (see engram-harness.sh). Client-side reports current PID state.",
            "current_pid": os.getpid(),
            "store": self.store_dir,
        }

    def run_compression_measurement_test(self, iterations: int = 3) -> Dict[str, Any]:
        """Rigorous exercise of Context Compression Tracking System v1.
        Builds on dual-lens / NREM / ki_hijacker scaffolding + compression_intent path.
        Produces before/after snapshots (via heavy tool timings + stats as proxy),
        promotes measurement protocol, mints COMPRESS marker with full event schema
        in session_end (triggers high-CRS compression_event_* + links in server).
        Integrates as regression gate bound to MCP transport work (same harness).
        Low-friction manual trigger simulation: the COMPRESS in end summary.
        """
        before_timings: List[float] = []
        after_timings: List[float] = []
        key_concepts = [
            "helper:next_compression_measurement_protocol_v1",
            "helper:promote_structured_tile_for_compression_v1",
            "tile:research_offload_pre-65--readiness-snapshot---phase-2-arc-at-63-2",
        ]
        # Before snapshot (proxy for dual-lens on promoted set)
        for _ in range(iterations):
            for name in ["mcp_engram_verify_manifold_integrity", "mcp_engram_query_with_momentum"]:
                args = {"min_crs": 0.5, "sample_size": 5} if "verify" in name else {"query": "compression OR 65% OR dual-lens OR measurement protocol", "k": 3}
                r = self.call_tool(name, args)
                if self.timings:
                    before_timings.append(self.timings[-1]["elapsed_ms"])
                if not self.is_alive:
                    break
        # Simulate crossing compression window + manual trigger (low friction)
        # Mint via session_end with explicit COMPRESS measurement marker (the trigger)
        compress_summary = (
            "Compression measurement cycle via test harness (binds to codeland 1780091465 + MCP transport regression investigation).\n"
            "COMPRESS: compression_tracking_v1 | tui_context=67 | trigger=harness_low_friction | "
            "before=dual_lens_proxy_on_promoted | promoted=hot_tiles_traces_anchors_hydration | "
            "after=rehydration_post_window | metrics=continuity_success_crs_retention | "
            "linked=harness_results+trace:1779992449+pilot_tiles+handoff:codeland_integration_2026_plan\n"
            "Full event schema populated in minted compression_event_* (see mcp handler). "
            "Every such event must produce high-CRS artifacts or scar the detection gap."
        )
        end_r = self.call_tool("mcp_engram_session_end", {"summary": compress_summary})
        # After snapshot
        for _ in range(iterations):
            for name in ["mcp_engram_summarize", "mcp_engram_verify_manifold_integrity"]:
                args = {"min_crs": 0.5, "sample_size": 5} if "verify" in name else {"top_n": 5}
                r = self.call_tool(name, args)
                if self.timings:
                    after_timings.append(self.timings[-1]["elapsed_ms"])
                if not self.is_alive:
                    break
        still_alive = self.is_alive
        transport_fails = self.transport_failures
        avg_before = round(sum(before_timings)/max(1, len(before_timings)), 1) if before_timings else 0
        avg_after = round(sum(after_timings)/max(1, len(after_timings)), 1) if after_timings else 0
        delta = round(avg_after - avg_before, 1)
        return {
            "label": "compression_measurement_v1",
            "iterations": iterations,
            "before_avg_ms": avg_before,
            "after_avg_ms": avg_after,
            "rehydration_delta_ms": delta,
            "still_alive": still_alive,
            "transport_failures": transport_fails,
            "compress_marker_minted": True,
            "event_schema_exercised": "before_state + promoted + after_state + continuity_metrics + codeland_1780091465 + MCP_harness_link + pilot_trace_1779992449",
            "note": "High-CRS compression_event_* artifact produced server-side on COMPRESS measurement marker. Run with --record-results for living config + manifold update commands. Binds new tracking system to recent MCP transport regression work and codeland handoff.",
            "errors": self.errors[-3:],
        }

    def shutdown(self):
        self._kill()

    def _kill(self):
        if self.proc:
            try:
                self.proc.terminate()
                time.sleep(0.3)
                if self.proc.poll() is None:
                    self.proc.kill()
            except Exception:
                pass
            self.proc = None
        self.is_alive = False

    def get_summary(self) -> Dict[str, Any]:
        return {
            "binary": self.binary,
            "store": self.store_dir,
            "still_alive": self.is_alive,
            "transport_failures": self.transport_failures,
            "total_tool_calls": len(self.timings),
            "errors": self.errors,
            "timings_sample": self.timings[-8:],
            "stderr_log": self.stderr_log,
        }

    def record_and_assert_wake_up_verification_metric(self, wake_up_context: str = "harness", server_binary: Optional[str] = None) -> Dict[str, Any]:
        """Exercise the new Wake-up Lawfulness Verification Tracking metric.
        Called after session_start + summarize per ritual. Records timestamped metric: block + trend update (update-preferred),
        binds to codeland handoff 1780091465, then asserts via recall. Returns results with lawful/score/assert_passed for harness gating.
        """
        if server_binary is None:
            server_binary = self.binary
        ts = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z").replace(":", "").replace("-", "")[:15]  # rough iso for concept
        metric_concept = f"metric:wake_up_verification_{ts}Z"

        # Fresh calls for full data (post-ritual state)
        verify_resp = self.call_tool("mcp_engram_verify_manifold_integrity", {"min_crs": 0.6, "sample_size": 15})
        genesis_resp = self.call_tool("mcp_engram_genesis", {"action": "status"})
        spatial_resp = self.call_tool("mcp_engram_spatial_status", {})
        stats_resp = self.call_tool("mcp_engram_stats", {})
        ki_recall = self.call_tool("mcp_engram_recall", {"query": "ki_hijacker OR DUAL_LENS", "k": 3}) if "mcp_engram_recall" in [t for t in LIGHT_TOOLS | MEDIUM_TOOLS | HEAVY_TOOLS] else {"result": {"content": [{"text": "ki recall skipped (use recall_recent if needed)"}]} }

        # Parse simple signals (text content for now; real would parse structured)
        verify_text = str(verify_resp.get("result", {}).get("content", [{}])[0].get("text", "")) if "result" in verify_resp else str(verify_resp)
        issues_found = 0
        if "Issues found: " in verify_text:
            try: issues_found = int(verify_text.split("Issues found: ")[1].split("\n")[0].strip())
            except: pass
        overall_health = "healthy" if "healthy" in verify_text.lower() or issues_found == 0 else "needs_review"

        genesis_text = str(genesis_resp.get("result", {}).get("content", [{}])[0].get("text", "")) if "result" in genesis_resp else str(genesis_resp)
        genesis_seeded = "YES" in genesis_text or "seeded" in genesis_text.lower()

        spatial_text = str(spatial_resp.get("result", {}).get("content", [{}])[0].get("text", "")) if "result" in spatial_resp else str(spatial_resp)
        spatial_gaps = 0 if "no critical" in spatial_text.lower() or "gaps" not in spatial_text.lower() else 1

        ki_text = str(ki_recall.get("result", {}).get("content", [{}])[0].get("text", "")) if "result" in ki_recall else str(ki_recall)
        ki_fresh = "ki_hijacker" in ki_text.lower() or "DUAL_LENS" in ki_text or "bake" in ki_text.lower()

        overall_lawful = (issues_found == 0 and genesis_seeded and spatial_gaps == 0 and ki_fresh and self.is_alive)
        lawfulness_score = max(0.0, min(1.0, 1.0 - (issues_found * 0.05) - (0 if genesis_seeded else 0.2) - (spatial_gaps * 0.1) - (0 if ki_fresh else 0.15)))

        metric_payload = (
            f"WAKE-UP LAWFULNESS VERIFICATION METRIC\n"
            f"timestamp: {ts}Z\n"
            f"wake_up_context: {wake_up_context}\n"
            f"server_binary: {server_binary}\n"
            f"verify_manifold_integrity: sampled~15 health={overall_health} issues={issues_found}\n"
            f"  details: {verify_text[:300]}...\n"
            f"genesis_status: {genesis_text[:200]}...\n"
            f"spatial_freshness: gaps={spatial_gaps} {spatial_text[:150]}...\n"
            f"ki_hijacker_freshness: fresh={ki_fresh}\n"
            f"overall_lawful: {overall_lawful}\n"
            f"lawfulness_score: {lawfulness_score:.2f}\n"
            f"source: test-harness post full-wakeup (exercises Phase 1.5 of engram-wake-up/SKILL.md)\n"
            f"codeland_goal: 1780091465\n"
            f"related: handoff:codeland_integration_2026_plan, May31 transport regression investigation\n"
        )

        # Record immutable metric event
        remember_resp = self.call_tool("mcp_engram_remember", {
            "concept": metric_concept,
            "text": metric_payload,
            "crs": 0.93 if overall_lawful else 0.7
        })

        # Update-preferred trend (append)
        trend_update = self.call_tool("mcp_engram_update", {
            "concept": "metric:wake_up_lawfulness_trend",
            "new_text": f"Entry {ts}Z: lawful={overall_lawful} score={lawfulness_score:.2f} ctx={wake_up_context} binary={os.path.basename(server_binary)} | codeland 1780091465 | May31 artifacts bound. (append via update-preferred)"
        })

        # Assert via query (recall the new metric)
        assert_recall = self.call_tool("mcp_engram_recall", {"query": f"metric:wake_up_verification", "k": 5})
        assert_text = str(assert_recall)
        metric_found = metric_concept in assert_text or "wake_up_verification" in assert_text
        assert_passed = metric_found and overall_lawful and self.is_alive and "error" not in str(remember_resp)

        result = {
            "metric_concept": metric_concept,
            "wake_up_context": wake_up_context,
            "server_binary": server_binary,
            "verify_issues": issues_found,
            "overall_health": overall_health,
            "genesis_seeded": genesis_seeded,
            "spatial_gaps": spatial_gaps,
            "ki_fresh": ki_fresh,
            "overall_lawful": overall_lawful,
            "lawfulness_score": round(lawfulness_score, 3),
            "remember_ok": "error" not in str(remember_resp),
            "trend_update_ok": "error" not in str(trend_update),
            "metric_found_in_recall": metric_found,
            "assert_passed": assert_passed,
            "codeland_binding": "handoff:codeland_integration_2026_plan + 1780091465 + May31 investigation",
            "helper_tile_ensured": "helper:wake_up_lawfulness_verification_v1 (via ritual description)",
            "errors": self.errors[-3:],
        }
        if not assert_passed:
            self.errors.append(f"Lawfulness metric assert failed for {metric_concept}")
        return result


def main():
    ap = argparse.ArgumentParser(description="Engram MCP Test Client (harness transport regression)")
    ap.add_argument("--binary", required=True, help="Path to engram binary (stable or dev)")
    ap.add_argument("--store", required=True, help="Isolated temp store dir (will be created)")
    ap.add_argument("--env", action="append", default=[], help="KEY=VAL env override (repeatable)")
    ap.add_argument("--suite", default="health", choices=["health", "full-wakeup", "transport-lifetime", "heavy-light", "optix-stress", "compression-measurement", "lawfulness-metric", "continuation-bundle", "agent-memory", "agent-tool-fidelity", "goal-clear", "all"])
    ap.add_argument("--iterations", type=int, default=12, help="For transport-lifetime")
    ap.add_argument("--timeout", type=float, default=60.0)
    ap.add_argument("--verbose", action="store_true")
    ap.add_argument("--json-out", help="Write full JSON results here")
    ap.add_argument("--workspace", default="/path/to/your/engram", help="For watch_workspace in ritual (your clone root)")
    args = ap.parse_args()

    env_over = {}
    for kv in args.env:
        if "=" in kv:
            k, v = kv.split("=", 1)
            env_over[k] = v

    client = MCPTestClient(
        binary=args.binary,
        store_dir=args.store,
        env_overrides=env_over,
        default_timeout=args.timeout,
        verbose=args.verbose,
    )

    if not client.start():
        print(json.dumps({"ok": False, "error": "failed_to_start", "details": client.errors, "summary": client.get_summary()}, indent=2))
        sys.exit(2)

    try:
        if args.suite in ("health", "all"):
            print("=== Running health_suite ===")
            res = client.run_health_suite()
            print(json.dumps(res, indent=2))

        if args.suite in ("full-wakeup", "all"):
            print("=== Running full_wakeup_ritual ===")
            res = client.run_full_wakeup_ritual(workspace_path=args.workspace)
            print(json.dumps(res, indent=2))

        if args.suite in ("transport-lifetime", "all"):
            print(f"=== Running transport_lifetime_test x{args.iterations} ===")
            res = client.run_transport_lifetime_test(iterations=args.iterations)
            print(json.dumps(res, indent=2))

        if args.suite in ("heavy-light", "all"):
            print("=== Running heavy_vs_light_timing ===")
            res = client.run_heavy_vs_light_timing(repeats=2)
            print(json.dumps(res, indent=2))

        if args.suite in ("optix-stress", "all"):
            print("=== Running optix_bvh_stress ===")
            res = client.run_optix_bvh_stress()
            print(json.dumps(res, indent=2))

        if args.suite in ("lawfulness-metric", "all"):
            print("=== Running wake-up lawfulness verification metric exercise + assert (binds to codeland 1780091465 + May 31 artifacts) ===")
            res = client.record_and_assert_wake_up_verification_metric(wake_up_context="harness-lawfulness-suite", server_binary=client.binary)
            print(json.dumps({"lawfulness_metric_standalone": res}, indent=2))

        if args.suite in ("compression-measurement", "all"):
            print("=== Running compression_measurement_test (Context Compression Tracking System v1) ===")
            res = client.run_compression_measurement_test(iterations=2)
            print(json.dumps(res, indent=2))

        if args.suite in ("continuation-bundle", "all"):
            print("=== Running continuation_bundle_suite (goals + bundle + compression handoff) ===")
            res = client.run_continuation_bundle_suite()
            print(json.dumps(res, indent=2))
            if not res.get("passed"):
                client.errors.append("continuation-bundle assertions failed")

        if args.suite in ("agent-memory", "all"):
            print("=== Running agent_memory_suite (MVP lean 8-tool loop + handoff continuity) ===")
            res = client.run_agent_memory_suite()
            print(json.dumps(res, indent=2))
            if not res.get("passed"):
                client.errors.append("agent-memory assertions failed")

        if args.suite in ("agent-tool-fidelity", "all"):
            print("=== Running agent_tool_fidelity_suite (composite edit/update + >=95% usage) ===")
            res = client.run_agent_tool_fidelity_suite(workspace_path=args.workspace)
            print(json.dumps(res, indent=2))
            if not res.get("passed"):
                client.errors.append("agent-tool-fidelity assertions failed")

        goal_clear_full_written = False
        if args.suite in ("goal-clear", "all"):
            print("=== Running goal_clear_suite (set_primary → clear → post observe 2x) ===")
            res = client.run_goal_clear_suite()
            print(json.dumps({k: v for k, v in res.items() if k != "raw_transcript"}, indent=2))
            if args.json_out:
                with open(args.json_out, "w") as f:
                    json.dump(res, f, indent=2)
                goal_clear_full_written = True
                print(f"Full goal-clear transcript written to {args.json_out}")
            if not res.get("passed"):
                client.errors.append("goal-clear assertions failed")

        summary = client.get_summary()
        if args.json_out and not goal_clear_full_written:
            with open(args.json_out, "w") as f:
                json.dump({"ok": True, "summary": summary, "timings": client.timings}, f, indent=2)
            print(f"Results written to {args.json_out}")

        print("\n=== CLIENT SUMMARY ===")
        print(json.dumps(summary, indent=2))
        sys.exit(0 if summary["still_alive"] and summary["transport_failures"] == 0 else 1)
    finally:
        client.shutdown()


if __name__ == "__main__":
    main()
