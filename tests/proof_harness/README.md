# Engram proof harness

CPU-only regression gate for core memory promises.

## What it checks

| Section | Behavior |
|---------|----------|
| Exact Recall@K | Remember a sentence; recall with the same text ranks the concept at top |
| Paraphrase Recall@K | Related wording still surfaces the concept (with closer fallback) |
| Restart continuity | Drop `CpuBackend`, reopen same store dir, marker still recallable |
| Seal corruption | `sig_5` whole-block seal detects flipped payload bytes; rewrite restores Valid; legacy zeros → `legacy_unsealed` |
| Handoff residual | `helper:session_handoff_latest` survives store reopen; optional MCP initialize if `ENGRAM_PROOF_BIN` set |
| Latency / RSS | 32 remembers + 64 recalls; print p50/p95 ms and VmRSS; fail only on absurd ceilings |

## Run

```bash
./scripts/run-proof-harness.sh
# or
cargo run -p engram-proof-harness
```

Exit code **0** only when `PROOF_HARNESS_RESULT=PASS`.

## CI

Job `proof-harness` in `.github/workflows/rust.yml` (required check alongside build-and-test).
