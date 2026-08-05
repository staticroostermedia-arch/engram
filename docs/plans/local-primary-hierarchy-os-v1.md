# Hierarchy OS — cold / warm / hot / compute (a-monad)

**Goal:** `goal:engram_local_primary_critical_path_v1`  
**Host:** dual RTX (0=hot, 1=compute) + T700 NVMe + ~93 GiB RAM

## Tiers

| Tier | Location | Contents | Promote trigger | Demote trigger |
|------|----------|----------|-----------------|----------------|
| **Cold** | T700 `.leg3` O_DIRECT | Full 256KB blocks | Default home for all concepts | Never delete without explicit forget |
| **Warm** | Host RAM | CSR adj, access index, tensors, BVH CPU tree | First recall / relation walk | Capacity compress / NREM unmark non-protected |
| **Hot** | GPU0 resident | Agent hot_set + LegView | promote_hot, primary_goal, edit path | hot_set soft/hard threshold compress |
| **Compute** | GPU1 | BVH rebuild, batch cosine, NREM jobs | Background rebuild when deferred | Completes then frees device mem |

## Dual-GPU policy (enforced in readiness labels)

- `hierarchy_gpu0_role` = `hot_agent_resident`
- `hierarchy_gpu1_role` = `compute_bvh_batch_nrem`
- MCP event loop must not block on GPU1 rebuild (async BVH already)

## Hit rates

Logged via readiness `hierarchy_hot_set_len` + recall path mode (`recall_mode`).
Series: emit `metric:hierarchy_hit_*` on future probes; baseline in SCRATCH.

## Capacity

Soft hot_set threshold 1000 / hard 2000 — `mcp_engram_apply_capacity_hot_compress` demotes residency only (no block delete).
