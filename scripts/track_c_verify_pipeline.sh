#!/usr/bin/env bash
# Track C verification pipeline (round-4: strict parser + tile lift, linear).
# Usage: scripts/track_c_verify_pipeline.sh [SCRATCH_DIR]
set -euo pipefail

SCRATCH="${1:-/tmp/grok-goal-5879e8737396/implementer}"
REPO="$(cd "$(dirname "$0")/.." && pwd)"

mkdir -p "$SCRATCH"

echo "=== unittest track_c_recall_parser + fs_cleanup ===" | tee "$SCRATCH/pipeline.log"
python3 -m unittest scripts/test_track_c_recall_parser.py scripts/test_track_c_fs_cleanup.py -q 2>&1 | tee "$SCRATCH/pytest-recall-parser.log"

echo "=== track_c_fs_cleanup ===" | tee -a "$SCRATCH/pipeline.log"
python3 "$REPO/scripts/track_c_fs_cleanup.py" --scratch "$SCRATCH" | tee -a "$SCRATCH/pipeline.log"

echo "=== track_c_manifold_repair (protocol_gap only) ===" | tee -a "$SCRATCH/pipeline.log"
python3 "$REPO/scripts/track_c_manifold_repair.py" --scratch "$SCRATCH" --gaps-only | tee -a "$SCRATCH/pipeline.log"

echo "=== track_c_lift_tile_crs ===" | tee -a "$SCRATCH/pipeline.log"
python3 "$REPO/scripts/track_c_lift_tile_crs.py" --scratch "$SCRATCH" | tee -a "$SCRATCH/pipeline.log"

echo "=== track_c_acceptance_gate (strict, read-only) ===" | tee -a "$SCRATCH/pipeline.log"
python3 "$REPO/scripts/track_c_acceptance_gate.py" --scratch "$SCRATCH" 2>&1 | tee -a "$SCRATCH/pipeline.log"

echo "=== post-gate stability verify (strict, read-only) ===" | tee -a "$SCRATCH/pipeline.log"
python3 "$REPO/scripts/track_c_acceptance_gate.py" --scratch "$SCRATCH" 2>&1 | tee -a "$SCRATCH/pipeline-stability.log"

echo "PIPELINE PASS (strict gate + stability)" | tee -a "$SCRATCH/pipeline.log"
exit 0