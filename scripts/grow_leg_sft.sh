#!/usr/bin/env bash
# Continuity-per-token: refresh disk leg_corpus packs → SFT JSONL (no chat dumps).
# Prefer MCP leg_corpus build first (writes disk_export_path); this only re-exports.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IN="${1:-$ROOT/data/lora-export/training_corpus_leg_geometry_v1_batch.json}"
OUT="${2:-$ROOT/data/lora-export/leg_geometry_sft.jsonl}"
EXTRA="${3:-$ROOT/data/lora-export/extra_training_tuples.jsonl}"
if [[ ! -f "$IN" ]]; then
  echo "missing pack batch: $IN (run mcp_engram_leg_corpus build first)" >&2
  exit 2
fi
ARGS=(--input "$IN" --output "$OUT")
[[ -f "$EXTRA" ]] && ARGS+=(--extra-tuples "$EXTRA")
python3 "$ROOT/scripts/export_leg_corpus_jsonl.py" "${ARGS[@]}"
wc -l "$OUT"
