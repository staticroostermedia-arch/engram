#!/usr/bin/env bash
# Trajectory-level metamemory meta-review (AutoMem Tier A3, arXiv:2607.01224).
# Runs unit gates + prints harness review hint for receipt:session_* aggregation.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SCRATCH="${SCRATCH:-/tmp/grok-goal-f165d4817872/implementer}"

mkdir -p "$SCRATCH"
cd "$REPO_ROOT"

: > "$SCRATCH/trajectory-meta-review.log"

echo "=== trajectory meta-review unit tests ===" | tee -a "$SCRATCH/trajectory-meta-review.log"
if cargo test -p engram-server -- trajectory_meta_review consult_before_write 2>&1 | tee -a "$SCRATCH/trajectory-meta-review.log"; then
  if grep -qE 'running [1-9][0-9]* test' "$SCRATCH/trajectory-meta-review.log"; then
    echo "TRAJECTORY_TEST_EXIT=0" | tee -a "$SCRATCH/trajectory-meta-review.log"
  else
    echo "TRAJECTORY_TEST_EXIT=1 (matched 0 tests)" | tee -a "$SCRATCH/trajectory-meta-review.log"
    exit 1
  fi
else
  echo "TRAJECTORY_TEST_EXIT=1" | tee -a "$SCRATCH/trajectory-meta-review.log"
  exit 1
fi

echo "=== trajectory review artifact ===" | tee -a "$SCRATCH/trajectory-meta-review.log"
echo "Receipt aggregation: StoreHandle::trajectory_meta_review(max) over receipt:session_* sidecars" | tee -a "$SCRATCH/trajectory-meta-review.log"
echo "Harness hint: rsi_cycle_metrics.trajectory_meta_review_hint" | tee -a "$SCRATCH/trajectory-meta-review.log"
echo "OVERALL_EXIT=0" | tee -a "$SCRATCH/trajectory-meta-review.log"
exit 0