# Hardware fit — host-adaptive Engram

Engram **detects the host** and selects a **host profile** so a laptop and a dual-GPU workstation both get sane defaults. Dual RTX + fast NVMe (a-monad class) is the **high end**, not the only supported shape.

## Host profile (`ENGRAM_HOST_PROFILE`)

| Value | When (auto) | Effects (defaults if env unset) |
|-------|-------------|----------------------------------|
| `auto` | default | Detect and apply |
| `minimal` | no GPU, RAM &lt; 32 GiB | Defer BVH, no cuFile, slim wake, small hot set |
| `cpu_large` | no GPU, RAM ≥ 32 GiB | Defer BVH, larger recall pools |
| `metal` | Apple Metal | No cuFile/OptiX; Metal path |
| `cuda_single` | 1× NVIDIA | Eager BVH, cuFile hot attempt, multiplex hot+compute |
| `cuda_dual` | ≥2× NVIDIA | GPU0 hot / GPU1 compute roles |
| `cuda_low_vram` | NVIDIA total VRAM &lt; 10 GiB | Defer BVH, small hot set, no cuFile default |

**Override:** set `ENGRAM_HOST_PROFILE=minimal` (etc.). Explicit env always wins over profile defaults.

**Orthogonal to** `ENGRAM_PROFILE=agent|deep|ui` (ritual/agent behavior).

## Readiness fields

`get_backend_readiness` includes:

- `host_profile_detected` / `host_profile_active` / `host_profile_override`
- `host_facts` (CPU, RAM, GPUs, cuFile driver present)
- `host_scaled` (defer BVH default, cuFile eligibility, dual-GPU roles)

## What we never claim without probe

- ROCm without AMD GPU  
- OptiX on CI runners  
- cuFile DMA success without `cufile_dma_success`  
- Dual-GPU roles on a single device (roles collapse honestly)
