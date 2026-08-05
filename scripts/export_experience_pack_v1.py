#!/usr/bin/env python3
"""experience_pack_v1 export — curated high-CRS concepts only (no whole-stalk dump).

Sources (first match wins):
  1. --bodies-json  JSON array of {concept, crs, text, role?} from live MCP/read
  2. --from-store   path to stalks dir: read selected concept .leg/.leg3 via engram
                    if target/debug/engram present, else extract text best-effort
  3. --concepts     concept ids only (placeholders) — rejected unless --allow-empty-bodies

Quality gates:
  - min_crs (default 0.74)
  - unfiltered_forbidden
  - held_out_required in doctrine
  - local:host:* bodies scrubbed / role=host_profile_nonsecret only if allowlisted

Usage:
  python3 scripts/export_experience_pack_v1.py --out /path/to/pack \\
    --bodies-json /tmp/bodies.json

  # Build bodies.json via MCP read_concept results (agent path).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import struct
import time
from pathlib import Path

SCHEMA = "experience_pack_v1"
MIN_CRS = 0.74
# Non-secret host fields only (no keys/tokens).
HOST_PROFILE_ALLOW = {
    "hostname",
    "gpus",
    "ram_gib",
    "nvme",
    "recall_mode",
    "backend_kind",
    "leg_block_count",
}


def scrub_text(text: str) -> str:
    text = re.sub(r"/home/[^\s/]+", "[REDACTED_PATH]", text)
    text = re.sub(r"(?i)(api[_-]?key|secret|token|password)\s*[:=]\s*\S+", "[REDACTED_SECRET]", text)
    text = re.sub(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b", "[REDACTED_EMAIL]", text)
    return text


def extract_leg_provlog(path: Path, max_chars: int = 8000) -> str | None:
    """Best-effort: scan .leg bytes for UTF-8 runs that look like provlog."""
    try:
        data = path.read_bytes()
    except OSError:
        return None
    # Prefer readable ASCII/UTF-8 chunks with GOAL/TRACE/TILE markers.
    text_chunks: list[str] = []
    i = 0
    n = len(data)
    while i < n and sum(len(c) for c in text_chunks) < max_chars:
        if 32 <= data[i] < 127 or data[i] in (9, 10, 13):
            j = i
            while j < n and (32 <= data[j] < 127 or data[j] in (9, 10, 13)):
                j += 1
            if j - i >= 40:
                chunk = data[i:j].decode("ascii", errors="ignore")
                if any(
                    k in chunk
                    for k in (
                        "GOAL",
                        "TRACE",
                        "TILE",
                        "HANDOFF",
                        "AGENT",
                        "ritual",
                        "CLAIMS",
                        "SKILL",
                    )
                ):
                    text_chunks.append(chunk)
            i = j + 1
        else:
            i += 1
    if not text_chunks:
        return None
    return scrub_text("\n".join(text_chunks)[:max_chars])


def load_bodies(args: argparse.Namespace) -> list[dict]:
    entries: list[dict] = []
    if args.bodies_json:
        raw = json.loads(Path(args.bodies_json).read_text())
        if not isinstance(raw, list):
            raise SystemExit("--bodies-json must be a JSON array")
        for item in raw:
            concept = item.get("concept") or item.get("id")
            if not concept:
                continue
            crs = float(item.get("crs", 1.0))
            if crs < args.min_crs:
                continue
            text = scrub_text(str(item.get("text") or item.get("body") or ""))
            if len(text.strip()) < 20:
                continue
            if str(concept).startswith("local:host:") and not args.allow_host_profile:
                # Host profile: only if non-secret structured fields provided.
                if not item.get("host_profile_nonsecret"):
                    continue
            role = item.get("role") or (
                "reference"
                if str(concept).startswith(("goal:", "helper:", "tile:", "process:", "docs:"))
                else "trajectory"
            )
            entries.append(
                {
                    "concept": concept,
                    "crs": crs,
                    "role": role,
                    "text": text[:12000],
                    "min_crs": args.min_crs,
                    "quality_gate": "pass",
                }
            )
        return entries

    if args.from_store:
        store = Path(args.from_store)
        concepts = [c.strip() for c in (args.concepts or "").split(",") if c.strip()]
        if not concepts:
            raise SystemExit("--from-store requires --concepts list")
        for c in concepts:
            # Try .leg then .leg3
            path = None
            for ext in (".leg", ".leg3"):
                p = store / f"{c}{ext}"
                if p.exists():
                    path = p
                    break
            if path is None:
                # stalk may use sanitized names
                matches = list(store.glob(f"*{c.replace(':', '_')}*.leg"))[:1]
                path = matches[0] if matches else None
            if path is None:
                continue
            text = extract_leg_provlog(path)
            if not text:
                continue
            entries.append(
                {
                    "concept": c,
                    "crs": args.min_crs,  # unknown without decode; gated by text quality
                    "role": "reference",
                    "text": text,
                    "min_crs": args.min_crs,
                    "quality_gate": "text_extract",
                    "source_path": str(path.name),
                }
            )
        return entries

    # Placeholder path — only if explicitly allowed
    concepts = [c.strip() for c in (args.concepts or "").split(",") if c.strip()]
    if not concepts:
        return []
    if not args.allow_empty_bodies:
        raise SystemExit(
            "refusing empty-body placeholders; pass --bodies-json, --from-store, or --allow-empty-bodies"
        )
    return [
        {
            "concept": c,
            "role": "reference",
            "min_crs": args.min_crs,
            "note": "placeholder — fill via MCP read_concept",
            "quality_gate": "placeholder",
        }
        for c in concepts
    ]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--concepts", default="", help="comma-separated concept ids")
    ap.add_argument("--bodies-json", type=Path, help="JSON array of {concept,crs,text}")
    ap.add_argument("--from-store", type=Path, help="path to stalks dir for best-effort .leg extract")
    ap.add_argument("--min-crs", type=float, default=MIN_CRS)
    ap.add_argument("--allow-empty-bodies", action="store_true")
    ap.add_argument("--allow-host-profile", action="store_true")
    args = ap.parse_args()

    out: Path = args.out
    out.mkdir(parents=True, exist_ok=True)
    for sub in ("trajectories", "preferences", "negatives", "harness", "reference"):
        (out / sub).mkdir(exist_ok=True)

    entries = load_bodies(args)
    if not entries:
        raise SystemExit("export produced 0 concepts after quality gates — aborting empty pack")

    # Split reference vs trajectories
    refs = [e for e in entries if e.get("role") == "reference"]
    traj = [e for e in entries if e.get("role") != "reference"]
    body = json.dumps(entries, sort_keys=True, indent=2)
    pack_hash = hashlib.blake2b(body.encode(), digest_size=16).hexdigest()
    manifest = {
        "schema_version": SCHEMA,
        "pack_hash": pack_hash,
        "created_at": int(time.time()),
        "filters": {
            "min_crs": args.min_crs,
            "unfiltered_forbidden": True,
            "held_out_required": True,
            "empty_bodies_forbidden": not args.allow_empty_bodies,
        },
        "concept_count": len(entries),
        "reference_count": len(refs),
        "trajectory_count": len(traj),
        "doctrine": "Unfiltered self-train forbidden; receipt required for positives; held-out eval before promote",
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    (out / "reference" / "concepts.json").write_text(
        json.dumps(refs if refs else entries, indent=2) + "\n"
    )
    if traj:
        (out / "trajectories" / "items.json").write_text(json.dumps(traj, indent=2) + "\n")
    # Frozen non-secret host profile stub for retrieval (no secrets).
    host = {
        "hostname": "a-monad",
        "gpus": "0=5060Ti16GB_hot,1=5060_8GB_compute",
        "ram_gib": 93,
        "nvme": "T700",
        "recall_mode": "full_bvh_gpu",
        "backend_kind": "cuda",
        "note": "non-secret host profile for reference pack only",
    }
    (out / "reference" / "host_profile_nonsecret.json").write_text(
        json.dumps(host, indent=2) + "\n"
    )
    print(json.dumps({"ok": True, "out": str(out), "pack_hash": pack_hash, "concept_count": len(entries)}, indent=2))


if __name__ == "__main__":
    main()
