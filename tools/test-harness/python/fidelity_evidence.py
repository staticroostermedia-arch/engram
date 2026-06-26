#!/usr/bin/env python3
"""Atomic evidence producer for agent_tool_fidelity_v1 — overwrite SCRATCH artifacts after 2 clean runs."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional


COMPOSITE_TOOLS = (
    "mcp_engram_safe_edit_and_verify",
    "mcp_engram_update_with_tensor_bond",
)

DIAGNOSIS_GREPS = [
    ("mcp_engram_safe_edit_and_verify", "crates/engram-server/src/mcp.rs"),
    ("mcp_engram_update_with_tensor_bond", "crates/engram-server/src/mcp.rs"),
    ("post_edit_palette", "crates/engram-server/src/harness_injection.rs"),
    ("fidelity_rituals", "crates/engram-server/src/harness_injection.rs"),
    ("ack_edit_arc_with_lineage", "crates/engram-server/src/edit_arc_gate.rs"),
    ("edit_pattern", "crates/engram-server/src/ki_hijacker.rs"),
    ("tensor_pattern_for_edit", "crates/engram-server/src/edit_fidelity.rs"),
    ("run_safe_edit_and_verify", "crates/engram-server/src/edit_fidelity.rs"),
    ("run_update_with_tensor_bond", "crates/engram-server/src/edit_fidelity.rs"),
]


def _utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def probe_fresh_mcp_composites(binary: str, workspace: str) -> Dict[str, Any]:
    """Isolated MCP probe — tools/list on fresh binary (no live-session skip)."""
    import tempfile

    tmpstore = tempfile.mkdtemp(prefix="engram-fidelity-probe-")
    init = json.dumps(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "fidelity-evidence", "version": "1"},
            },
        }
    )
    list_req = json.dumps({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
    try:
        proc = subprocess.run(
            [binary, "--store", tmpstore, "mcp"],
            input=f"{init}\n{list_req}\n",
            capture_output=True,
            text=True,
            timeout=30,
        )
        lines = [ln for ln in proc.stdout.splitlines() if ln.strip()]
        tools_line = lines[-1] if lines else ""
        parsed = json.loads(tools_line) if tools_line else {}
        tools = parsed.get("result", {}).get("tools", [])
        names = {t.get("name", "") for t in tools}
        composites = {n: (n in names) for n in COMPOSITE_TOOLS}
        few_shot = {}
        for t in tools:
            if t.get("name") in COMPOSITE_TOOLS:
                few_shot[t["name"]] = "FEW-SHOT" in (t.get("description") or "")
        return {
            "probe_at": _utc_now(),
            "binary": binary,
            "workspace": workspace,
            "tool_count": len(tools),
            "composites_registered": composites,
            "few_shot_present": few_shot,
            "all_composites_ok": all(composites.values()),
            "tools_list_tail": parsed,
        }
    except Exception as e:
        return {
            "probe_at": _utc_now(),
            "binary": binary,
            "error": str(e),
            "all_composites_ok": False,
        }
    finally:
        subprocess.run(["rm", "-rf", tmpstore], check=False)


def capture_diagnosis_grep(workspace: str) -> str:
    """grep -n excerpts from mandated source files (verif plan step 1)."""
    lines: List[str] = [f"# fidelity_diagnosis_source.txt — {_utc_now()}", f"# workspace: {workspace}", ""]
    for pattern, relpath in DIAGNOSIS_GREPS:
        path = os.path.join(workspace, relpath)
        lines.append(f"## grep -n '{pattern}' {relpath}")
        try:
            r = subprocess.run(
                ["grep", "-n", pattern, path],
                capture_output=True,
                text=True,
                timeout=10,
            )
            lines.append(r.stdout.strip() or "(no matches)")
        except Exception as e:
            lines.append(f"(grep failed: {e})")
        lines.append("")
    return "\n".join(lines)


def _parse_composite_lineage(responses: Dict[str, Any]) -> Dict[str, Any]:
    """Extract verif-plan fields: trace_id, lineage, tensor_pattern, scar, failure_pattern."""
    out: Dict[str, Any] = {}
    safe = responses.get("safe_edit_and_verify") or {}
    if safe:
        lin = safe.get("lineage") or {}
        tp = safe.get("tensor_pattern") or {}
        out["safe_edit_and_verify"] = {
            "trace_id": safe.get("trace_id"),
            "arc_concept": safe.get("arc_concept"),
            "arc_updated": safe.get("arc_updated"),
            "lineage_ok": lin.get("ok"),
            "lineage_merkle_ok": lin.get("merkle_ok"),
            "lineage_merkle_trace_sig": lin.get("merkle_trace_sig"),
            "lineage_merkle_arc_sig": lin.get("merkle_arc_sig"),
            "tensor_pattern": tp,
            "tensor_bonds_created": tp.get("bonds_created", tp.get("bonds", 0)),
            "full_response": safe,
        }
    arc_up = responses.get("update_with_tensor_bond_arc") or {}
    if arc_up:
        lin = arc_up.get("lineage") or {}
        tp = arc_up.get("tensor_pattern") or {}
        out["update_with_tensor_bond_arc"] = {
            "concept": arc_up.get("concept"),
            "ok": arc_up.get("ok"),
            "recall_match": arc_up.get("recall_match"),
            "crs_after": arc_up.get("crs_after"),
            "crs_gate_ok": arc_up.get("crs_gate_ok"),
            "lineage_merkle_arc_sig": lin.get("merkle_arc_sig"),
            "tensor_pattern": tp,
            "tensor_bonds_created": tp.get("bonds", 0),
            "full_response": arc_up,
        }
    misuse = responses.get("misuse_self_correction") or {}
    if misuse:
        out["misuse_self_correction"] = {
            "recall_match": misuse.get("recall_match"),
            "scar_key": misuse.get("scar_key"),
            "failure_pattern": misuse.get("failure_pattern"),
            "full_response": misuse,
        }
    return out


def sync_live_mcp_binary(workspace: str, binary: str, scratch_dir: str) -> Dict[str, Any]:
    """Rebuild + restart stale MCP so connected agents can wield composites."""
    script = os.path.join(workspace, "scripts", "sync-live-mcp-fidelity.sh")
    env = {**os.environ, "ENGRAM_BINARY": binary, "SCRATCH": scratch_dir, "FORCE_MCP_RESTART": "1"}
    try:
        proc = subprocess.run(
            ["bash", script],
            cwd=workspace,
            env=env,
            capture_output=True,
            text=True,
            timeout=180,
        )
        probe_path = os.path.join(scratch_dir, "live_mcp_probe.json")
        live_probe: Dict[str, Any] = {}
        if os.path.isfile(probe_path):
            with open(probe_path) as f:
                live_probe = json.load(f)
        return {
            "sync_exit_code": proc.returncode,
            "sync_stdout_tail": proc.stdout[-2000:] if proc.stdout else "",
            "sync_stderr_tail": proc.stderr[-1000:] if proc.stderr else "",
            "live_mcp_probe": live_probe,
            "sync_ok": proc.returncode == 0 and live_probe.get("all_ok", False),
        }
    except Exception as e:
        return {"sync_ok": False, "error": str(e)}


def write_fidelity_evidence(
    scratch_dir: str,
    runs: List[Dict[str, Any]],
    binary: str,
    workspace: str,
    final_payload: Dict[str, Any],
) -> None:
    """Overwrite six SCRATCH artifacts after two consecutive passed:true runs."""
    os.makedirs(scratch_dir, exist_ok=True)

    live_sync = sync_live_mcp_binary(workspace, binary, scratch_dir)
    probe = probe_fresh_mcp_composites(binary, workspace)
    last_responses = (runs[-1] or {}).get("composite_responses") or {}
    parsed_lineage = _parse_composite_lineage(last_responses)
    composite_body = {
        "live_mcp_sync": live_sync,
        "mcp_tools_list_probe": probe,
        "parsed_lineage_final_run": parsed_lineage,
        "suite_composite_responses": [
            {
                "run_index": i + 1,
                "passed": r.get("passed"),
                "fidelity_rate": r.get("fidelity_rate"),
                "composite_responses": r.get("composite_responses", {}),
                "parsed_lineage": _parse_composite_lineage(r.get("composite_responses") or {}),
            }
            for i, r in enumerate(runs)
        ],
    }

    # fidelity_harness.json — must include suite_result
    harness_json_path = os.path.join(scratch_dir, "fidelity_harness.json")
    with open(harness_json_path, "w") as f:
        json.dump(final_payload, f, indent=2)

    # agent_tool_fidelity_harness.log — last two runs only
    log_path = os.path.join(scratch_dir, "agent_tool_fidelity_harness.log")
    with open(log_path, "w") as f:
        for i, r in enumerate(runs):
            f.write(f"=== RUN {i + 1} {_utc_now()} ===\n")
            f.write(json.dumps(r, indent=2))
            f.write("\n\n")

    # fidelity_demo.log — final passing run transcript
    demo_path = os.path.join(scratch_dir, "fidelity_demo.log")
    with open(demo_path, "w") as f:
        f.write(f"=== DEMO {_utc_now()} passed={runs[-1].get('passed')} ===\n")
        f.write(json.dumps(runs[-1], indent=2))
        f.write("\n")

    # composite_tool_evidence.txt — probe + in-suite MCP JSON bodies
    composite_path = os.path.join(scratch_dir, "composite_tool_evidence.txt")
    with open(composite_path, "w") as f:
        f.write("# composite_tool_evidence.txt\n")
        f.write(json.dumps(composite_body, indent=2))
        f.write("\n")

    # fidelity_diagnosis_source.txt — grep excerpts
    diag_path = os.path.join(scratch_dir, "fidelity_diagnosis_source.txt")
    with open(diag_path, "w") as f:
        f.write(capture_diagnosis_grep(workspace))

    # ritual_toml_evidence.txt — verif plan step 2
    ritual_path = os.path.join(scratch_dir, "ritual_toml_evidence.txt")
    ritual_files = [
        "processes/ritual/safe-code-edit.toml",
        "processes/ritual/verified-memory-update.toml",
        "processes/ritual/engram-working-memory.toml",
        "processes/meta/agent_evolution.toml",
    ]
    with open(ritual_path, "w") as f:
        f.write(f"# ritual_toml_evidence.txt — {_utc_now()}\n\n")
        for rel in ritual_files:
            path = os.path.join(workspace, rel)
            f.write(f"## {rel}\n")
            try:
                content = open(path).read()
                f.write(content[:2000])
                if len(content) > 2000:
                    f.write("\n... (truncated)\n")
            except OSError as e:
                f.write(f"(read failed: {e})\n")
            f.write("\n")

    # Remove stale polluted artifacts from prior manual captures
    stale_prefixes = (
        "agent_tool_fidelity_harness_run",
        "fidelity_demo_run",
        "fidelity_harness_harness-",
        "harness_evidence_run",
    )
    for name in os.listdir(scratch_dir):
        if any(name.startswith(p) for p in stale_prefixes):
            try:
                os.remove(os.path.join(scratch_dir, name))
            except OSError:
                pass

    print(f"Wrote fidelity evidence to {scratch_dir}", file=sys.stderr)