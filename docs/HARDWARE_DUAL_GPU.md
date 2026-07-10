# Dual-GPU + NVMe Policy (this host)

**Audience:** Agents and operators on dual RTX 5060-class + T700-class NVMe  
**Status:** Tier-2 policy (2026-07)

## Device roles

| Env | Default | Role |
|-----|---------|------|
| `ENGRAM_GPU_HOT_DEVICE` | `0` | Hot set residency, BVH query / GPU search |
| `ENGRAM_GPU_COMPUTE_DEVICE` | `1` | Batch encode, NREM / heavy compute side work |

Surfaced on every wake as `mcp_health.gpu_hot_device` / `gpu_compute_device` and `get_backend_readiness`.

## cuFile / GDS honesty

| Label | Meaning |
|-------|---------|
| `cufile_dma` | Last DMA read of q-region **succeeded** |
| `h2d_memcpy` | Host→device copy fallback used |
| `unavailable` | Driver may be open / hot requested, but **no successful DMA** yet |
| `off` | `ENGRAM_CUFILE_HOT` not enabled |

`cufile_hot_ready=true` with `cufile_transfer_path=unavailable` is a **valid honest state** (driver open, no DMA proof).

## Operator scripts

```bash
# After cold boot
./scripts/hw_readiness.sh ~/.engram/stalks/ /tmp/hw_readiness.txt

# q-load microbench (O_DIRECT host path; DMA claimed only via readiness after real GPU path)
./scripts/hw_microbench_qload.sh ~/.engram/stalks/ 256 /tmp/hw_microbench.txt
```

## Related

- `crates/engram-gpu/src/cufile.rs`
- `docs/CONTEXT_INJECTION_NVME_BYPASS.md`
- `docs/plans/tier2-trust-hardware-v1.md`
