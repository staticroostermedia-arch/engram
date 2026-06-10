// Phase 8 — OptiX Intersection Program (Engram)
//
// Custom IS program for AABB primitives in the Engram semantic manifold.
// Implements Condition 2: interior rays (query origin inside AABB) report t=0.0
// so that RT-Core traversal does not discard them, enabling KNN recall for
// queries whose 3D projection lands inside the concept's bounding sphere.
//
// GENESIS_SEED = 0x454E4752 ("ENGR") — matches engram-gpu/src/bvh.rs.
// AABB_RADIUS  = 200.0               — matches engram-core/src/index.rs.
//
// OptiX 9 note: optixGetPrimitiveAabb() was removed. AABB bounds are now
// fetched from the launch params d_aabb_data buffer via optixGetPrimitiveIndex().
//
// Compiled to PTX by build.rs:
//   nvcc --ptx --gpu-architecture=compute_86 -I${OPTIX_SDK_PATH}/include optix_intersect.cu

#include <optix.h>
#include <stdint.h>

// Must match the layout in optix_rg.cu, optix_ah.cu, and optix_host.cpp.
struct EngramLaunchParams {
    float3*                  d_query_positions;
    OptixTraversableHandle   traversable;
    unsigned long long*      d_hit_list;
    unsigned int*            d_hit_counts;
    unsigned int             max_hits_per_query;
    const float*             d_aabb_data;   // packed [minX,minY,minZ,maxX,maxY,maxZ] × n
};

extern "C" __constant__ EngramLaunchParams params;

extern "C" __global__ void __intersection__engram_aabb() {
    // Query ray in object space (same as world space — no instance transforms)
    const float3 ro = optixGetObjectRayOrigin();
    const float3 rd = optixGetObjectRayDirection();

    // Retrieve per-primitive AABB bounds from launch params (OptiX 9 compatible).
    // optixGetPrimitiveAabb() was removed after OptiX 7.3; use the primitive
    // index to look up bounds in the device-side aabb_data array instead.
    const uint32_t pi = optixGetPrimitiveIndex();
    const float* ab   = params.d_aabb_data + (size_t)pi * 6;
    // ab[0..2] = min XYZ,  ab[3..5] = max XYZ

    // Direct coordinate-wise containment check with a small epsilon tolerance
    const float eps = 1e-3f;
    if (ro.x >= ab[0] - eps && ro.x <= ab[3] + eps &&
        ro.y >= ab[1] - eps && ro.y <= ab[4] + eps &&
        ro.z >= ab[2] - eps && ro.z <= ab[5] + eps) {
        // Calculate exit distance tmax along ray direction (rd has positive components)
        float tx = (rd.x != 0.0f) ? (ab[3] - ro.x) / rd.x : 1e16f;
        float ty = (rd.y != 0.0f) ? (ab[4] - ro.y) / rd.y : 1e16f;
        float tz = (rd.z != 0.0f) ? (ab[5] - ro.z) / rd.z : 1e16f;
        float tmax = fminf(fminf(tx, ty), tz);
        if (tmax < 0.0f) tmax = 0.0f;
        optixReportIntersection(tmax, 0u);
    }
}
