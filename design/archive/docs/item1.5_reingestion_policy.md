# Item 1.5 - Re-ingestion Policy for AST Spatial Data

## Core Principle
The spatial/AST layer should feel "alive" — new saves and deliberate bootstrap calls both contribute to a continuously improving geometric view of the codebase.

## How Ingestion Works

### Normal Operation (File Watcher)
- Every allowed file save while the watcher is bound triggers:
  - Tree-sitter extraction of top-level items (fn, struct, impl, etc.)
  - Creation/update of HolographicBlocks with AABB coordinates
  - Automatic relational gluing (defines, next_sibling_in_file, etc.)
- This is incremental by nature. Changed functions update their blocks; new functions create new ones.

### Bootstrap / Force Ingest (mcp_engram_force_spatial_ingest)
- Used for historical coverage or after watcher was not active.
- Behaves like a batch of "virtual saves".
- Existing concepts with the same name are updated via the store logic (not duplicated).
- New concepts are created.

## Tracking "Last Fully Bootstrapped"

We maintain `item1.5_spatial_ingestion_state_engram` (or a more general `spatial_ingestion_state::<project>` block) with:

- `last_full_bootstrap_timestamp`
- `directories_ingested`
- `approximate_ast_node_count`
- `notes` (including any known gaps)

On every wake-up (Phase 5), the agent checks this state.

If `last_full_bootstrap_timestamp` is missing or older than a threshold (e.g., 30 days or after major refactors), it surfaces as a hygiene item.

## Update vs New Semantics

- The underlying storage (`store` / `update` path) handles this.
- When ingesting, if a concept with the exact same name already exists, the store layer will evolve it (preserving thermodynamic history where possible).
- In practice, for AST nodes we often get fresh blocks on significant changes because the content (and therefore the embedding) changes.

## Recommended Discipline

1. After any large refactoring or when the watcher was down for a while → run a targeted `force_spatial_ingest` on the affected directories.
2. Treat the state block as the source of truth for "how spatially aware is this project right now?"
3. Use scars + traces when you discover you did significant self-editing work without recent spatial coverage.

This policy keeps the substrate honest about its own visibility into the code it is responsible for.
