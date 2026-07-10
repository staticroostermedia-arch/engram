#!/usr/bin/env python3
"""Export Engram leg_block_pack_v1 batches to chat/instruction JSONL for LoRA.

Accepts either:
  - leg_corpus_batch_v1 JSON (full MCP mcp_engram_leg_corpus build response)
  - leg_corpus_manifest_v1 JSON (packs or packs_preview)

Does NOT train adapters — only materializes supervised rows from scrubbed_provlog.

Usage:
  # Preferred: full pack dump from engram (after MCP restart on binary with disk export):
  #   mcp_engram_leg_corpus(action=build) → writes
  #   $ENGRAM_LORA_EXPORT_DIR/<corpus>_batch.json  (or data/lora-export/)
  #   response.disk_export_path points at the file (packs omitted from chat).

  python3 scripts/export_leg_corpus_jsonl.py \\
    --input data/lora-export/training_corpus_leg_geometry_v1_batch.json \\
    --output data/lora-export/leg_geometry_sft.jsonl

  # Also append hermies / agent TRAINING tuples if present:
  python3 scripts/export_leg_corpus_jsonl.py \\
    --input data/lora-export/training_corpus_leg_geometry_v1_batch.json \\
    --output data/lora-export/leg_geometry_sft.jsonl \\
    --extra-tuples data/lora-export/extra_training_tuples.jsonl

PEFT train (out of band, example — install peft/transformers yourself):
  # Prefer GPU; hermies server uses -ngl 0 and is embeddings-first.
  # Use this JSONL as --dataset for your chosen SFT trainer; do not claim
  # Engram trained a LoRA until train metrics + adapter path exist.

Env:
  ENGRAM_LORA_EXPORT_DIR   directory for full pack dumps (server-side)
  ENGRAM_LORA_EXPORT_INLINE=1  include packs array in MCP response (default: omit when dump exists)
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Dict, Iterable, List


def load_packs(doc: Dict[str, Any]) -> List[Dict[str, Any]]:
    if "packs" in doc and isinstance(doc["packs"], list):
        return doc["packs"]
    if "packs_preview" in doc and isinstance(doc["packs_preview"], list):
        return doc["packs_preview"]
    # Nested under markdown-extracted blob
    for key in ("result", "data"):
        if key in doc and isinstance(doc[key], dict):
            return load_packs(doc[key])
    return []


def pack_to_row(pack: Dict[str, Any]) -> Dict[str, Any] | None:
    text = (pack.get("scrubbed_provlog") or "").strip()
    if not text:
        return None
    src = pack.get("source_concept") or pack.get("geometry_ref") or "unknown"
    crs = pack.get("crs")
    coh = pack.get("semantic_coherence")
    system = (
        "You are an Engram geometric-memory agent. Answer from non-flat "
        "substrate concepts (FHRR/VSA, CRS≥0.74, ProvLog, Merkle, rituals)."
    )
    user = (
        f"Recall and restate the load-bearing content of corpus pack "
        f"`{src}` (crs={crs}, semantic_coherence={coh}) for training fidelity."
    )
    return {
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
            {"role": "assistant", "content": text[:12000]},
        ],
        "meta": {
            "source_concept": src,
            "crs": crs,
            "semantic_coherence": coh,
            "format": pack.get("format"),
            "zedos_tag": pack.get("zedos_tag"),
        },
    }


def iter_extra_tuples(path: Path) -> Iterable[Dict[str, Any]]:
    if not path.exists():
        return
    with path.open() as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            yield json.loads(line)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--input", "-i", required=True, type=Path)
    ap.add_argument("--output", "-o", required=True, type=Path)
    ap.add_argument("--extra-tuples", type=Path, default=None)
    ap.add_argument("--min-crs", type=float, default=0.74)
    args = ap.parse_args()

    raw = args.input.read_text(encoding="utf-8")
    # Allow markdown-wrapped ```json ... ```
    if "```json" in raw:
        raw = raw.split("```json", 1)[1].split("```", 1)[0]
    doc = json.loads(raw)
    packs = load_packs(doc)
    rows: List[Dict[str, Any]] = []
    skipped = 0
    for p in packs:
        crs = p.get("crs")
        if isinstance(crs, (int, float)) and crs < args.min_crs:
            skipped += 1
            continue
        row = pack_to_row(p)
        if row is None:
            skipped += 1
            continue
        rows.append(row)

    if args.extra_tuples:
        for t in iter_extra_tuples(args.extra_tuples):
            rows.append(t)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("w", encoding="utf-8") as f:
        for r in rows:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")

    print(
        json.dumps(
            {
                "packs_in": len(packs),
                "rows_out": len(rows),
                "skipped": skipped,
                "output": str(args.output),
            }
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
