#!/usr/bin/env bash
# Verification capture for commit title/versioning process goal (plan steps 2-4).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HARNESS_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$HARNESS_ROOT/../.." && pwd)"
SCRATCH="${SCRATCH:-/tmp/grok-goal-4275257a67aa/implementer}"
BINARY="${BINARY:-$REPO_ROOT/target/debug/engram}"
STORE="${STORE:-$HOME/.engram/stalks}"

mkdir -p "$SCRATCH/final-commit-process-evidence"
cd "$REPO_ROOT"

echo "SCRATCH=$SCRATCH"

# Step 1: plan excerpt
cp "/home/a/.grok/sessions/%2Fhome%2Fa%2FDocuments%2FEngram/019ec286-85aa-79d1-849b-c4a08298456a/goal/plan.md" "$SCRATCH/plan.md"
head -53 "/home/a/.grok/sessions/%2Fhome%2Fa%2FDocuments%2FEngram/019ec286-85aa-79d1-849b-c4a08298456a/goal/plan.md" > "$SCRATCH/plan-commit-process.txt"

# Step 2: git evidence via git_eng_evidence_capture.py (documents git-eng MCP tools + shell cross-check)
python3 "$HARNESS_ROOT/python/git_eng_evidence_capture.py" --scratch "$SCRATCH" \
  2>&1 | tee "$SCRATCH/git-eng-capture.log"

# Step 3: MCP ritual capture (2x) — includes context_for_edit on all 6 docs + session_end on run2
for run in 1 2; do
  python3 "$HARNESS_ROOT/python/commit_process_verify_capture.py" \
    --binary "$BINARY" --store "$STORE" --scratch "$SCRATCH" --run "$run" \
    2>&1 | tee "$SCRATCH/engram-capture-run${run}.log"
done

# Step 4: harness + exact plan unit test command + git status
STABLE_BIN="$BINARY" "$SCRIPT_DIR/engram-harness.sh" \
  --suite agent-memory --binary "$BINARY" --workspace "$REPO_ROOT" --timeout 90 \
  2>&1 | tee "$SCRATCH/harness-commit-process.log"

cargo test -p engram-server --test store -- --quiet \
  2>&1 | tee "$SCRATCH/unit-commit.log"

{
  git status --branch --short
  echo "---"
  ls -la "$SCRATCH"/*.log "$SCRATCH"/*.json 2>/dev/null || true
  echo "---"
  ls -la "$SCRATCH/final-commit-process-evidence/" 2>/dev/null || true
} > "$SCRATCH/git-status-and-scratch-ls.txt" 2>&1

cp "$SCRATCH"/*.log "$SCRATCH"/*.json "$SCRATCH"/*.txt "$SCRATCH/final-commit-process-evidence/" 2>/dev/null || true

echo "Done. Evidence in $SCRATCH/final-commit-process-evidence/"