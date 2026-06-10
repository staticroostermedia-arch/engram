#!/bin/bash
# launch-leg-browser-review.sh
# One-command launcher for reviewing the current geometric mind state in the leg-browser.
# Static quick-review mode (always works, rich demo payloads for hero cards + new tiles).
# Full live dynamic mode: also run `engram serve` in another terminal (or background).

set -euo pipefail

PORT=${1:-8765}
DIR="tools/leg-browser"
URL="http://localhost:${PORT}"

echo "🚀 Launching LEG Browser Review (static quick-review mode with rich hero payloads)"
echo "   Port: $PORT"
echo "   This mode now delivers actual contents for unifying tiles, provenance, handoff deltas, etc. on click."
echo ""
echo "   For FULL DYNAMIC live updates (recommended one-cmd):"
echo "     ./scripts/leg --live     # starts engram serve --light --no-scout + bg consciousness/world_state tile emitter (real new tile:consciousness:live-demo:* + world_state:* emitted via /api)"
echo "     Then: status pill auto emerald; consciousness dashboard pulls live tiles (dynamic, not old embedded); interject emits real tiles the loop consumes."
echo "   Manual: engram serve --light --no-scout ; click pill/'Try live' in UI at 8765"
echo ""

# Start the static server in background
python3 -m http.server "$PORT" --directory "$DIR" >/dev/null 2>&1 &
SERVER_PID=$!

sleep 1.2

# Open browser (macOS / Linux / generic)
if command -v open >/dev/null 2>&1; then
    open "$URL"
elif command -v xdg-open >/dev/null 2>&1; then
    xdg-open "$URL" >/dev/null 2>&1 || true
else
    echo "Open this URL manually: $URL"
fi

echo ""
echo "✅ LEG Browser open at $URL"
echo "   Hero 'Current Work' cards now render rich formatted content on click (even in pure static)."
echo "   Activity Canvas has 'Inject Handoff/Trace' demo actions to feel alive."
echo "   Press Ctrl-C here to stop the static server when done."
echo ""

# Wait for the server process
wait $SERVER_PID || true
echo "Server stopped."