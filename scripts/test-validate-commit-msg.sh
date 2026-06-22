#!/usr/bin/env bash
# Tests scripts/validate-commit-msg.sh against good/bad examples from CONTRIBUTING.md
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VALIDATOR="$SCRIPT_DIR/validate-commit-msg.sh"

GOOD_MSG='fix(server): bundle injection completeness inputs for clippy CI

Refactor compute_injection_completeness in injection_priority.rs.
Fixes clippy::too_many_arguments on GitHub CI.

Refs: trace:1782162559_use-conventional-commits goal:commit_title_versioning_process'

BAD_SHORT='clippy struct refactor'

BAD_NO_REFS='fix(server): bundle injection completeness inputs for clippy CI

Refactor compute_injection_completeness to take InjectionCompletenessInput struct.'

pass=0
fail=0

assert_pass() {
  local label="$1" msg="$2"
  if printf '%s' "$msg" | "$VALIDATOR" - >/dev/null 2>&1; then
    echo "PASS: $label"
    pass=$((pass + 1))
  else
    echo "FAIL expected pass: $label" >&2
    fail=$((fail + 1))
  fi
}

assert_fail() {
  local label="$1" msg="$2"
  if printf '%s' "$msg" | "$VALIDATOR" - >/dev/null 2>&1; then
    echo "FAIL expected reject: $label" >&2
    fail=$((fail + 1))
  else
    echo "PASS reject: $label"
    pass=$((pass + 1))
  fi
}

assert_pass "good full message" "$GOOD_MSG"
assert_fail "bad shorthand" "$BAD_SHORT"
assert_fail "bad missing refs" "$BAD_NO_REFS"

echo "---"
echo "pass=$pass fail=$fail"
[[ "$fail" -eq 0 ]]