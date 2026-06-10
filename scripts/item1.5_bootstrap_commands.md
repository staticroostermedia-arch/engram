# Item 1.5 - AST Spatial Bootstrap Commands

After the engram MCP server is restarted with the latest binary, run these calls in your TUI/Grok session in this order:

## Phase 1: Core Server (Highest Leverage)
mcp_engram_force_spatial_ingest paths:["/path/to/Documents/Engram/crates/engram-server/src"] recursive:true

## Phase 2: Core Engine
mcp_engram_force_spatial_ingest paths:["/path/to/Documents/Engram/crates/engram-core/src"] recursive:true

## Phase 3: Ritual & Skill Layer (Critical for Inheritance)
mcp_engram_force_spatial_ingest paths:["/path/to/Documents/Engram/.grok/skills"] recursive:true

## Phase 4: AST Crate (for completeness)
mcp_engram_force_spatial_ingest paths:["/path/to/Documents/Engram/crates/engram-ast/src"] recursive:true

## Verification Steps (after above)
context_for_file path:"/path/to/Documents/Engram/crates/engram-server/src/store.rs"

recall_in_file file_stem:"store.rs" start_line:1320 end_line:1430

## Update State Block
mcp_engram_update concept:item1.5_spatial_ingestion_state_engram new_text:"bootstrap_completed: true
last_bootstrap: $(date)
directories_ingested:
- crates/engram-server/src
- crates/engram-core/src
- .grok/skills
- crates/engram-ast/src
notes: Rich AST nodes now available for Code Edit Ritual on core substrate files."

## Optional: Full Project (slower)
mcp_engram_force_spatial_ingest paths:["/path/to/Documents/Engram"] recursive:true
