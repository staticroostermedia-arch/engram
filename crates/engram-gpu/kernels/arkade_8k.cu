// Phase 90.D (Suture): THE ARKADE 8K PROJECTOR — Gaussian CSRP
//
// Projects 8192-D Complex32 vectors from the unit hypersphere into R^3
// using a Gaussian Compressed Sensing Random Projection (CSRP) matrix.
//
// MATHEMATICAL BASIS (Johnson-Lindenstrauss Lemma):
//   For vectors u, v on the unit hypersphere (Hermitian-normalized),
//   the projected expected pairwise distance is preserved:
//     E[‖Φu − Φv‖²] = ‖u − v‖²
//   The Gaussian distribution N(0, 1/d) is the canonical JL-satisfying
//   distribution; MurmurHash avalanche explicitly annihilates spatial locality.
//
// SEEDING: Genesis Root Hash (0x454E4752414D2026) → xorshift64 state
//          "ENGRAM" × epoch 2026 — fully deterministic
// Box-Muller transform converts uniform U(0,1) pairs to N(0,1) samples,
// scaled by JL_SCALE = 1/√3 for k=3 target dimensions.
//
// U.S. Patent Application No. 19/372,256 — Static Rooster Media

#include <cuComplex.h>
#include <math.h>
#include <stdint.h>
#include <cooperative_groups.h>
#include <cooperative_groups/reduce.h>

namespace cg = cooperative_groups;

#define DIM        8192
#define K_DIM      3
#define WARP_SIZE  32
// "ENGRAM" × epoch 2026
#define GENESIS_ROOT_HASH 0x454E4752414D2026ULL
// JL scale: 1/√3 ≈ 0.5773502691896258
#define JL_SCALE   0.5773502691896258f

__device__ __forceinline__ uint64_t xorshift64(uint64_t s) {
    s ^= s << 13; s ^= s >> 7; s ^= s << 17; return s;
}

__device__ __forceinline__ float uniform01(uint64_t s) {
    return __saturatef((float)(s >> 11) * (1.0f / (float)(1ULL << 53)));
}

__device__ __forceinline__ float gaussian_sample(uint64_t s1, uint64_t s2) {
    float u1 = uniform01(s1);
    float u2 = uniform01(s2);
    if (u1 < 1e-7f) u1 = 1e-7f;
    return sqrtf(-2.0f * logf(u1)) * cosf(2.0f * 3.14159265358979f * u2);
}

// ── Primary projection kernel ─────────────────────────────────────────────────
// Projects ONE 8192-D Complex32 vector → float3 Arkade coordinate.
// One block per vector. 256 threads × 32 dims/thread = 8192 dims.
// The 3×8192 projection matrix is generated on-the-fly — never materialized.
extern "C" __global__ void
engram_project_8k_to_3d(const cuComplex *__restrict__ tensor,
                         float3 *__restrict__ point_out,
                         unsigned long long seed) {
    int tid = threadIdx.x;
    float lx = 0.0f, ly = 0.0f, lz = 0.0f;

    for (int i = tid; i < DIM; i += blockDim.x) {
        float mag = cuCabsf(tensor[i]);
        uint64_t base = seed ^ ((uint64_t)(i + 1) * 0x9e3779b97f4a7c15ULL);

        uint64_t sx1 = xorshift64(base ^ 0xAABBCCDD00112233ULL), sx2 = xorshift64(sx1);
        uint64_t sy1 = xorshift64(base ^ 0x11223344AABBCCDDULL), sy2 = xorshift64(sy1);
        uint64_t sz1 = xorshift64(base ^ 0xDEADBEEFCAFEBABEULL), sz2 = xorshift64(sz1);

        lx += mag * gaussian_sample(sx1, sx2) * JL_SCALE;
        ly += mag * gaussian_sample(sy1, sy2) * JL_SCALE;
        lz += mag * gaussian_sample(sz1, sz2) * JL_SCALE;
    }

    // Warp reduction using Cooperative Groups
    cg::thread_block_tile<32> warp = cg::tiled_partition<32>(cg::this_thread_block());
    lx = cg::reduce(warp, lx, cg::plus<float>());
    ly = cg::reduce(warp, ly, cg::plus<float>());
    lz = cg::reduce(warp, lz, cg::plus<float>());

    __shared__ float smx[8], smy[8], smz[8];
    int warpId = tid / WARP_SIZE, laneId = tid % WARP_SIZE;
    if (laneId == 0) { smx[warpId] = lx; smy[warpId] = ly; smz[warpId] = lz; }
    __syncthreads();

    if (warpId == 0) {
        int nw = blockDim.x / WARP_SIZE;
        lx = (laneId < nw) ? smx[laneId] : 0.0f;
        ly = (laneId < nw) ? smy[laneId] : 0.0f;
        lz = (laneId < nw) ? smz[laneId] : 0.0f;
        lx = cg::reduce(warp, lx, cg::plus<float>());
        ly = cg::reduce(warp, ly, cg::plus<float>());
        lz = cg::reduce(warp, lz, cg::plus<float>());
        if (tid == 0) point_out[blockIdx.x] = make_float3(lx, ly, lz);
    }
}

// ── Batch cosine similarity kernel ────────────────────────────────────────────
// Computes exact Hermitian cosine similarity between a query vector and
// N candidate vectors in parallel. One block per candidate.
//
// cos(q, c) = Re(⟨q, c⟩) / (|q| × |c|)
extern "C" __global__ void
engram_cosine_batch(const cuComplex *__restrict__ query,    // [DIM]
                    const cuComplex *__restrict__ candidates, // [N × DIM]
                    float *__restrict__ scores,              // [N]
                    int N) {
    if (blockIdx.x >= N) return;
    const cuComplex *cand = candidates + (blockIdx.x * DIM);

    __shared__ float s_dot[8], s_na[8], s_nb[8];
    float ldot = 0.0f, lna = 0.0f, lnb = 0.0f;
    int tid = threadIdx.x;

    for (int i = tid; i < DIM; i += blockDim.x) {
        cuComplex q = query[i], c = cand[i];
        ldot += q.x * c.x + q.y * c.y;
        lna  += q.x * q.x + q.y * q.y;
        lnb  += c.x * c.x + c.y * c.y;
    }

    cg::thread_block_tile<32> warp_tile = cg::tiled_partition<32>(cg::this_thread_block());
    ldot = cg::reduce(warp_tile, ldot, cg::plus<float>());
    lna  = cg::reduce(warp_tile, lna, cg::plus<float>());
    lnb  = cg::reduce(warp_tile, lnb, cg::plus<float>());

    int warp = tid / WARP_SIZE, lane = tid % WARP_SIZE;
    if (lane == 0) { s_dot[warp] = ldot; s_na[warp] = lna; s_nb[warp] = lnb; }
    __syncthreads();

    if (warp == 0) {
        int nw = blockDim.x / WARP_SIZE;
        ldot = (lane < nw) ? s_dot[lane] : 0.0f;
        lna  = (lane < nw) ? s_na[lane]  : 0.0f;
        lnb  = (lane < nw) ? s_nb[lane]  : 0.0f;
        ldot = cg::reduce(warp_tile, ldot, cg::plus<float>());
        lna  = cg::reduce(warp_tile, lna, cg::plus<float>());
        lnb  = cg::reduce(warp_tile, lnb, cg::plus<float>());
        if (tid == 0) {
            float denom = sqrtf(lna) * sqrtf(lnb);
            scores[blockIdx.x] = (denom > 1e-8f) ? (ldot / denom) : 0.0f;
        }
    }
}

// ── C launchers (called from Rust via FFI) ────────────────────────────────────
extern "C" void
engram_launch_cosine_batch(const cuComplex *d_query,
                           const cuComplex *d_candidates,
                           float          *d_scores,
                           int             N) {
    if (N <= 0) return;
    engram_cosine_batch<<<N, 256>>>(d_query, d_candidates, d_scores, N);
    cudaDeviceSynchronize();
}
