// Phase 90.D (Suture): THE ARKADE 8K PROJECTOR — Gaussian CSRP
//
// Projects 8192-D Complex32 vectors from the unit hypersphere into R^3
// using a Gaussian Compressed Sensing Random Projection (CSRP) matrix.
// Ported to Apple Metal Shading Language (MSL).

#include <metal_stdlib>
using namespace metal;

#define DIM        8192
#define WARP_SIZE  32
#define GENESIS_ROOT_HASH 0x454E4752414D2026ULL
#define JL_SCALE   0.5773502691896258f

inline uint64_t xorshift64(uint64_t s) {
    s ^= s << 13; s ^= s >> 7; s ^= s << 17; return s;
}

inline float uniform01(uint64_t s) {
    return saturate(float(s >> 11) * (1.0f / float(1ULL << 53)));
}

inline float gaussian_sample(uint64_t s1, uint64_t s2) {
    float u1 = uniform01(s1);
    float u2 = uniform01(s2);
    if (u1 < 1e-7f) u1 = 1e-7f;
    return sqrt(-2.0f * log(u1)) * cos(2.0f * 3.14159265358979f * u2);
}

kernel void engram_project_8k_to_3d(
    device const float2* tensor [[buffer(0)]],
    device float3* point_out [[buffer(1)]],
    constant uint64_t& seed [[buffer(2)]],
    uint tid [[thread_position_in_threadgroup]],
    uint blockDim [[threads_per_threadgroup]],
    uint blockIdx [[threadgroup_position_in_grid]]
) {
    float lx = 0.0f, ly = 0.0f, lz = 0.0f;

    for (uint i = tid; i < DIM; i += blockDim) {
        float mag = length(tensor[i]);
        uint64_t base = seed ^ (uint64_t(i + 1) * 0x9e3779b97f4a7c15ULL);

        uint64_t sx1 = xorshift64(base ^ 0xAABBCCDD00112233ULL), sx2 = xorshift64(sx1);
        uint64_t sy1 = xorshift64(base ^ 0x11223344AABBCCDDULL), sy2 = xorshift64(sy1);
        uint64_t sz1 = xorshift64(base ^ 0xDEADBEEFCAFEBABEULL), sz2 = xorshift64(sz1);

        lx += mag * gaussian_sample(sx1, sx2) * JL_SCALE;
        ly += mag * gaussian_sample(sy1, sy2) * JL_SCALE;
        lz += mag * gaussian_sample(sz1, sz2) * JL_SCALE;
    }

    // SIMD group reduction
    lx = simd_sum(lx);
    ly = simd_sum(ly);
    lz = simd_sum(lz);

    threadgroup float smx[8];
    threadgroup float smy[8];
    threadgroup float smz[8];

    uint warpId = tid / WARP_SIZE;
    uint laneId = tid % WARP_SIZE;

    if (laneId == 0) {
        smx[warpId] = lx;
        smy[warpId] = ly;
        smz[warpId] = lz;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (warpId == 0) {
        uint nw = blockDim / WARP_SIZE;
        lx = (laneId < nw) ? smx[laneId] : 0.0f;
        ly = (laneId < nw) ? smy[laneId] : 0.0f;
        lz = (laneId < nw) ? smz[laneId] : 0.0f;

        lx = simd_sum(lx);
        ly = simd_sum(ly);
        lz = simd_sum(lz);

        if (tid == 0) {
            point_out[blockIdx] = float3(lx, ly, lz);
        }
    }
}

kernel void engram_cosine_batch(
    device const float2* query [[buffer(0)]],
    device const float2* candidates [[buffer(1)]],
    device float* scores [[buffer(2)]],
    constant int& N [[buffer(3)]],
    uint tid [[thread_position_in_threadgroup]],
    uint blockDim [[threads_per_threadgroup]],
    uint blockIdx [[threadgroup_position_in_grid]]
) {
    if ((int)blockIdx >= N) return;
    device const float2* cand = candidates + (blockIdx * DIM);

    float ldot = 0.0f, lna = 0.0f, lnb = 0.0f;

    for (uint i = tid; i < DIM; i += blockDim) {
        float2 q = query[i];
        float2 c = cand[i];
        ldot += q.x * c.x + q.y * c.y;
        lna  += q.x * q.x + q.y * q.y;
        lnb  += c.x * c.x + c.y * c.y;
    }

    ldot = simd_sum(ldot);
    lna  = simd_sum(lna);
    lnb  = simd_sum(lnb);

    threadgroup float s_dot[8], s_na[8], s_nb[8];
    uint warp = tid / WARP_SIZE, lane = tid % WARP_SIZE;

    if (lane == 0) {
        s_dot[warp] = ldot;
        s_na[warp] = lna;
        s_nb[warp] = lnb;
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    if (warp == 0) {
        uint nw = blockDim / WARP_SIZE;
        ldot = (lane < nw) ? s_dot[lane] : 0.0f;
        lna  = (lane < nw) ? s_na[lane]  : 0.0f;
        lnb  = (lane < nw) ? s_nb[lane]  : 0.0f;

        ldot = simd_sum(ldot);
        lna  = simd_sum(lna);
        lnb  = simd_sum(lnb);

        if (tid == 0) {
            float denom = sqrt(lna) * sqrt(lnb);
            scores[blockIdx] = (denom > 1e-8f) ? (ldot / denom) : 0.0f;
        }
    }
}
