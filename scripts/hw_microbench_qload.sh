#!/usr/bin/env bash
# Microbench: time O_DIRECT reads of first 64KiB (q-vector) of N .leg files.
# Does not claim cuFile DMA success; reports env + optional note to poll readiness.
# Usage: scripts/hw_microbench_qload.sh [store] [N] [out_file]
set -euo pipefail
STORE="${1:-${ENGRAM_STORE:-$HOME/.engram/stalks/}}"
STORE="${STORE/#\~/$HOME}"
N="${2:-256}"
OUT="${3:-}"

report() {
  echo "=== hw_microbench_qload $(date -Iseconds) ==="
  echo "STORE=$STORE N=$N Q_BYTES=65536"
  echo "ENGRAM_CUFILE_HOT=${ENGRAM_CUFILE_HOT:-unset}"
  shopt -s nullglob
  mapfile -t files < <(find "$STORE" -maxdepth 1 \( -name '*.leg' -o -name '*.leg3' \) 2>/dev/null | head -n "$N")
  COUNT=${#files[@]}
  echo "files_found=$COUNT"
  if (( COUNT == 0 )); then
    echo "SKIP: no .leg/.leg3 in store — run after stalk has blocks"
    echo "=== end ==="
    return 0
  fi
  # Python timing: open O_DIRECT when possible, else normal read first 64k
  python3 - <<'PY' "$STORE" "$COUNT" "${files[@]}"
import os, sys, time, statistics
store, count = sys.argv[1], int(sys.argv[2])
paths = sys.argv[3:]
Q = 65536
lat = []
ok = 0
for p in paths:
    t0 = time.perf_counter()
    try:
        # O_DIRECT may fail on some FS; fall back
        flags = os.O_RDONLY
        if hasattr(os, "O_DIRECT"):
            try:
                fd = os.open(p, flags | os.O_DIRECT)
            except OSError:
                fd = os.open(p, flags)
        else:
            fd = os.open(p, flags)
        try:
            data = os.read(fd, Q)
            if len(data) >= Q or len(data) > 0:
                ok += 1
        finally:
            os.close(fd)
    except OSError as e:
        print(f"err {p}: {e}", file=sys.stderr)
        continue
    lat.append((time.perf_counter() - t0) * 1000.0)
if not lat:
    print("success_rate=0")
    print("SKIP: no successful reads")
else:
    lat.sort()
    def pct(p):
        i = min(len(lat)-1, int(round((p/100.0)*(len(lat)-1))))
        return lat[i]
    print(f"success={ok}/{len(paths)} rate={ok/len(paths):.3f}")
    print(f"p50_ms={pct(50):.3f}")
    print(f"p95_ms={pct(95):.3f}")
    print(f"min_ms={lat[0]:.3f} max_ms={lat[-1]:.3f}")
    print("path_label=host_read_O_DIRECT_or_fallback (not cufile_dma unless separate DMA probe)")
    print("note=cuFile DMA requires ENGRAM_CUFILE_HOT=1 + successful cufile_direct_read_to_device")
PY
  echo "=== end ==="
}

if [[ -n "$OUT" ]]; then
  report | tee "$OUT"
else
  report
fi
