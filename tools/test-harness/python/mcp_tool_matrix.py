#!/usr/bin/env python3
"""
Full MCP tool smoke matrix for Engram.
Exercises every tool registered via tools/list with minimal valid arguments
against an isolated temp store. Reports pass / soft_fail / hard_fail / skip.

Usage:
  python3 mcp_tool_matrix.py --binary target/debug/engram --store /tmp/engram-tool-matrix-$$
  python3 mcp_tool_matrix.py --binary ... --store ... --json-out results/tool_matrix.json
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional, Tuple

# Reuse harness client
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mcp_test_client import MCPTestClient  # noqa: E402

WORKSPACE = os.environ.get("ENGRAM_MATRIX_WORKSPACE", "/home/a/Documents/Engram")
LING_BUNDLE = {
    "words": [{"text": "matrix", "coeff": [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]}],
    "patches": [],
    "functor_metadata": {"source": "tool_matrix"},
}

# External-dependency tools: still attempted unless --skip-external
EXTERNAL_TOOLS = {
    "mcp_engram_scout": "web search + optional LLM synthesis (ENGRAM_SCOUT_LLM_URL)",
}

# Known environment limitations in isolated harness (handler works; preconditions missing)
ENV_LIMIT_TOOLS = {
    "mcp_engram_rebuild_bvh": "CPU-only harness — BVH thread spawn blocked",
    "mcp_engram_set_namespace": "isolated store has ENGRAM_DISABLE_SHEAF=1 — namespaces need sheaf.toml",
}

# Tools where intentional missing fixture should count as handler-ok
EXPECTED_MISS_TOOLS = {
    "mcp_engram_invoke_protocol",
    "mcp_engram_verify_behavior",
}

# Dependency-aware order (spatial/goal-heavy tools after seed + ingest)
TOOL_ORDER = [
    "mcp_engram_get_backend_readiness",
    "mcp_engram_stats",
    "mcp_engram_list_namespaces",
    "mcp_engram_list_concepts",
    "mcp_engram_read_concept",
    "mcp_engram_recall",
    "mcp_engram_recall_recent",
    "mcp_engram_query_pure",
    "mcp_engram_query_with_momentum",
    "mcp_engram_summarize",
    "mcp_engram_get_continuation_bundle",
    "mcp_engram_session_start",
    "mcp_engram_ack_wake_queue",
    "mcp_engram_set_memory_mode",
    "mcp_engram_watch_workspace",
    "mcp_engram_force_spatial_ingest",
    "mcp_engram_incremental_spatial_ingest",
    "mcp_engram_spatial_status",
    "mcp_engram_context_for_file",
    "mcp_engram_context_for_edit",
    "mcp_engram_recall_in_file",
    "mcp_engram_remember",
    "mcp_engram_batch_remember",
    "mcp_engram_update",
    "mcp_engram_pin",
    "mcp_engram_promote_hot",
    "mcp_engram_promote_hot_batch",
    "mcp_engram_relate",
    "mcp_engram_relate_batch",
    "mcp_engram_search_by_relation",
    "mcp_engram_visualize",
    "mcp_engram_quick_trace",
    "mcp_engram_record_reasoning_trace",
    "mcp_engram_goal_create",
    "mcp_engram_goal_list",
    "mcp_engram_goal_status",
    "mcp_engram_goal_search",
    "mcp_engram_goal_get_children",
    "mcp_engram_goal_decompose",
    "mcp_engram_goal_update_status",
    "mcp_engram_goal_set_primary",
    "mcp_engram_demote_from_context",
    "mcp_engram_thought_tile_draft_from_chain",
    "mcp_engram_thought_tile_create",
    "mcp_engram_thought_tile_write_result",
    "mcp_engram_thought_tile_create_visualization",
    "mcp_engram_process_metrics",
    "mcp_engram_remember_solution",
    "mcp_engram_scar",
    "mcp_engram_forget",
    "mcp_engram_forget_old",
    "mcp_engram_export",
    "mcp_engram_import",
    "mcp_engram_genesis",
    "mcp_engram_verify_manifold_integrity",
    "mcp_engram_verify_block_lawfulness",
    "mcp_engram_verify_behavior",
    "mcp_engram_invoke_protocol",
    "mcp_engram_track_user",
    "mcp_engram_set_geosphere_frame",
    "mcp_engram_get_geosphere_frame",
    "mcp_engram_clear_geosphere_frame",
    "mcp_engram_rebuild_bvh",
    "mcp_engram_set_namespace",
    "mcp_engram_scout",
    "mcp_compress_linguistic",
    "mcp_decompress_linguistic",
    "mcp_fibered_linguistic_equivalence",
    "mcp_linguistic_calculus",
    "mcp_engram_session_end",
]


def _text(resp: Dict[str, Any]) -> str:
    try:
        content = resp.get("result", {}).get("content", [])
        if content and isinstance(content[0], dict):
            return content[0].get("text", "") or ""
    except (TypeError, KeyError, IndexError):
        pass
    return ""


def _is_error_flag(resp: Dict[str, Any]) -> bool:
    if "error" in resp:
        return True
    result = resp.get("result") or {}
    if result.get("isError"):
        return True
    text = _text(resp).strip()
    if not text:
        return False
    low = text.lower()
    if low.startswith(("exported ", "✓", "ok ", "success")):
        return False
    if low.startswith(("error:", "scar failed", "invocation error")):
        return True
    first = low.split("\n", 1)[0]
    fatal = (
        "unknown tool",
        "transport closed",
        "internal server error",
    )
    return any(p in first for p in fatal)


def _extract_concept(text: str, prefix: str) -> Optional[str]:
    for line in text.splitlines():
        if prefix in line:
            m = re.search(rf"({re.escape(prefix)}[:\w.-]+)", line)
            if m:
                return m.group(1)
    m = re.search(rf"({re.escape(prefix)}[:\w.-]+)", text)
    return m.group(1) if m else None


def seed_store(client: MCPTestClient, state: Dict[str, Any]) -> List[str]:
    """Bootstrap fixtures; return seed errors."""
    errors: List[str] = []
    ts = int(time.time())
    state["concept_a"] = f"matrix:test_a_{ts}"
    state["concept_b"] = f"matrix:test_b_{ts}"
    state["goal_id"] = f"matrix_test_goal_{ts}"
    state["disposable"] = f"matrix:disposable_{ts}"

    ws = state.get("_workspace", WORKSPACE)
    steps: List[Tuple[str, Dict[str, Any]]] = [
        ("mcp_engram_session_start", {"intent": f"Tool matrix seed {ts} — isolated smoke test"}),
        ("mcp_engram_goal_create", {
            "statement": "Tool matrix harness goal",
            "goal_id": state["goal_id"],
            "priority": "medium",
        }),
        ("mcp_engram_goal_set_primary", {"goal": f"goal:{state['goal_id']}"}),
        ("mcp_engram_remember", {
            "concept": state["concept_a"],
            "text": "Matrix fixture concept A for relate/recall tests.",
        }),
        ("mcp_engram_remember", {
            "concept": state["concept_b"],
            "text": "Matrix fixture concept B for batch/relation tests.",
        }),
        ("mcp_engram_quick_trace", {
            "decision": "Seed trace for matrix chain tests",
            "why": "thought_tile_draft_from_chain needs trace:* under goal",
            "context": "tools/test-harness/python/mcp_tool_matrix.py",
        }),
        ("mcp_engram_watch_workspace", {"path": ws}),
    ]
    for name, args in steps:
        resp = client.call_tool(name, args, timeout=90.0)
        if "error" in resp:
            errors.append(f"seed {name}: {resp['error']}")
        if name == "mcp_engram_quick_trace":
            tid = _extract_concept(_text(resp), "trace:")
            if tid:
                state["trace_id"] = tid

    # Tile for write_result / visualization companions
    tile_resp = client.call_tool(
        "mcp_engram_thought_tile_create",
        {
            "tile_type": "tabular",
            "title": f"Matrix tile {ts}",
            "payload": {"rows": [{"col": "seed"}], "schema": "matrix_v0"},
            "goal_context": f"goal:{state['goal_id']}",
        },
        timeout=60.0,
    )
    tid = _extract_concept(_text(tile_resp), "tile:")
    if tid:
        state["tile_id"] = tid
    elif "error" in tile_resp:
        errors.append(f"seed thought_tile_create: {tile_resp['error']}")

    rel = client.call_tool(
        "mcp_engram_relate",
        {"from": state["concept_a"], "to": state["concept_b"], "relation": "matrix_test"},
        timeout=30.0,
    )
    if "error" in rel:
        errors.append(f"seed relate: {rel['error']}")

    sec_goal = f"goal:matrix_secondary_{state['goal_id']}"
    state["secondary_goal"] = sec_goal
    sg = client.call_tool(
        "mcp_engram_goal_create",
        {
            "statement": "Secondary matrix goal for demote probe",
            "goal_id": f"matrix_secondary_{state['goal_id']}",
            "priority": "low",
        },
        timeout=30.0,
    )
    if "error" in sg:
        errors.append(f"seed secondary goal_create: {sg['error']}")

    scar_target = f"matrix:scar_target_{state['goal_id']}"
    state["scar_target"] = scar_target
    sc = client.call_tool(
        "mcp_engram_remember",
        {"concept": scar_target, "text": "Disposable target for scar probe."},
        timeout=30.0,
    )
    if "error" in sc:
        errors.append(f"seed scar target: {sc['error']}")

    # Directory ingest required: single-file force_ingest_path returns before item1.5 state mint
    client.call_tool(
        "mcp_engram_force_spatial_ingest",
        {
            "paths": [os.path.join(ws, "crates/engram-server/src")],
            "recursive": False,
        },
        timeout=180.0,
    )
    client.call_tool(
        "mcp_engram_incremental_spatial_ingest",
        {"max_files": 5},
        timeout=120.0,
    )

    return errors


def build_tool_args(state: Dict[str, Any]) -> Dict[str, Dict[str, Any]]:
    ca, cb = state["concept_a"], state["concept_b"]
    goal = f"goal:{state['goal_id']}"
    trace = state.get("trace_id", "trace:matrix_placeholder")
    tile = state.get("tile_id", "tile:matrix_placeholder")
    ws = state.get("_workspace", WORKSPACE)
    profile_rs = os.path.join(ws, "crates/engram-server/src/profile.rs")
    export_json = json.dumps([{"concept": "matrix:imported", "text": "imported via matrix"}])

    return {
        "mcp_engram_read_concept": {"concept": ca},
        "mcp_compress_linguistic": {"bundle": LING_BUNDLE},
        "mcp_decompress_linguistic": {"bundle": LING_BUNDLE},
        "mcp_fibered_linguistic_equivalence": {
            "bundle_a": LING_BUNDLE,
            "bundle_b": LING_BUNDLE,
        },
        "mcp_linguistic_calculus": {"bundle": LING_BUNDLE, "operation": "differentiate"},
        "mcp_engram_remember": {
            "concept": state["disposable"],
            "text": "Disposable concept for forget/pin tests.",
        },
        "mcp_engram_recall": {"query": "matrix fixture", "k": 3, "scope": "all"},
        "mcp_engram_forget": {"concept": state["disposable"]},
        "mcp_engram_list_concepts": {"prefix": "matrix:", "limit": 10},
        "mcp_engram_watch_workspace": {"path": ws},
        "mcp_engram_force_spatial_ingest": {
            "paths": [os.path.join(ws, "crates/engram-server/src")],
            "recursive": False,
        },
        "mcp_engram_spatial_status": {},
        "mcp_engram_ack_wake_queue": {"executed": True},
        "mcp_engram_session_start": {"intent": "Matrix per-tool probe (non-seed)"},
        "mcp_engram_session_end": {
            "summary": "Matrix probe session_end — no compression required.",
            "prepare_compression": False,
        },
        "mcp_engram_get_continuation_bundle": {},
        "mcp_engram_query_pure": {"intent": "matrix fixture concepts", "k": 3},
        "mcp_engram_incremental_spatial_ingest": {"max_files": 3},
        "mcp_engram_promote_hot_batch": {"concepts": [ca, cb]},
        "mcp_engram_relate_batch": {
            "relations": [{"from": ca, "to": cb, "relation": "matrix_batch"}],
        },
        "mcp_engram_record_reasoning_trace": {
            "decision_point": "Matrix record_reasoning_trace probe",
            "justification": "Validates full trace mint path",
            "goal_context": goal,
            "prev_trace": trace if trace != "trace:matrix_placeholder" else None,
        },
        "mcp_engram_quick_trace": {
            "decision": "Matrix quick_trace probe",
            "why": "Second trace in chain",
            "prev": trace if trace != "trace:matrix_placeholder" else None,
        },
        "mcp_engram_thought_tile_draft_from_chain": {"goal_context": goal},
        "mcp_engram_process_metrics": {
            "process_key": "process:engram.harness.sub-agent-launch",
        },
        "mcp_engram_goal_create": {
            "statement": "Secondary matrix goal",
            "goal_id": f"matrix_secondary_{state['goal_id']}",
            "priority": "low",
        },
        "mcp_engram_goal_update_status": {"goal": goal, "status": "active"},
        "mcp_engram_demote_from_context": {
            "concept": state.get("secondary_goal", f"goal:matrix_secondary_{state['goal_id']}"),
            "note": "matrix demote probe",
        },
        "mcp_engram_goal_status": {"goal": goal},
        "mcp_engram_goal_decompose": {
            "parent": goal,
            "statements": ["matrix subgoal 1"],
        },
        "mcp_engram_goal_search": {"query": "matrix", "limit": 5},
        "mcp_engram_goal_get_children": {"parent": goal},
        "mcp_engram_goal_set_primary": {"goal": goal},
        "mcp_engram_goal_list": {"limit": 10},
        "mcp_engram_thought_tile_create": {
            "tile_type": "research_offload",
            "title": "Matrix second tile",
            "payload": {"findings": ["probe"]},
            "goal_context": goal,
        },
        "mcp_engram_thought_tile_create_visualization": {
            "title": "Matrix viz",
            "payload": "<html><body>matrix</body></html>",
            "goal_context": goal,
        },
        "mcp_engram_promote_hot": {"concept": ca},
        "mcp_engram_thought_tile_write_result": {
            "tile": tile,
            "result_payload": {"matrix": "ok"},
            "status": "completed",
        },
        "mcp_engram_pin": {"concept": ca},
        "mcp_engram_relate": {"from": cb, "to": ca, "relation": "matrix_reverse"},
        "mcp_engram_context_for_file": {"path": profile_rs},
        "mcp_engram_context_for_edit": {"path": profile_rs, "auto_ingest": False},
        "mcp_engram_remember_solution": {
            "error_pattern": "matrix test error pattern",
            "solution": "matrix test solution",
        },
        "mcp_engram_stats": {},
        "mcp_engram_recall_recent": {"n": 5},
        "mcp_engram_set_namespace": {"namespace": "matrix_ns"},
        "mcp_engram_list_namespaces": {},
        "mcp_engram_update": {
            "concept": ca,
            "new_text": "Updated by matrix — still fixture A.",
        },
        "mcp_engram_get_backend_readiness": {},
        "mcp_engram_set_memory_mode": {"mode": "lean"},
        "mcp_engram_rebuild_bvh": {},
        "mcp_engram_summarize": {"top_n": 5},
        "mcp_engram_batch_remember": {
            "entries": [{"concept": f"matrix:batch_{state['goal_id']}", "text": "batch item"}],
        },
        "mcp_engram_export": {"min_crs": 0.0},
        "mcp_engram_import": {"json": export_json},
        "mcp_engram_forget_old": {"min_crs_threshold": 0.01, "older_than_days": 3650},
        "mcp_engram_search_by_relation": {"concept": ca, "direction": "both", "k": 3},
        "mcp_engram_visualize": {"concept": ca, "depth": 2},
        "mcp_engram_genesis": {"action": "status"},
        "mcp_engram_scar": {
            "concept": state.get("scar_target", f"matrix:scar_target_{state['goal_id']}"),
            "reason": "matrix scar probe — ruled-out path",
        },
        "mcp_engram_recall_in_file": {"file_stem": "profile", "k": 5},
        "mcp_engram_query_with_momentum": {"query": "matrix fixture", "k": 3},
        "mcp_engram_verify_behavior": {
            "concept": "matrix:nonexistent_hypothesis",
            "success": True,
        },
        "mcp_engram_verify_block_lawfulness": {"concept": ca},
        "mcp_engram_verify_manifold_integrity": {"min_crs": 0.5, "sample_size": 5},
        "mcp_engram_invoke_protocol": {
            "key": "protocol:matrix_nonexistent",
            "dry_run": True,
        },
        "mcp_engram_track_user": {"interaction": "matrix harness probe — user prefers lean wake"},
        "mcp_engram_scout": {"query": "Engram geometric memory", "max_results": 2},
        "mcp_engram_set_geosphere_frame": {
            "origin": "giza_sacred_cubit",
            "time_offset": "matrix_probe",
        },
        "mcp_engram_get_geosphere_frame": {},
        "mcp_engram_clear_geosphere_frame": {},
    }


def _clean_args(args: Dict[str, Any]) -> Dict[str, Any]:
    return {k: v for k, v in args.items() if v is not None}


def classify(
    name: str,
    resp: Dict[str, Any],
    elapsed_ms: float,
    client: MCPTestClient,
    skip_external: bool,
) -> Dict[str, Any]:
    if skip_external and name in EXTERNAL_TOOLS:
        return {
            "tool": name,
            "status": "skip",
            "reason": EXTERNAL_TOOLS[name],
            "elapsed_ms": 0,
        }
    if not client.is_alive:
        return {
            "tool": name,
            "status": "hard_fail",
            "reason": "transport_dead",
            "elapsed_ms": elapsed_ms,
            "snippet": "",
        }
    if "error" in resp:
        err = resp["error"]
        msg = err.get("message", str(err)) if isinstance(err, dict) else str(err)
        if "timeout" in msg.lower():
            status = "hard_fail"
            reason = "timeout"
        else:
            status = "hard_fail"
            reason = f"jsonrpc_error: {msg[:120]}"
        return {
            "tool": name,
            "status": status,
            "reason": reason,
            "elapsed_ms": elapsed_ms,
            "snippet": msg[:200],
        }

    text = _text(resp)
    is_err = _is_error_flag(resp)
    snippet = (text or str(resp.get("result", "")))[:200].replace("\n", " ")

    if name in ENV_LIMIT_TOOLS and is_err:
        status = "env_limit"
        reason = ENV_LIMIT_TOOLS[name]
    elif name == "mcp_engram_goal_create" and "already exists" in text.lower():
        status = "pass"
        reason = "handler_ok_idempotent"
    elif name in EXPECTED_MISS_TOOLS and is_err and "not found" in text.lower():
        status = "pass"
        reason = "handler_ok_expected_miss"
    elif is_err:
        status = "soft_fail"
        reason = "isError_or_fatal_text"
    else:
        status = "pass"
        reason = "ok"

    if name in EXTERNAL_TOOLS and status not in ("pass",):
        status = "external_dep"
        reason = f"{EXTERNAL_TOOLS[name]} — {reason}"

    return {
        "tool": name,
        "status": status,
        "reason": reason,
        "elapsed_ms": round(elapsed_ms, 1),
        "snippet": snippet,
        "isError": bool(resp.get("result", {}).get("isError")),
    }


def run_matrix(
    client: MCPTestClient,
    skip_external: bool = False,
    per_tool_timeout: float = 90.0,
    workspace: Optional[str] = None,
) -> Dict[str, Any]:
    ws = workspace or WORKSPACE
    state: Dict[str, Any] = {"_workspace": ws}
    seed_errors = seed_store(client, state)
    if not client.wait_for_fully_initialized(max_wait=90.0):
        seed_errors.append("backend never reached fully_initialized")

    tools_resp = client._send_request("tools/list", {}, timeout=30.0)
    listed = []
    if "result" in tools_resp:
        listed = sorted(t.get("name", "") for t in tools_resp["result"].get("tools", []))

    arg_map = build_tool_args(state)
    results: List[Dict[str, Any]] = []
    missing_args: List[str] = []

    ordered = [t for t in TOOL_ORDER if t in listed]
    ordered += sorted(t for t in listed if t not in ordered)

    for name in ordered:
        if name not in arg_map:
            missing_args.append(name)
            results.append({
                "tool": name,
                "status": "skip",
                "reason": "no_fixture_args_in_matrix",
                "elapsed_ms": 0,
            })
            continue

        args = _clean_args(arg_map[name])
        t0 = time.time()
        timeout = per_tool_timeout
        if name in (
            "mcp_engram_verify_manifold_integrity",
            "mcp_engram_query_with_momentum",
            "mcp_engram_rebuild_bvh",
            "mcp_engram_force_spatial_ingest",
            "mcp_engram_session_start",
        ):
            timeout = max(per_tool_timeout, 120.0)
        if name == "mcp_engram_scout":
            timeout = 45.0

        resp = client.call_tool(name, args, timeout=timeout)
        elapsed = (time.time() - t0) * 1000
        results.append(classify(name, resp, elapsed, client, skip_external))

        if name in (
            "mcp_engram_record_reasoning_trace",
            "mcp_engram_quick_trace",
        ):
            tid = _extract_concept(_text(resp), "trace:")
            if tid:
                state["trace_id"] = tid
        if name == "mcp_engram_thought_tile_create":
            tid = _extract_concept(_text(resp), "tile:")
            if tid:
                state["tile_id"] = tid

        if name in (
            "mcp_engram_verify_manifold_integrity",
            "mcp_engram_rebuild_bvh",
        ):
            time.sleep(0.1)

    counts = {"pass": 0, "soft_fail": 0, "hard_fail": 0, "skip": 0, "external_dep": 0, "env_limit": 0}
    for r in results:
        counts[r["status"]] = counts.get(r["status"], 0) + 1

    return {
        "timestamp": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "binary": client.binary,
        "store": client.store_dir,
        "workspace": ws,
        "tools_listed": len(listed),
        "tools_tested": len(results),
        "counts": counts,
        "seed_errors": seed_errors,
        "missing_fixture_args": missing_args,
        "state": {k: v for k, v in state.items() if not k.startswith("_")},
        "still_alive": client.is_alive,
        "transport_failures": client.transport_failures,
        "results": results,
    }


def print_summary(report: Dict[str, Any]) -> None:
    c = report["counts"]
    print(f"\n=== TOOL MATRIX SUMMARY ===")
    print(f"Listed: {report['tools_listed']}  Tested: {report['tools_tested']}")
    print(
        f"pass={c.get('pass',0)} soft_fail={c.get('soft_fail',0)} "
        f"hard_fail={c.get('hard_fail',0)} env_limit={c.get('env_limit',0)} "
        f"external_dep={c.get('external_dep',0)} skip={c.get('skip',0)}"
    )
    print(f"Transport alive: {report['still_alive']}  failures: {report['transport_failures']}")
    if report.get("seed_errors"):
        print("Seed errors:", report["seed_errors"])

    print("\n--- FAILURES ---")
    for r in report["results"]:
        if r["status"] in ("hard_fail", "soft_fail", "external_dep", "env_limit"):
            print(f"  [{r['status']}] {r['tool']}: {r.get('reason','')} | {r.get('snippet','')[:100]}")

    print("\n--- ALL TOOLS ---")
    for r in report["results"]:
        mark = {
            "pass": "OK",
            "soft_fail": "SOFT",
            "hard_fail": "FAIL",
            "skip": "SKIP",
            "external_dep": "EXT",
            "env_limit": "ENV",
        }[r["status"]]
        ms = r.get("elapsed_ms", 0)
        print(f"  {mark:4} {r['tool']:45} {ms:7.0f}ms  {r.get('snippet','')[:60]}")


def main() -> None:
    ap = argparse.ArgumentParser(description="Engram full MCP tool smoke matrix")
    ap.add_argument("--binary", required=True)
    ap.add_argument("--store", required=True)
    ap.add_argument("--timeout", type=float, default=90.0)
    ap.add_argument("--skip-external", action="store_true")
    ap.add_argument("--json-out", help="Write JSON report")
    ap.add_argument("--workspace", default=WORKSPACE)
    args = ap.parse_args()

    client = MCPTestClient(
        binary=args.binary,
        store_dir=args.store,
        default_timeout=args.timeout,
    )
    if not client.start():
        print(json.dumps({"ok": False, "error": "failed_to_start", "details": client.errors}))
        sys.exit(2)

    try:
        report = run_matrix(
            client,
            skip_external=args.skip_external,
            per_tool_timeout=args.timeout,
            workspace=args.workspace,
        )
        report["ok"] = report["still_alive"] and report["counts"].get("hard_fail", 0) == 0
        print_summary(report)
        if args.json_out:
            os.makedirs(os.path.dirname(os.path.abspath(args.json_out)), exist_ok=True)
            with open(args.json_out, "w") as f:
                json.dump(report, f, indent=2)
            print(f"\nJSON written to {args.json_out}")
        sys.exit(0 if report["ok"] else 1)
    finally:
        client.shutdown()


if __name__ == "__main__":
    main()