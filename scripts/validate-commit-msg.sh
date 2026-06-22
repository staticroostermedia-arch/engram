#!/usr/bin/env bash
# Validate commit message against CONTRIBUTING.md ## Commit Message & Versioning Discipline
# Usage: validate-commit-msg.sh <msg-file> | validate-commit-msg.sh -  (stdin)
set -euo pipefail

MSG_FILE="${1:--}"
if [[ "$MSG_FILE" == "-" ]]; then
  MSG="$(cat)"
else
  MSG="$(cat "$MSG_FILE")"
fi

failures=0
report() { echo "validate-commit-msg: $1"; failures=$((failures + 1)); }

# Conventional title: type(scope)?: description (first non-empty line)
TITLE="$(printf '%s\n' "$MSG" | grep -m1 -v '^[[:space:]]*$' || true)"
if [[ -z "$TITLE" ]]; then
  report "empty message"
else
  if ! printf '%s' "$TITLE" | grep -qE '^(feat|fix|docs|style|refactor|test|chore|perf|ci)(\([a-z0-9_-]+\))?: [a-z].+[^.]$'; then
    report "title must match conventional commits: <type>[scope]: <lowercase description without period>"
  fi
  if [[ ${#TITLE} -gt 72 ]]; then
    report "title exceeds 72 characters (${#TITLE})"
  fi
fi

# Body: at least one non-empty line after title
BODY_LINES="$(printf '%s\n' "$MSG" | tail -n +2 | grep -c -v '^[[:space:]]*$' || true)"
if [[ "$BODY_LINES" -lt 1 ]]; then
  report "missing body (blank line + at least one explanatory line required)"
fi

# trace or goal ref
if ! printf '%s' "$MSG" | grep -qE '(trace:[a-zA-Z0-9_.-]+|goal:[a-zA-Z0-9_.-]+)'; then
  report "missing trace:* or goal:* reference in message"
fi

if [[ "$failures" -gt 0 ]]; then
  echo "See CONTRIBUTING.md ## Commit Message & Versioning Discipline"
  exit 1
fi
echo "validate-commit-msg: OK"
exit 0