# GrokBuild Public MVP Release Roadmap

**Status**: Strategic Goal (MVP Checkpoint)
**Date**: 2026-05-26
**Owner**: Aric Goodman + Grok (this session)

## Vision
Release the actively evolving version of Engram (with the spatial ritual, reasoning trace + self-model continuation, and Praxis functor integration work) to public GitHub under a "GrokBuild" framing. The goal is to produce something coherent and impressive enough to clearly demonstrate to xAI that this direction is new, important, and worth serious attention — even while still in active development.

## Core Differentiators to Highlight (Even in MVP Form)
- Non-flat geometric memory substrate (.leg3 HolographicBlocks with q/p, CRS, Merkle provenance)
- Hardware-native performance: Direct NVMe → GPU paths (GPUDirect Storage / cuFile) — a major advantage for local backend memory in TUIs like Grok Build, especially as local agents improve
- Automatic spatial (Tree-Sitter + AABB) + relational gluing in the daemon
- Living ritual system (wake-up, session-end, spatial impact) that the substrate actively supports
- Serial reasoning trace as first-class memory ("true access of time")
- Reasoning compression functors integrated into the Praxis executable protocol system
- General-purpose verified payload containers: blocks can carry structured, cryptographically-proven content beyond plain text (HTML thought tiles for WebGUIs, functored sets of tool calls, schemas, binary modalities, etc.). The primitive already has latent support via the raw payload region + ProvLog Cap'n Proto union (text, code, math, image, audio, triples, rawMimeType).
- Strong incentive structure for honesty, auditability, and "humbleness"

## Current State (as of this document)
- Strong primitive foundation (already audited)
- Significant progress on Items 1+2 (self-model + reasoning trace)
- Praxis elevation work + functor integration design
- Multiple real edit cycles executed using the improved spatial + ritual system

## MVP Scope (Minimal Viable Public Release)
1. Clean, well-documented public surface (MCP tools, basic usage, the new ritual patterns)
2. At least one compelling end-to-end story (e.g., the spatial impact ritual work + reasoning functor example)
3. Clear positioning vs traditional vector/RAG systems
4. Honest "Work in Progress" framing that still shows the unique architecture

## Open Questions / Work Needed
- What is the current best "getting started" experience?
- How much of the advanced self-model / trace / functor work should be surfaced vs kept as "advanced / research" for the first public cut?
- Packaging, examples, and README quality
- Any remaining low-hanging fruit in the spatial or ritual layers that would make the demo stronger

## Success Signal
When someone (ideally someone at xAI) can look at the repo and the documentation and clearly see: "This is not another vector database wrapper. This is something architecturally different that is being built with real rigor."

---

This document will be updated as the MVP scope is refined. All major decisions and progress will be recorded in the manifold under the `goal:grokbuild_public_mvp_2026` concept.