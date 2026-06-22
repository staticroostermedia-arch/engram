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

# Step 2: git evidence (CONTEXT shas = missing refs; eb4c247b = compliant)
{
  echo "=== ASSERTION KEY ==="
  echo "cb5a7541/58283e64: conventional title+body but MISSING trace/goal refs (documented bad)"
  echo "eb4c247b: full discipline PASS (has Refs: trace + goal)"
  echo ""
  echo "=== git log -10 ==="
  git log --oneline -10 --decorate
  echo ""
  for sha in cb5a7541 58283e64 eb4c247b; do
    echo "=== git show $sha (full message) ==="
    git show "$sha" --format=fuller --no-patch
    echo "--- ref check ---"
    git show "$sha" --format=%B --no-patch | grep -E '(trace:|goal:)' && echo "HAS_REFS=yes" || echo "HAS_REFS=no"
    echo ""
  done
  echo "=== Cargo.toml version at eb4c247b (no bump on docs commit) ==="
  git show eb4c247b:Cargo.toml | head -15
  echo "=== CHANGELOG Unreleased section ==="
  sed -n '1,12p' CHANGELOG.md
  echo ""
  echo "=== validate-commit-msg on eb4c247b message ==="
  git show eb4c247b --format=%B --no-patch | "$REPO_ROOT/scripts/validate-commit-msg.sh" -
  echo ""
  echo "=== simulated compliant fix(server) message ==="
  SIM_MSG='fix(server): bundle injection completeness inputs for clippy CI

Refactor compute_injection_completeness in injection_priority.rs; update store.rs.

Refs: trace:1782162619_land-commit-discipline goal:commit_title_versioning_process'
  printf '%s\n' "$SIM_MSG" | "$REPO_ROOT/scripts/validate-commit-msg.sh" -
} > "$SCRATCH/git-commit-evidence.log" 2>&1

# Step 3: MCP ritual capture (2x)
for run in 1 2; do
  python3 "$HARNESS_ROOT/python/commit_process_verify_capture.py" \
    --binary "$BINARY" --store "$STORE" --scratch "$SCRATCH" --run "$run" \
    2>&1 | tee "$SCRATCH/engram-capture-run${run}.log"
done

# Step 4: harness + unit tests + git status
STABLE_BIN="$BINARY" "$SCRIPT_DIR/engram-harness.sh" \
  --suite agent-memory --binary "$BINARY" --workspace "$REPO_ROOT" --timeout 90 \
  2>&1 | tee "$SCRATCH/harness-commit-process.log"

cargo test -p engram-server build_continuation_bundle_emits_injection_observables -- --quiet \
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