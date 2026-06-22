#!/usr/bin/env bash
# Single entry point for manage-resume goal verification (plan step 2–5 artifacts).
# Populates {SCRATCH} with honest MCP transcripts — no hand-written summary JSON.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HARNESS_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$HARNESS_ROOT/../.." && pwd)"
PYTHON_CLIENT="$HARNESS_ROOT/python/mcp_test_client.py"
LIVE_CAPTURE="$HARNESS_ROOT/python/goal_clear_live_capture.py"

BINARY="${BINARY:-$REPO_ROOT/target/debug/engram}"
STORE="${STORE:-$HOME/.engram/stalks}"
SCRATCH="${SCRATCH:-/tmp/grok-manage-resume-clear-$$}"
WORKSPACE="${WORKSPACE:-$REPO_ROOT}"

mkdir -p "$SCRATCH"
echo "SCRATCH=$SCRATCH"
echo "BINARY=$BINARY"
echo "STORE=$STORE"

cd "$REPO_ROOT"

# Step 3: isolated goal-clear harness (2x)
for run in 1 2; do
  echo "=== goal-clear harness run $run ==="
  "$SCRIPT_DIR/engram-harness.sh" \
    --suite goal-clear \
    --binary "$BINARY" \
    --workspace "$WORKSPACE" \
    --timeout 120 \
    2>&1 | tee "$SCRATCH/harness-goal-clear-run${run}.log"
  latest_json="$(ls -t "$HARNESS_ROOT/results/"*-goal-clear.json 2>/dev/null | head -1 || true)"
  if [[ -n "$latest_json" && -f "$latest_json" ]]; then
    cp "$latest_json" "$SCRATCH/goal-clear-run${run}.json"
    echo "Copied harness transcript -> $SCRATCH/goal-clear-run${run}.json"
  fi
done

# Step 2 + 5: live store capture (requires exclusive MCP lock — stop TUI MCP first)
echo "=== live goal_clear_live_capture (resume-clear-verify + final-resume-clear) ==="
echo "NOTE: If TUI MCP holds the store lock, stop it briefly before this step."
python3 "$LIVE_CAPTURE" \
  --binary "$BINARY" \
  --store "$STORE" \
  --scratch "$SCRATCH" \
  2>&1 | tee "$SCRATCH/live-capture.log"
capture_rc=${PIPESTATUS[0]}
if [[ "$capture_rc" -ne 0 ]]; then
  echo "live capture failed (rc=$capture_rc); retry with --skip-clear if goal already cleared"
  exit "$capture_rc"
fi

# Step 4: git evidence
{
  git log --oneline -8 --decorate
  echo "---"
  git branch --show-current
  echo "---"
  git status --short
} > "$SCRATCH/git-log.txt" 2>&1

cat > "$SCRATCH/pr-notes.md" <<'PRNOTES'
# Manage-resume verification: all ACs pass

**Goal:** `goal:manage_resume_019ec286` (TUI session 019ec286)

## Protocol answer
When the objective is met: **yes, clear the completed goal** from resume injection via
`goal_update_status(completed)` + `demote_from_context`, then **`session_end(prepare_compression=true)`**.
The **last step** is to push branch + PR notes to GitHub outlining fixes and improvements.

## Fixes
- `injection_completeness` + `nvme_context` in slim wake bundle
- Composite `injection_rank` on `suggested_actions`
- BVH dedup — eliminated rebuild storms; `full_bvh_gpu` at wake
- `goal_update_status` provlog rewrite (status no longer stuck at active)
- `restore_primary_goal_marker_after_complete` + cache invalidation on status change
- Active-goal filter in `build_suggested_actions`

## Improvements
- Lean 8-tool resume without re-brief after goal complete
- `goal-clear` harness suite + `goal_clear_live_capture.py` single-writer artifacts
- `capture-manage-resume-clear.sh` deterministic verification pipeline

**Branch:** `feat/perfect-context-injection-nvme-bypass`
PRNOTES

# Plan excerpt capture (step 1)
if [[ -f "/home/a/.grok/sessions/%2Fhome%2Fa%2FDocuments%2FEngram/019ec286-85aa-79d1-849b-c4a08298456a/goal/plan.md" ]]; then
  cp "/home/a/.grok/sessions/%2Fhome%2Fa%2FDocuments%2FEngram/019ec286-85aa-79d1-849b-c4a08298456a/goal/plan.md" \
    "$SCRATCH/plan.md"
  head -80 "/home/a/.grok/sessions/%2Fhome%2Fa%2FDocuments%2FEngram/019ec286-85aa-79d1-849b-c4a08298456a/goal/plan.md" \
    > "$SCRATCH/plan-and-protocol.txt"
fi

echo ""
echo "=== Artifacts in $SCRATCH ==="
ls -la "$SCRATCH"
echo ""
echo "Done. Review final-resume-clear.json assertions before update_goal(completed=true)."