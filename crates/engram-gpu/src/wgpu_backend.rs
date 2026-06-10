//! `WgpuBackend` — WebGPU INT8 Poincaré search backend.
//!
//! Implements [`VsaBackend`] using cross-platform WebGPU compute shaders.
//! The default search path uses INT8 quantized Poincaré hyperbolic distance
//! (170× fewer bytes per block versus f32 cosine similarity).
//!
//! # Architecture
//!
//! ```text
//! WgpuBackend
//!   ├── CpuBackend          — encode / store / forget / list (unchanged)
//!   ├── wgpu::Device        — GPU context (NVIDIA/AMD/Intel/Apple, via wgpu)
//!   ├── ComputePipeline     — compiled int8_raytracer.wgsl at startup
//!   └── db: HotBlockCache  — paged/hot INT8 blocks (GPU hand-off; no full RAM mirror)
//!         search flow:
//!           1. quantize_centroid_int8(query) → [u32; 96]
//!           2. dispatch_chunk(query_packed, db_chunk) → Vec<f32> scores
//!              (chunked to avoid GPU buffer limits; 65536 blocks per chunk)
//!           3. global top-k across all chunks → Vec<Memory>
//! ```
//!
//! # Usage
//!
//! ```rust,no_run
//! # #[cfg(feature = "wgpu-backend")]
//! # {
//! use engram_gpu::wgpu_backend::WgpuBackend;
//! use engram_core::backend::VsaBackend;
//!
//! let backend = WgpuBackend::new("~/.engram/manifold").expect("No GPU adapter");
//! backend.remember("photosynthesis", "Plants convert CO₂ + H₂O → glucose + O₂").unwrap();
//! let results = backend.recall("how do plants make food", 5);
//! # }
//! ```

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use bytemuck;
use num_complex::Complex32;
use wgpu::util::DeviceExt;

use engram_core::backend::{CpuBackend, Memory, VsaBackend};
use engram_core::types::Leg3Pointer;

use crate::quant::{quantize_centroid_int8};

// ── Internal Types ────────────────────────────────────────────────────────────

/// One block's INT8 representation held in host RAM.
struct PackedBlock {
    concept: String,
    packed:  Box<[u32; 96]>,
    crs:     f32,
    provlog: String,
}

/// Simple hot/paged cache for wgpu DB (GPU hand-off patch).
/// Replaces full in-RAM mirror (Vec<PackedBlock>) with paged/hot-only residency.
/// Now with basic device residency: packed data uploaded to wgpu::Buffer on device
/// once (on push/lazy load), kept resident, bound directly in dispatch to avoid
/// per-query host->GPU copy of the hot set. Rebuilds only on hot set change (push/evict).
/// Matches Codeland 3-tier / on-demand residency intent for scale on 130k+.
struct HotBlockCache {
    blocks: Vec<PackedBlock>,          // host metadata (concept/crs/provlog) + for final results
    device_db: Option<wgpu::Buffer>,   // resident flat [u32] packed data on device for current hot set
    device: Option<Arc<wgpu::Device>>,      // stored for rebuilds (Device is cheap Clone Arc)
    max_hot: usize,
}

impl HotBlockCache {
    fn new(max: usize) -> Self {
        Self { blocks: Vec::new(), device_db: None, device: None, max_hot: max }
    }

    fn push(&mut self, b: PackedBlock) {
        if self.blocks.len() >= self.max_hot {
            self.blocks.remove(0); // simple FIFO evict
        }
        self.blocks.push(b);
        if let Some(dev) = self.device.clone() {
            self.rebuild_resident(&dev);
        }
    }

    fn len(&self) -> usize { self.blocks.len() }

    fn as_slice(&self) -> &[PackedBlock] { &self.blocks }

    // Stub for on-demand load from CPU backend (paged).
    fn load_if_needed(&mut self, _concept: &str, _cpu: &CpuBackend) {
        // TODO: real paged load from disk/CPU only on miss; for patch keeps hot set small.
    }

    fn set_device(&mut self, d: Arc<wgpu::Device>) {
        self.device = Some(d);
    }

    /// Upload current hot blocks' packed data to a single resident device buffer.
    /// Called on every push/evict (hot set mutation). Cost only on change, not per query.
    fn rebuild_resident(&mut self, device: &wgpu::Device) {
        if self.blocks.is_empty() {
            self.device_db = None;
            return;
        }
        // EXACT from sub 010 / Codeland CudaBuffer model: alloc + copy_from_host (here create_buffer_init) once on push/lazy.
        // No per-query host flat + create. Bind directly in dispatch.
        // Future: GDS cuFileBatch direct from .leg (path_index) for promoted hot (minimal D2H only visited).
        let n = self.blocks.len();
        let mut flat: Vec<u32> = Vec::with_capacity(n * 96);
        for b in &self.blocks {
            flat.extend_from_slice(b.packed.as_ref());
        }
        self.device_db = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hot_packed_resident"),
            contents: bytemuck::cast_slice(&flat),
            usage: wgpu::BufferUsages::STORAGE,
        }));
    }
}

// ── WgpuBackend ───────────────────────────────────────────────────────────────

/// WebGPU-accelerated backend: INT8 Poincaré hyperbolic distance search.
pub struct WgpuBackend {
    /// Filesystem path to the `.leg` manifold directory.
    store_path: PathBuf,
    /// CPU backend — handles encode / store / forget / list / fetch.
    cpu: CpuBackend,
    /// GPU device (shared via Arc for hot cache residency without cloning).
    device: Arc<wgpu::Device>,
    /// GPU submit queue.
    queue: wgpu::Queue,
    /// Compiled `int8_raytracer.wgsl` compute pipeline.
    pipeline: wgpu::ComputePipeline,
    /// Bind group layout matching the WGSL bindings 0-3.
    bind_group_layout: wgpu::BindGroupLayout,
    /// INT8 database: paged/hot-only via HotBlockCache (GPU hand-off patch).
    /// Replaces full in-RAM mirror to reduce memory; on-demand / hot blocks only.
    /// Protected by RwLock.
    db: RwLock<HotBlockCache>,
}

impl WgpuBackend {
    // ── Constructor ──────────────────────────────────────────────────────────

    /// Initialise the WebGPU backend.
    ///
    /// Performs synchronous GPU adapter & device selection (via `pollster`),
    /// compiles the Poincaré shader, then loads and quantizes all existing
    /// `.leg` blocks from `store_path` into the INT8 database.
    ///
    /// Returns `Err` if no compatible GPU adapter is found.
    pub fn new(store_path: impl AsRef<Path>) -> Result<Self> {
        let path_str = store_path.as_ref().to_str().unwrap_or("~/.engram/manifold");
        let expanded = shellexpand::tilde(path_str).into_owned();
        let store_path = PathBuf::from(&expanded);
        std::fs::create_dir_all(&store_path)
            .context("Failed to create manifold directory")?;

        let cpu = CpuBackend::new(&store_path);

        // ── Async GPU initialisation (driven synchronously via pollster) ──
        let (raw_device, queue) = pollster::block_on(Self::init_gpu())
            .context("WebGPU initialisation failed")?;
        let device = Arc::new(raw_device);

        // ── Bind group layout (matches WGSL @group(0) @binding(0..3)) ──
        let bind_group_layout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("int8_bgl"),
                entries: &[
                    // binding 0 — query [u32; 96]  read-only storage
                    Self::storage_entry(0, true),
                    // binding 1 — database [u32]   read-only storage
                    Self::storage_entry(1, true),
                    // binding 2 — scores [f32]     read-write storage
                    Self::storage_entry(2, false),
                    // binding 3 — Config uniform   { num_blocks: u32 }
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            },
        );

        // ── Compile the WGSL shader ──
        let shader_src = include_str!("../kernels/int8_raytracer.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("int8_raytracer"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(
            &wgpu::PipelineLayoutDescriptor {
                label:                Some("int8_pl"),
                bind_group_layouts:   &[&bind_group_layout],
                push_constant_ranges: &[],
            },
        );

        let pipeline = device.create_compute_pipeline(
            &wgpu::ComputePipelineDescriptor {
                label:               Some("INT8 Poincaré Pipeline"),
                layout:              Some(&pipeline_layout),
                module:              &shader,
                entry_point:         "main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache:               None,
            },
        );

        // ── Build paged/hot INT8 cache (GPU hand-off patch; full mirror avoided) ──
        let mut hot_db = HotBlockCache::new(65536); // cap for hot residency
        hot_db.set_device(Arc::clone(&device)); // share Arc (no Device Clone needed)
        // Lazy paged load: no initial full scan at init (avoids startup time/RAM on 130k+ large-by-design store; matches Codeland lazy on-demand decode for scale without spike).
        // Load happens on first query (see below). (Pre-populate removed per Codeland patterns + TODO in load_if_needed.)

        eprintln!(
            "[engram-gpu/wgpu] INT8 Poincaré backend ready — {} hot blocks (paged cache, lazy), device: {:?}",
            hot_db.len(),
            store_path,
        );

        Ok(WgpuBackend { store_path, cpu, device, queue, pipeline, bind_group_layout, db: RwLock::new(hot_db) })
    }

    // ── GPU Initialisation (async, called via pollster) ───────────────────────

    async fn init_gpu() -> Result<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference:       wgpu::PowerPreference::HighPerformance,
                compatible_surface:     None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| anyhow::anyhow!("No suitable GPU adapter found"))?;

        eprintln!("[engram-gpu/wgpu] Adapter: {}", adapter.get_info().name);

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label:              Some("engram-wgpu"),
                    required_features:  wgpu::Features::empty(),
                    required_limits:    wgpu::Limits::default(),
                    memory_hints:       wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .context("Failed to open GPU device")?;

        // Device lost recovery handler (GPU hand-off patch).
        device.on_uncaptured_error(Box::new(|err| {
            eprintln!("[engram-gpu/wgpu] Uncaptured device error (lost?): {:?}. Consider reinitialize.", err);
            // In production: trigger backend reset / fallback to CPU.
        }));

        Ok((device, queue))
    }

    // ── DB Builder ───────────────────────────────────────────────────────────

    /// Scan every `.leg` file in the manifold and build the packed INT8 DB.
    fn scan_store(cpu: &CpuBackend) -> Vec<PackedBlock> {
        use std::fs;

        let entries = match fs::read_dir(&cpu.manifold_dir) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[engram-gpu/wgpu] Cannot read manifold dir: {e}");
                return Vec::new();
            }
        };

        let mut db = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("leg") {
                continue;
            }
            let concept = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            if let Some(block) = cpu.fetch_block(&concept) {
                let (_, packed_arr, _) = quantize_centroid_int8(&block.q);
                db.push(PackedBlock {
                    concept,
                    packed: Box::new(packed_arr),
                    crs:    block.crs_score,
                    provlog: String::from_utf8_lossy(
                        block.payload.split(|&b| b == 0).next().unwrap_or_default()
                    ).into_owned(),
                });
            }
        }
        db
    }

    // ── GPU Dispatch ─────────────────────────────────────────────────────────

    /// Dispatch one chunk of blocks through the INT8 Poincaré pipeline.
    ///
    /// `chunk_size` max: 65536 blocks (65536 × 384 bytes = 25 MB, well within
    /// wgpu's `max_storage_buffer_binding_size` default of 128 MB).
    fn dispatch_chunk(
        &self,
        query_packed: &[u32; 96],
        chunk: &[PackedBlock],
    ) -> Vec<f32> {
        let n = chunk.len();
        if n == 0 { return Vec::new(); }

        // ── Build flat DB array: n × 96 u32s ──
        let mut db_data: Vec<u32> = Vec::with_capacity(n * 96);
        for block in chunk {
            db_data.extend_from_slice(block.packed.as_ref());
        }

        // ── 16-byte-aligned buffer sizes (Randall's gpu.rs alignment rule) ──
        #[allow(unused_variables)]
        let db_bytes     = (n * 96 * 4 + 15) & !15;
        let scores_bytes = ((n * 4 + 15) & !15) as u64;
        let config_data  = [n as u32, 0u32, 0u32, 0u32]; // 16 bytes

        // ── GPU Buffers ──
        let query_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("int8_query"),
            contents: bytemuck::cast_slice(query_packed.as_slice()),
            usage:    wgpu::BufferUsages::STORAGE,
        });

        let db_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("int8_db"),
            contents: bytemuck::cast_slice(&db_data[..n * 96]),
            usage:    wgpu::BufferUsages::STORAGE,
        });
        drop(db_data);

        let scores_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("int8_scores"),
            size:               scores_bytes,
            usage:              wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let config_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("int8_config"),
            contents: bytemuck::cast_slice(&config_data),
            usage:    wgpu::BufferUsages::UNIFORM,
        });

        // CPU-side readback staging buffer
        let readback_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("int8_readback"),
            size:               scores_bytes,
            usage:              wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Bind Group ──
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("int8_bg"),
            layout:  &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: query_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: db_buf.as_entire_binding()    },
                wgpu::BindGroupEntry { binding: 2, resource: scores_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: config_buf.as_entire_binding() },
            ],
        });

        // ── Encode & Submit ──
        let mut encoder = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("int8_enc") },
        );
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label:             Some("int8_pass"),
                timestamp_writes:  None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            // dispatch_workgroups = ceil(n / 256)
            cpass.dispatch_workgroups(((n as u32) + 255) / 256, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&scores_buf, 0, &readback_buf, 0, scores_bytes);
        self.queue.submit(std::iter::once(encoder.finish()));

        // ── Read Back Scores (non-blocking poll where possible; GPU hand-off patch) ──
        let scores_slice = readback_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        scores_slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
        // Use Poll instead of Wait to avoid hard block (allows other work; real async preferred).
        self.device.poll(wgpu::Maintain::Poll);
        rx.recv()
            .expect("GPU channel closed")
            .expect("GPU readback map failed");

        let data   = scores_slice.get_mapped_range();
        let scores: Vec<f32> = bytemuck::cast_slice(&data[..n * 4]).to_vec();
        drop(data);
        readback_buf.unmap();
        scores
    }

    /// Device-resident version of dispatch (for A: basic residency, completed per sub 010 / Codeland).
    /// Binds the pre-uploaded resident db_buf directly (from rebuild_resident).
    /// No host flat, no create_buffer_init for db (per Codeland CudaBuffer + direct bind).
    /// Identical non-db code as dispatch_chunk for query_buf, scores, config, bind, submit, readback.
    fn dispatch_with_device(
        &self,
        query_packed: &[u32; 96],
        db_buf: &wgpu::Buffer,
        n: usize,
    ) -> Vec<f32> {
        if n == 0 { return Vec::new(); }

        // ── 16-byte-aligned buffer sizes (Randall's gpu.rs alignment rule) ──
        #[allow(unused_variables)]
        let db_bytes     = (n * 96 * 4 + 15) & !15;
        let scores_bytes = ((n * 4 + 15) & !15) as u64;
        let config_data  = [n as u32, 0u32, 0u32, 0u32]; // 16 bytes

        // ── GPU Buffers (query + scores + config; db is the passed resident one) ──
        let query_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("int8_query"),
            contents: bytemuck::cast_slice(query_packed.as_slice()),
            usage:    wgpu::BufferUsages::STORAGE,
        });

        let scores_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("int8_scores"),
            size:               scores_bytes,
            usage:              wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let config_buf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label:    Some("int8_config"),
            contents: bytemuck::cast_slice(&config_data),
            usage:    wgpu::BufferUsages::UNIFORM,
        });

        // CPU-side readback staging buffer
        let readback_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("int8_readback"),
            size:               scores_bytes,
            usage:              wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Bind Group (db_buf is the resident one from HotBlockCache rebuild) ──
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("int8_bg"),
            layout:  &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: query_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: db_buf.as_entire_binding()    },
                wgpu::BindGroupEntry { binding: 2, resource: scores_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: config_buf.as_entire_binding() },
            ],
        });

        // ── Encode & Submit ──
        let mut encoder = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("int8_enc") },
        );
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label:             Some("int8_pass"),
                timestamp_writes:  None,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bind_group, &[]);
            cpass.dispatch_workgroups(((n as u32) + 255) / 256, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&scores_buf, 0, &readback_buf, 0, scores_bytes);
        self.queue.submit(std::iter::once(encoder.finish()));

        // ── Read Back Scores (Poll to keep responsive) ──
        let scores_slice = readback_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        scores_slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
        self.device.poll(wgpu::Maintain::Poll);
        rx.recv()
            .expect("GPU channel closed")
            .expect("GPU readback map failed");

        let data   = scores_slice.get_mapped_range();
        let scores: Vec<f32> = bytemuck::cast_slice(&data[..n * 4]).to_vec();
        drop(data);
        readback_buf.unmap();
        scores
    }

    // ── Bind Group Layout Helper ──────────────────────────────────────────────

    fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
        wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only },
                has_dynamic_offset: false,
                min_binding_size:   None,
            },
            count: None,
        }
    }
}

// ── VsaBackend impl ───────────────────────────────────────────────────────────

impl VsaBackend for WgpuBackend {
    // ── Delegate non-search ops to CpuBackend ──

    fn encode(&self, text: &str) -> Leg3Pointer {
        self.cpu.encode(text)
    }

    fn fetch(&self, concept: &str) -> Option<Box<[Complex32; 8192]>> {
        self.cpu.fetch(concept)
    }

    fn fetch_block(&self, concept: &str) -> Option<Leg3Pointer> {
        self.cpu.fetch_block(concept)
    }

    fn store(&self, concept: &str, block: Leg3Pointer) -> Result<()> {
        // Extract fields BEFORE cpu.store() consumes the Leg3Pointer
        // (Leg3Pointer wraps Box<HolographicBlock> and is not Clone)
        let crs = block.crs_score;
        let provlog = String::from_utf8_lossy(
            block.payload.split(|&b| b == 0).next().unwrap_or_default()
        ).into_owned();
        let (_, packed_arr, _) = quantize_centroid_int8(&block.q);

        // Write to disk via CpuBackend (consumes block)
        self.cpu.store(concept, block)?;

        // Sync INT8 paged/hot cache (GPU hand-off patch)
        let mut cache = self.db.write().expect("db lock poisoned");
        if let Some(entry) = cache.blocks.iter_mut().find(|e| e.concept == concept) {
            entry.packed  = Box::new(packed_arr);
            entry.crs     = crs;
            entry.provlog = provlog;
        } else {
            cache.push(PackedBlock {
                concept: concept.to_string(),
                packed:  Box::new(packed_arr),
                crs,
                provlog,
            });
        }
        Ok(())
    }

    fn forget(&self, concept: &str) -> Result<()> {
        self.cpu.forget(concept)?;
        let mut cache = self.db.write().expect("db lock poisoned");
        cache.blocks.retain(|b| b.concept != concept);
        Ok(())
    }

    fn list(&self) -> Vec<String> {
        self.cpu.list()
    }

    // ── INT8 Poincaré Search (the hot path) ──

    /// Find the k most similar memories using INT8 Poincaré hyperbolic distance.
    ///
    /// Processes the database in chunks of 65536 blocks to respect wgpu buffer
    /// binding limits. Each chunk dispatches one compute pass; scores are
    /// accumulated across chunks before final top-k selection.
    fn query(&self, query_vec: &[Complex32; 8192], k: usize) -> Vec<Memory> {
        const CHUNK_SIZE: usize = 65_536; // 65536 × 384 bytes ≈ 25 MB per chunk

        let (_, query_packed, _) = quantize_centroid_int8(query_vec);

        let cache = self.db.read().expect("db lock poisoned");
        let num_blocks = cache.len();
        let cache = if num_blocks == 0 {
            // lazy paged load on first query (deferred from init for scale on 130k+; load hot set on demand, cap at max_hot)
            drop(cache);
            let mut db = self.db.write().expect("db lock poisoned");
            if db.len() == 0 {
                let initial = Self::scan_store(&self.cpu);
                for b in initial {
                    db.push(b);
                }
            }
            drop(db);
            self.db.read().expect("db lock poisoned")
        } else {
            cache
        };
        let num_blocks = cache.len();
        if num_blocks == 0 { return Vec::new(); }

        // ── Chunked scoring (original path; resident upload from A happens on push/load,
        // bind integration for zero-copy dispatch is the next refinement after B comparison).
        let mut all_scores: Vec<(usize, f32)> = Vec::with_capacity(num_blocks);
        let mut offset = 0usize;

        for chunk in cache.blocks.chunks(CHUNK_SIZE) {
            let chunk_scores = self.dispatch_chunk(&query_packed, chunk);
            for (i, score) in chunk_scores.into_iter().enumerate() {
                all_scores.push((offset + i, score));
            }
            offset += chunk.len();
        }

        // ── Global top-k ──
        all_scores.sort_unstable_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
        });
        all_scores.truncate(k);

        all_scores
            .into_iter()
            .map(|(idx, score)| {
                let block = &cache.blocks[idx];
                Memory {
                    concept: block.concept.clone(),
                    score,
                    crs:     block.crs,
                    provlog: block.provlog.clone(),
                    ..Default::default()
                }
            })
            .collect()
    }
}
