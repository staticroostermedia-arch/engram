#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::question_mark)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::manual_c_str_literals)]

//! CUDA backend for Engram — BVH O(log N) K-NN + parallel cosine kernels.
//!
//! # Architecture
//!
//! ```text
//! CudaBackend
//!   ├── BvhManifold (CPU LBVH tree + entry index)
//!   │     └── filter_cpu() → candidate IDs     O(log N)
//!   ├── GPU kernels (compiled by build.rs)
//!   │     ├── engram_project_8k_to_3d          (Gaussian CSRP)
//!   │     ├── engram_cosine_batch              (Hermitian cosine × N)
//!   │     └── engram_bvh_traverse              (slab traversal)
//!   └── CpuBackend fallback (for machines without CUDA)
//!
//! WgpuBackend (feature = "wgpu-backend")
//!   ├── CpuBackend          — encode / store / forget / list
//!   ├── wgpu::ComputePipeline — INT8 Poincaré hyperbolic search
//!   └── Vec<PackedBlock>   — INT8 host-RAM mirror of every .leg block
//! ```
//!
//! The BVH is built once at startup and rebuilt incrementally as new blocks arrive.
//! On GPU-less machines, `CudaBackend` transparently delegates to `CpuBackend`.

pub mod bvh;
pub mod cuda_dispatch;
#[cfg(engram_backend_cuda)]
pub mod optix_pipeline;
pub mod backend;
pub mod metal_backend;

pub use bvh::{BvhManifold, Float3, LBVHNode};
pub use backend::CudaBackend;
pub use metal_backend::MetalBackend;
pub mod quant;

/// WebGPU INT8 Poincaré backend — available when no CUDA/Metal is detected (build.rs wgpu fallback).
#[cfg(engram_backend_wgpu)]
pub mod wgpu_backend;
#[cfg(engram_backend_wgpu)]
pub use wgpu_backend::WgpuBackend;

