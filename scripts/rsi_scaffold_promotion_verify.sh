#!/usr/bin/env bash
# Scaffold versioning + gated promotion verify (AutoMem Tier A4, Cycle 14).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SCRATCH="${SCRATCH:-/tmp/grok-goal-f165d4817872/implementer}"

mkdir -p "$SCRATCH"
cd "$REPO_ROOT"

: > "$SCRATCH/scaffold-promotion-verify.log"

echo "=== scaffold promotion unit tests ===" | tee -a "$SCRATCH/scaffold-promotion-verify.log"
if cargo test -p engram-server -- scaffold_registry scaffold_promotion 2>&1 | tee -a "$SCRATCH/scaffold-promotion-verify.log"; then
  if grep -qE 'running [1-9][0-9]* test' "$SCRATCH/scaffold-promotion-verify.log"; then
    echo "SCAFFOLD_TEST_EXIT=0" | tee -a "$SCRATCH/scaffold-promotion-verify.log"
  else
    echo "SCAFFOLD_TEST_EXIT=1 (matched 0 tests)" | tee -a "$SCRATCH/scaffold-promotion-verify.log"
    exit 1
  fi
else
  echo "SCAFFOLD_TEST_EXIT=1" | tee -a "$SCRATCH/scaffold-promotion-verify.log"
  exit 1
fi

echo "OVERALL_EXIT=0" | tee -a "$SCRATCH/scaffold-promotion-verify.log"
exit 0