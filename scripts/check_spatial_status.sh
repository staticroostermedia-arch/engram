#!/bin/bash
# Quick convenience check for Item 1.5 spatial awareness

echo "=== Item 1.5 Spatial Awareness Quick Check ==="
echo ""

echo "1. Current watched projects (if any status available):"
# Placeholder - in a real session the agent would query the state block
echo "   (Run: mcp_engram_read_concept concept:item1.5_spatial_ingestion_state_engram )"

echo ""
echo "2. Quick verification on key files:"
echo "   context_for_file path:\"/path/to/Documents/Engram/crates/engram-server/src/store.rs\""
echo "   context_for_file path:\"/path/to/Documents/Engram/.grok/skills/engram-wake-up/SKILL.md\""

echo ""
echo "3. If you want to force a refresh right now:"
echo "   mcp_engram_force_spatial_ingest paths:[\"/path/to/Documents/Engram/crates/engram-server/src\"] recursive:true"
