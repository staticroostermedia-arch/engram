#!/bin/bash
# Item 1.5 - Automated AST Spatial Bootstrap helper
# Run this after the engram MCP server is restarted with the latest binary.

set -e

ENGRA M_ROOT="/path/to/Documents/Engram"

echo "=== Item 1.5 Spatial Bootstrap ==="
echo "This will use mcp_engram_force_spatial_ingest on priority directories."
echo ""

# Priority order for best leverage on self-modification work
PRIORITY_DIRS=(
    "$ENGRA M_ROOT/crates/engram-server/src"
    "$ENGRA M_ROOT/crates/engram-core/src"
    "$ENGRA M_ROOT/.grok/skills"
    "$ENGRA M_ROOT/crates/engram-ast/src"
)

for dir in "${PRIORITY_DIRS[@]}"; do
    if [ -d "$dir" ]; then
        echo "Bootstrapping: $dir"
        # The actual call the user should paste into the TUI / Grok
        echo "  -> Run this in your TUI:"
        echo "     mcp_engram_force_spatial_ingest paths:[\"$dir\"] recursive:true"
        echo ""
    else
        echo "Skipping (not found): $dir"
    fi
done

echo "After running the above calls, update the state with:"
echo "  mcp_engram_update concept:item1.5_spatial_ingestion_state_engram new_text:\"...bootstrap completed on $(date)...\""
echo ""
echo "Then verify with context_for_file on key files like store.rs"
