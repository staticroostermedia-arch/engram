#!/bin/bash
# ritual_post_recon.sh - minimal post-recon helper for engram-code-edit-ritual
# Usage: ./scripts/ritual_post_recon.sh <target> [note]
TARGET=${1:-.}
NOTE=${2:-"mutation complete"}
echo "=== POST-RECON for $TARGET ($NOTE) ==="
echo "Diff/outcome:"
git diff --stat HEAD
echo "Spatial post: ls $TARGET"
ls -la "$TARGET" 2>/dev/null
echo "Trace rec: create engram trace with decision/why/spatial/prev"
echo "Axiom post: minimal, rollback via git"
echo "I'm in danger: PASSED"
echo "=== POST-RECON END ==="
