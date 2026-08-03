# Patent Notice

**Engram** is a reference implementation of technology covered by the following
pending United States patent application:

> **U.S. Patent Application No. 19/372,256**  
> *Self-Contained Variable File System (.LEG Container Format)*  
> Applicant: **Aric Goodman**, Oregon, USA  
> Filed under 35 U.S.C. § 111(a) — Non-Continuing Utility Patent  
> USPTO Streamlined Claim Set Pilot Program

---

## What Is Patented

The patent covers a **triadic digital container format** — the `.LEG` file — consisting of:

| Section | Element | LEG Implementation |
|---|---|---|
| **Header** (100) | Schema identifier | `magic`, `schema_ver` fields |
| **Header** (100) | Allowed-transformations list | `allowed_transforms[64]` field |
| **Header** (100) | Public verification key | `concept_ref[32]` anchor |
| **Body** (200) | Deterministic, type-tagged payload | `zedos_tag` + `payload[122,584]` |
| **Body** (200) | Geometric phase vectors | `q[8192]`, `p[8192]` (VSA tensors) |
| **Footer** (300) | Cryptographic hash / Merkle chain slots | `footer.sig_0`–`sig_5` (BLAKE3). Reference impl: update chain uses `sig_0`/`sig_1` (q hash); whole-block seal of canonical header+body is stored in `sig_5` when sealing is enabled (see `CLAIMS_LEDGER.md`) |
| **Footer** (300) | Link value to prior container | `footer.merkle_sub_root` |
| **Footer** (300) | Error-detection counters | `footer.error_checks` |

The key inventive step: **self-contained verification and lineage without any external registry**.
Each `.leg` file verifies its own integrity and reconstructs its lineage chain locally,
following link values from container to container without consulting blockchain, database, or index.

---

## This Is a Reference Implementation

This open-source release demonstrates the triadic container format and a functional
memory retrieval system built on top of it. The `.leg` container includes all fields
of the full patented format — including the energetics capsule, ZEDOS epistemic tags,
and Merkle footer — so that files written by this reference implementation remain
forward-compatible with the complete system.

Users familiar with patent claims will recognize that the reference encoder here
(deterministic BLAKE3-based spiral encoding) is a simplified version of the
production encoding pipeline, which implements the geometric phase-vector encoding
described in the body format.

---

## Commercial Licensing

If you wish to:

- Use `.LEG` containers in a **commercial product or service**
- Build a **SaaS or hosted offering** on top of Engram
- Embed this technology in a **proprietary application**
- Obtain rights beyond what AGPL-3.0 permits

**We are actively licensing U.S. Patent Application 19/372,256 and welcome commercial conversations.**

Contact: **StaticRoosterMedia@gmail.com**  
Company: Static Rooster Media — *Aric Goodman, Oregon, USA*

We built this to be used. AGPL protects improvements while enabling broad adoption.
Commercial licenses are available on reasonable terms.

---

## AGPL-3.0 Requirement

This software is licensed under the **GNU Affero General Public License v3.0**.
Any networked service (SaaS, API, hosted tool) built on Engram must make its
complete corresponding source code available to users of that service.

If AGPL obligations are incompatible with your use case, a commercial license is
available. Contact us — we're happy to talk.

---

*Aric Goodman & Static Rooster Media · Patent Pending US19/372,256 · Oregon, USA · StaticRoosterMedia@gmail.com*
