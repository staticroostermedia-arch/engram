// Phase 8 — OptiX BVH Pipeline Host Code (Engram)
//
// Mirrors CodeLand's optix_host.cpp with Engram-prefixed symbols.
// Exposes a clean C ABI for Rust FFI from optix_pipeline.rs.
//
// Build gate: compiled only when OPTIX_SDK_AVAILABLE is defined (i.e. OPTIX_SDK_PATH set).
// Without the SDK this file compiles as pure stubs that return nullptr / -1.

#ifdef OPTIX_SDK_AVAILABLE

#include <optix.h>
#include <optix_stubs.h>
// REQUIRED: defines g_optixFunctionTable (the version-stamped function pointer table).
// optix_stubs.h only declares it — this header provides the one definition per link unit.
#include <optix_function_table_definition.h>
#include <cuda_runtime.h>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>

// ── Helpers ──────────────────────────────────────────────────────────────────

#define OPTIX_CHECK_NULL(call)                                              \
    do {                                                                    \
        OptixResult res = (call);                                           \
        if (res != OPTIX_SUCCESS) {                                         \
            fprintf(stderr, "[Engram-OptiX] %s:%d — %s (%d)\n",            \
                    __FILE__, __LINE__, optixGetErrorString(res), (int)res);\
            return nullptr;                                                 \
        }                                                                   \
    } while(0)

// ── Launch parameters (must match kernels/optix_rg.cu + optix_ah.cu) ─────────

struct EngramLaunchParams {
    float3*                 d_query_positions;
    OptixTraversableHandle  traversable;
    unsigned long long*     d_hit_list;
    unsigned int*           d_hit_counts;
    unsigned int            max_hits_per_query;
    const float*            d_aabb_data;   // packed [minX,minY,minZ,maxX,maxY,maxZ] × n
};

// ── Pipeline state ────────────────────────────────────────────────────────────

struct EngramOptiXPipeline {
    OptixDeviceContext      context     = nullptr;
    OptixPipeline           pipeline    = nullptr;
    OptixShaderBindingTable sbt         = {};
    OptixTraversableHandle  traversable = 0;

    CUdeviceptr d_sbt_hitgroup  = 0;
    CUdeviceptr d_sbt_raygen    = 0;
    CUdeviceptr d_sbt_miss      = 0;
    CUdeviceptr d_gas_output    = 0;
    CUdeviceptr d_aabb_input    = 0;

    CUdeviceptr d_query_pos     = 0;
    CUdeviceptr d_hit_list      = 0;
    CUdeviceptr d_hit_counts    = 0;
    uint32_t    scratch_queries = 0;
    uint32_t    scratch_hits    = 0;
};

static void engram_optix_log(unsigned int level, const char* tag,
                              const char* msg, void*) {
    fprintf(stderr, "[Engram-OptiX-Logger:%u:%s] %s\n", level, tag, msg);
    fflush(stderr);
}

// ── Public C API ─────────────────────────────────────────────────────────────

extern "C" {

EngramOptiXPipeline* engram_optix_init(
    const float* aabb_data,
    uint32_t     n_primitives,
    const char*  ptx_is,
    const char*  ptx_rg,
    const char*  ptx_ah,
    const char*  ptx_ms)
{
    auto* state = new EngramOptiXPipeline();

    OptixResult init_res = optixInit();
    fprintf(stderr, "[Engram-OptiX] optixInit() returned: %d\n", (int)init_res);
    fflush(stderr);

    // CRITICAL: if optixInit() fails the function table is uninitialized.
    // Calling ANY OptiX API after this (including optixDeviceContextCreate)
    // dereferences a null function pointer -> SIGSEGV. Return immediately.
    if (init_res != OPTIX_SUCCESS) {
        fprintf(stderr, "[Engram-OptiX] optixInit() failed (%d) — OptiX unavailable on this driver/GPU. CPU BVH fallback.\n", (int)init_res);
        fflush(stderr);
        delete state;
        return nullptr;
    }

    cudaError_t free_err = cudaFree(nullptr);
    fprintf(stderr, "[Engram-OptiX] cudaFree(nullptr) returned: %d (%s)\n", (int)free_err, cudaGetErrorString(free_err));
    fflush(stderr);

    CUcontext cu_ctx = nullptr;
    CUresult ctx_res = cuCtxGetCurrent(&cu_ctx);
    fprintf(stderr, "[Engram-OptiX] cuCtxGetCurrent returned: %d, cu_ctx = %p\n", (int)ctx_res, (void*)cu_ctx);
    fprintf(stderr, "[Engram-OptiX] optixDeviceContextCreate pointer = %p\n", (void*)OPTIX_FUNCTION_TABLE_SYMBOL.optixDeviceContextCreate);
    fflush(stderr);

    // Secondary guard: verify the function table was actually populated.
    if (!OPTIX_FUNCTION_TABLE_SYMBOL.optixDeviceContextCreate) {
        fprintf(stderr, "[Engram-OptiX] Function table null after optixInit — PTX arch mismatch? CPU BVH fallback.\n");
        fflush(stderr);
        delete state;
        return nullptr;
    }

    OptixDeviceContextOptions ctx_opts = {};
    ctx_opts.logCallbackFunction = engram_optix_log;
    ctx_opts.logCallbackLevel    = 4;
    ctx_opts.validationMode      = OPTIX_DEVICE_CONTEXT_VALIDATION_MODE_ALL;
    OPTIX_CHECK_NULL(optixDeviceContextCreate(cu_ctx, &ctx_opts, &state->context));

    OptixModuleCompileOptions mod_opts = {};
    mod_opts.optLevel   = OPTIX_COMPILE_OPTIMIZATION_DEFAULT;
    mod_opts.debugLevel = OPTIX_COMPILE_DEBUG_LEVEL_NONE;

    OptixPipelineCompileOptions pipe_opts = {};
    pipe_opts.usesMotionBlur                   = 0;
    pipe_opts.traversableGraphFlags            = OPTIX_TRAVERSABLE_GRAPH_FLAG_ALLOW_SINGLE_GAS;
    pipe_opts.numPayloadValues                 = 2;
    pipe_opts.numAttributeValues               = 0;
    pipe_opts.usesPrimitiveTypeFlags           = OPTIX_PRIMITIVE_TYPE_FLAGS_CUSTOM;  // renamed in OptiX 9
    pipe_opts.pipelineLaunchParamsVariableName = "params";
    pipe_opts.pipelineLaunchParamsSizeInBytes = sizeof(EngramLaunchParams);
    char log[4096];
    size_t log_size = sizeof(log);

    auto compile_mod = [&](const char* ptx, OptixModule* mod) -> bool {
        log_size = sizeof(log);
        OptixResult r = optixModuleCreate(
            state->context, &mod_opts, &pipe_opts,
            ptx, strlen(ptx), log, &log_size, mod);
        if (r != OPTIX_SUCCESS) {
            fprintf(stderr, "[Engram-OptiX] Module compile failed: %s\n", log);
            return false;
        }
        return true;
    };

    OptixModule mod_is, mod_rg, mod_ah, mod_ms;
    fprintf(stderr, "[Engram-OptiX] Compiling IS module...\n"); fflush(stderr);
    if (!compile_mod(ptx_is, &mod_is)) { delete state; return nullptr; }
    fprintf(stderr, "[Engram-OptiX] Compiling RG module...\n"); fflush(stderr);
    if (!compile_mod(ptx_rg, &mod_rg)) { delete state; return nullptr; }
    fprintf(stderr, "[Engram-OptiX] Compiling AH module...\n"); fflush(stderr);
    if (!compile_mod(ptx_ah, &mod_ah)) { delete state; return nullptr; }
    fprintf(stderr, "[Engram-OptiX] Compiling MS module...\n"); fflush(stderr);
    if (!compile_mod(ptx_ms, &mod_ms)) { delete state; return nullptr; }
    fprintf(stderr, "[Engram-OptiX] All modules compiled successfully.\n"); fflush(stderr);

    OptixProgramGroup pg_rg, pg_hg, pg_ms;
    OptixProgramGroupOptions pg_opts = {};

    {
        fprintf(stderr, "[Engram-OptiX] Creating RG program group...\n");
        fprintf(stderr, "[Engram-OptiX] context: %p, optixProgramGroupCreate ptr: %p\n", (void*)state->context, (void*)OPTIX_FUNCTION_TABLE_SYMBOL.optixProgramGroupCreate);
        fprintf(stderr, "[Engram-OptiX] mod_rg: %p, entryName: %s\n", (void*)mod_rg, "__raygen__engram_knn");
        fprintf(stderr, "[Engram-OptiX] sizeof(OptixProgramGroupDesc) = %zu\n", sizeof(OptixProgramGroupDesc));
        fprintf(stderr, "[Engram-OptiX] sizeof(OptixProgramGroupSingleModule) = %zu\n", sizeof(OptixProgramGroupSingleModule));
        fprintf(stderr, "[Engram-OptiX] sizeof(OptixProgramGroupHitgroup) = %zu\n", sizeof(OptixProgramGroupHitgroup));
        fflush(stderr);
        OptixProgramGroupDesc d = {};
        d.kind = OPTIX_PROGRAM_GROUP_KIND_RAYGEN;
        d.raygen.module = mod_rg;
        d.raygen.entryFunctionName = "__raygen__engram_knn";
        log_size = sizeof(log);
        OPTIX_CHECK_NULL(optixProgramGroupCreate(state->context, &d, 1, &pg_opts, log, &log_size, &pg_rg));
    }
    {
        fprintf(stderr, "[Engram-OptiX] Creating HG program group...\n"); fflush(stderr);
        OptixProgramGroupDesc d = {};
        d.kind = OPTIX_PROGRAM_GROUP_KIND_HITGROUP;
        d.hitgroup.moduleAH             = mod_ah;
        d.hitgroup.entryFunctionNameAH  = "__anyhit__engram_collect";
        d.hitgroup.moduleIS             = mod_is;
        d.hitgroup.entryFunctionNameIS  = "__intersection__engram_aabb";
        log_size = sizeof(log);
        OPTIX_CHECK_NULL(optixProgramGroupCreate(state->context, &d, 1, &pg_opts, log, &log_size, &pg_hg));
    }
    {
        fprintf(stderr, "[Engram-OptiX] Creating MS program group...\n"); fflush(stderr);
        OptixProgramGroupDesc d = {};
        d.kind = OPTIX_PROGRAM_GROUP_KIND_MISS;
        d.miss.module = mod_ms;
        d.miss.entryFunctionName = "__miss__engram_knn";
        log_size = sizeof(log);
        OPTIX_CHECK_NULL(optixProgramGroupCreate(state->context, &d, 1, &pg_opts, log, &log_size, &pg_ms));
    }

    fprintf(stderr, "[Engram-OptiX] Program groups created successfully. Creating pipeline...\n"); fflush(stderr);
    OptixProgramGroup all_pgs[] = { pg_rg, pg_hg, pg_ms };
    OptixPipelineLinkOptions link_opts = {};
    link_opts.maxTraceDepth = 1;
    log_size = sizeof(log);
    OPTIX_CHECK_NULL(optixPipelineCreate(state->context, &pipe_opts, &link_opts,
                                         all_pgs, 3, log, &log_size, &state->pipeline));
    
    fprintf(stderr, "[Engram-OptiX] Pipeline created. Setting stack size...\n"); fflush(stderr);
    optixPipelineSetStackSize(state->pipeline, 2048, 2048, 2048, 1);

    // Build GAS
    fprintf(stderr, "[Engram-OptiX] Allocating d_aabb_input for GAS (n_primitives = %u)...\n", n_primitives); fflush(stderr);
    const size_t aabb_bytes = (size_t)n_primitives * 6 * sizeof(float);
    cudaMalloc(reinterpret_cast<void**>(&state->d_aabb_input), aabb_bytes);
    fprintf(stderr, "[Engram-OptiX] Copying AABB input to device...\n"); fflush(stderr);
    cudaMemcpy(reinterpret_cast<void*>(state->d_aabb_input),
               aabb_data, aabb_bytes, cudaMemcpyHostToDevice);

    OptixBuildInput bi = {};
    bi.type = OPTIX_BUILD_INPUT_TYPE_CUSTOM_PRIMITIVES;
    bi.customPrimitiveArray.aabbBuffers   = &state->d_aabb_input;
    bi.customPrimitiveArray.numPrimitives = n_primitives;
    uint32_t flags[1] = { OPTIX_GEOMETRY_FLAG_NONE };
    bi.customPrimitiveArray.flags         = flags;
    bi.customPrimitiveArray.numSbtRecords = 1;

    OptixAccelBuildOptions accel_opts = {};
    accel_opts.buildFlags = OPTIX_BUILD_FLAG_ALLOW_COMPACTION | OPTIX_BUILD_FLAG_PREFER_FAST_TRACE;
    accel_opts.operation  = OPTIX_BUILD_OPERATION_BUILD;

    OptixAccelBufferSizes sizes = {};
    fprintf(stderr, "[Engram-OptiX] Computing accel memory usage...\n"); fflush(stderr);
    OPTIX_CHECK_NULL(optixAccelComputeMemoryUsage(state->context, &accel_opts, &bi, 1, &sizes));
    fprintf(stderr, "[Engram-OptiX] Accel sizes: tempSize = %zu, outputSize = %zu\n", sizes.tempSizeInBytes, sizes.outputSizeInBytes); fflush(stderr);

    CUdeviceptr d_temp, d_out_full;
    cudaMalloc(reinterpret_cast<void**>(&d_temp),     sizes.tempSizeInBytes);
    cudaMalloc(reinterpret_cast<void**>(&d_out_full), sizes.outputSizeInBytes);

    OptixTraversableHandle h_full;
    fprintf(stderr, "[Engram-OptiX] Building initial GAS...\n"); fflush(stderr);
    OPTIX_CHECK_NULL(optixAccelBuild(state->context, nullptr, &accel_opts,
                                     &bi, 1,
                                     d_temp, sizes.tempSizeInBytes,
                                     d_out_full, sizes.outputSizeInBytes,
                                     &h_full, nullptr, 0));

    // Compact
    fprintf(stderr, "[Engram-OptiX] Allocating compact size pointer...\n"); fflush(stderr);
    CUdeviceptr d_compact_size;
    cudaMalloc(reinterpret_cast<void**>(&d_compact_size), sizeof(size_t));
    OptixAccelEmitDesc emit = { d_compact_size, OPTIX_PROPERTY_TYPE_COMPACTED_SIZE };
    fprintf(stderr, "[Engram-OptiX] Emitting compacted size...\n"); fflush(stderr);
    optixAccelBuild(state->context, nullptr, &accel_opts, &bi, 1,
                    d_temp, sizes.tempSizeInBytes,
                    d_out_full, sizes.outputSizeInBytes,
                    &h_full, &emit, 1);
    fprintf(stderr, "[Engram-OptiX] Freeing d_temp...\n"); fflush(stderr);
    cudaFree(reinterpret_cast<void*>(d_temp));
    size_t compact_size = 0;
    fprintf(stderr, "[Engram-OptiX] Copying compact size from device...\n"); fflush(stderr);
    cudaMemcpy(&compact_size, reinterpret_cast<void*>(d_compact_size),
               sizeof(size_t), cudaMemcpyDeviceToHost);
    cudaFree(reinterpret_cast<void*>(d_compact_size));
    fprintf(stderr, "[Engram-OptiX] Compacted GAS size: %zu bytes\n", compact_size); fflush(stderr);

    cudaMalloc(reinterpret_cast<void**>(&state->d_gas_output), compact_size);
    fprintf(stderr, "[Engram-OptiX] Compacting GAS...\n"); fflush(stderr);
    OPTIX_CHECK_NULL(optixAccelCompact(state->context, nullptr,
                                       h_full, state->d_gas_output, compact_size,
                                       &state->traversable));
    fprintf(stderr, "[Engram-OptiX] Freeing d_out_full...\n"); fflush(stderr);
    cudaFree(reinterpret_cast<void*>(d_out_full));

    // SBT
    fprintf(stderr, "[Engram-OptiX] Packing SBT headers...\n"); fflush(stderr);
    struct RaygenRecord  { char h[OPTIX_SBT_RECORD_HEADER_SIZE]; };
    struct MissRecord    { char h[OPTIX_SBT_RECORD_HEADER_SIZE]; };
    struct HgRecord      { char h[OPTIX_SBT_RECORD_HEADER_SIZE]; };

    RaygenRecord rg = {}; optixSbtRecordPackHeader(pg_rg, &rg);
    MissRecord   ms = {}; optixSbtRecordPackHeader(pg_ms, &ms);
    HgRecord     hg = {}; optixSbtRecordPackHeader(pg_hg, &hg);

    fprintf(stderr, "[Engram-OptiX] Allocating SBT records on device...\n"); fflush(stderr);
    cudaMalloc(reinterpret_cast<void**>(&state->d_sbt_raygen),    sizeof(rg));
    cudaMalloc(reinterpret_cast<void**>(&state->d_sbt_miss),      sizeof(ms));
    cudaMalloc(reinterpret_cast<void**>(&state->d_sbt_hitgroup),  sizeof(hg));

    fprintf(stderr, "[Engram-OptiX] Copying SBT records to device...\n"); fflush(stderr);
    cudaMemcpy(reinterpret_cast<void*>(state->d_sbt_raygen),   &rg, sizeof(rg), cudaMemcpyHostToDevice);
    cudaMemcpy(reinterpret_cast<void*>(state->d_sbt_miss),     &ms, sizeof(ms), cudaMemcpyHostToDevice);
    cudaMemcpy(reinterpret_cast<void*>(state->d_sbt_hitgroup), &hg, sizeof(hg), cudaMemcpyHostToDevice);

    state->sbt.raygenRecord                = state->d_sbt_raygen;
    state->sbt.missRecordBase              = state->d_sbt_miss;
    state->sbt.missRecordStrideInBytes     = sizeof(MissRecord);
    state->sbt.missRecordCount             = 1;
    state->sbt.hitgroupRecordBase          = state->d_sbt_hitgroup;
    state->sbt.hitgroupRecordStrideInBytes = sizeof(HgRecord);
    state->sbt.hitgroupRecordCount         = 1;

    fprintf(stderr, "[Engram-OptiX] Pipeline ready: %u primitives, GAS=%.1fKB\n",
            n_primitives, compact_size / 1024.0f);
    return state;
}

int engram_optix_query(
    EngramOptiXPipeline* state,
    const float* positions,
    uint32_t     n_queries,
    uint32_t     max_hits,
    uint64_t*    hit_list_out,
    uint32_t*    hit_counts_out)
{
    if (!state || !state->pipeline) return -1;

    const size_t pos_bytes   = (size_t)n_queries * 3 * sizeof(float);
    const size_t hits_bytes  = (size_t)n_queries * max_hits * sizeof(uint64_t);
    const size_t count_bytes = (size_t)n_queries * sizeof(uint32_t);

    if (n_queries > state->scratch_queries || max_hits > state->scratch_hits) {
        if (state->d_query_pos)  cudaFree(reinterpret_cast<void*>(state->d_query_pos));
        if (state->d_hit_list)   cudaFree(reinterpret_cast<void*>(state->d_hit_list));
        if (state->d_hit_counts) cudaFree(reinterpret_cast<void*>(state->d_hit_counts));
        cudaMalloc(reinterpret_cast<void**>(&state->d_query_pos),   pos_bytes);
        cudaMalloc(reinterpret_cast<void**>(&state->d_hit_list),    hits_bytes);
        cudaMalloc(reinterpret_cast<void**>(&state->d_hit_counts),  count_bytes);
        state->scratch_queries = n_queries;
        state->scratch_hits    = max_hits;
    }

    cudaMemcpy(reinterpret_cast<void*>(state->d_query_pos),
               positions, pos_bytes, cudaMemcpyHostToDevice);
    cudaMemset(reinterpret_cast<void*>(state->d_hit_counts), 0, count_bytes);

    EngramLaunchParams params;
    params.d_query_positions  = reinterpret_cast<float3*>(state->d_query_pos);
    params.traversable        = state->traversable;
    params.d_hit_list         = reinterpret_cast<unsigned long long*>(state->d_hit_list);
    params.d_hit_counts       = reinterpret_cast<unsigned int*>(state->d_hit_counts);
    params.max_hits_per_query = max_hits;
    params.d_aabb_data        = reinterpret_cast<const float*>(state->d_aabb_input);

    CUdeviceptr d_params;
    cudaMalloc(reinterpret_cast<void**>(&d_params), sizeof(params));
    cudaMemcpy(reinterpret_cast<void*>(d_params), &params, sizeof(params), cudaMemcpyHostToDevice);

    OptixResult r = optixLaunch(state->pipeline, nullptr,
                                d_params, sizeof(params),
                                &state->sbt, n_queries, 1, 1);
    cudaFree(reinterpret_cast<void*>(d_params));
    if (r != OPTIX_SUCCESS) return -2;

    cudaDeviceSynchronize();

    cudaMemcpy(hit_list_out,   reinterpret_cast<void*>(state->d_hit_list),
               hits_bytes,  cudaMemcpyDeviceToHost);
    cudaMemcpy(hit_counts_out, reinterpret_cast<void*>(state->d_hit_counts),
               count_bytes, cudaMemcpyDeviceToHost);
    return 0;
}

void engram_optix_free(EngramOptiXPipeline* state) {
    if (!state) return;
    if (state->pipeline)        optixPipelineDestroy(state->pipeline);
    if (state->context)         optixDeviceContextDestroy(state->context);
    if (state->d_gas_output)    cudaFree(reinterpret_cast<void*>(state->d_gas_output));
    if (state->d_aabb_input)    cudaFree(reinterpret_cast<void*>(state->d_aabb_input));
    if (state->d_sbt_raygen)    cudaFree(reinterpret_cast<void*>(state->d_sbt_raygen));
    if (state->d_sbt_miss)      cudaFree(reinterpret_cast<void*>(state->d_sbt_miss));
    if (state->d_sbt_hitgroup)  cudaFree(reinterpret_cast<void*>(state->d_sbt_hitgroup));
    if (state->d_query_pos)     cudaFree(reinterpret_cast<void*>(state->d_query_pos));
    if (state->d_hit_list)      cudaFree(reinterpret_cast<void*>(state->d_hit_list));
    if (state->d_hit_counts)    cudaFree(reinterpret_cast<void*>(state->d_hit_counts));
    delete state;
}

} // extern "C"

#else // stubs

#include <cstdint>
#include <cstdio>
extern "C" {
void* engram_optix_init(const float*, uint32_t, const char*, const char*, const char*, const char*) {
    fprintf(stderr, "[Engram-OptiX] SDK not compiled in — using CPU BVH.\n");
    return nullptr;
}
int  engram_optix_query(void*, const float*, uint32_t, uint32_t, uint64_t*, uint32_t*) { return -1; }
void engram_optix_free(void*) {}
}

#endif
