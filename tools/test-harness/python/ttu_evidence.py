#!/usr/bin/env python3
"""Launcher for tensor_thought_unification SCRATCH evidence — Rust handle_tool_call harness only."""

from __future__ import annotations

import os
import subprocess
import sys
from typing import Optional


def run_rust_scratch_evidence(scratch: str, workspace: str) -> int:
    """Sole SCRATCH writer: delegates to ttu_write_scratch_when_env_set cargo test."""
    env = os.environ.copy()
    env["SCRATCH"] = scratch
    env.setdefault("ENGRAM_DISABLE_SHEAF", "1")
    env.setdefault("ENGRAM_FORCE_CPU_BACKEND", "1")
    env.setdefault("ENGRAM_UPDATE_COHERENCE", "off")
    env.setdefault("ENGRAM_KI_DISABLE", "1")
    print(f"=== TTU SCRATCH via Rust handle_tool_call (SCRATCH={scratch}) ===")
    proc = subprocess.run(
        [
            "cargo",
            "test",
            "-p",
            "engram-server",
            "ttu_write_scratch_when_env_set",
            "--",
            "--test-threads=1",
            "--nocapture",
        ],
        cwd=workspace,
        env=env,
    )
    return proc.returncode


def run_fast_skip_test(workspace: str) -> int:
    """Fast path when SCRATCH unset — test returns immediately."""
    env = os.environ.copy()
    env.setdefault("ENGRAM_DISABLE_SHEAF", "1")
    proc = subprocess.run(
        [
            "cargo",
            "test",
            "-p",
            "engram-server",
            "ttu_write_scratch_when_env_set",
            "--",
            "--test-threads=1",
        ],
        cwd=workspace,
        env=env,
        capture_output=True,
        text=True,
    )
    return proc.returncode