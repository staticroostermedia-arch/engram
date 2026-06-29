#!/usr/bin/env python3
"""Track C filesystem + manifest cleanup only. No MCP calls."""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import sys
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from legacy_leg_parse import parse_legacy_leg  # noqa: E402

REPO = Path(__file__).resolve().parent.parent
ORG = REPO / "data/theory-corpus/organized"
MONAD = ORG / "monad-math-research"
THEORY_MANIFEST = REPO / "data/theory-corpus/theory-manifest.jsonl"
FE_CORPUS = Path("/home/a/Documents/BookForge/corpus/false-empire")
FE_MANIFEST = FE_CORPUS / "false-empire-manifest.jsonl"
FE_DEFERRED = FE_CORPUS / "deferred-from-track-c"
FE_ARCHIVES = FE_CORPUS / "archives"
QUAR = ORG / "_quarantine"
FE_QUAR = QUAR / "false-empire-deferred"
JUNK = QUAR / "junk"


def md5_file(p: Path) -> str:
    h = hashlib.md5()
    with p.open("rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def is_false_empire_name(name: str) -> bool:
    low = name.lower()
    return name.startswith("False_Empire") or name.startswith("false_empire") or low.startswith("false empire")


def fe_dest_for(src: Path) -> Path:
    if src.suffix in (".zip", ".tar", ".gz"):
        return FE_ARCHIVES / src.name
    return FE_DEFERRED / src.name


def move_fe_file(src: Path, moved: list[dict]) -> Path:
    """Move one FE file to BookForge; return final destination path."""
    dest = fe_dest_for(src)
    if dest.exists() and md5_file(src) != md5_file(dest):
        dest = dest.parent / f"{dest.stem}_dup{dest.suffix}"
    shutil.move(str(src), str(dest))
    moved.append({"from": str(src), "to": str(dest)})
    return dest


def move_fe_out_of_engram_tree(log: dict) -> None:
    """Move all False Empire artifacts from anywhere under Engram organized/ to BookForge."""
    FE_DEFERRED.mkdir(parents=True, exist_ok=True)
    FE_ARCHIVES.mkdir(parents=True, exist_ok=True)
    moved: list[dict] = []

    # Scan entire organized tree (static-rooster-ops, uncertain-defer, _quarantine, etc.)
    for src in sorted(ORG.rglob("*")):
        if not src.is_file():
            continue
        if not is_false_empire_name(src.name):
            continue
        move_fe_file(src, moved)

    # Remove empty FE quarantine dir if possible
    if FE_QUAR.exists() and not any(FE_QUAR.iterdir()):
        FE_QUAR.rmdir()

    log["fe_moved_from_engram_organized"] = moved


def update_theory_manifest_fe(log: dict) -> None:
    if not THEORY_MANIFEST.exists():
        log["theory_manifest"] = "missing"
        return
    updated = 0
    out: list[str] = []
    for line in THEORY_MANIFEST.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        e = json.loads(line)
        np = e.get("new_path", "") or ""
        title = e.get("title", "") or ""
        is_fe = (
            "False_Empire" in np
            or "False_Empire" in title
            or "false_empire" in np.lower()
            or is_false_empire_name(Path(np).name)
            or is_false_empire_name(title)
        )
        if is_fe and ("theory-corpus/organized" in np or "Engram/data" in np):
            fname = Path(np).name
            resolved: Path | None = None
            for candidate in (FE_DEFERRED / fname, FE_ARCHIVES / fname):
                if candidate.exists():
                    resolved = candidate
                    break
            if resolved is None:
                for base in (FE_DEFERRED, FE_ARCHIVES):
                    for alt in base.glob(f"{Path(fname).stem}*{Path(fname).suffix}"):
                        resolved = alt
                        break
                    if resolved:
                        break
            if resolved:
                e["new_path"] = str(resolved)
            e["pass2_category"] = "track-a-deferred"
            e["engram_ingest"] = False
            e["track"] = "A-deferred"
            if e.get("engram_hub"):
                e.pop("engram_hub", None)
            if e.get("leg_sidecar") and "theory-corpus" in str(e.get("leg_sidecar", "")):
                sc_name = Path(e["leg_sidecar"]).name
                for base in (FE_DEFERRED, FE_ARCHIVES):
                    sc = base / sc_name
                    if sc.exists():
                        e["leg_sidecar"] = str(sc)
                        break
            updated += 1
        out.append(json.dumps(e, ensure_ascii=False))
    THEORY_MANIFEST.write_text("\n".join(out) + "\n", encoding="utf-8")
    log["theory_manifest_fe_updated"] = updated


def dedupe_fe_sidecars(log: dict) -> None:
    removed = []
    for base in (FE_DEFERRED, FE_ARCHIVES):
        if not base.exists():
            continue
        for dup in list(base.glob("*_dup.json")):
            canon_name = dup.name.replace("_dup", "")
            canon = base / canon_name
            if canon.exists():
                dup.unlink()
                removed.append(str(dup))
    log["deduped_sidecars"] = removed


def canonicalize_track_a_non_best(log: dict) -> None:
    if not FE_MANIFEST.exists():
        log["track_a"] = "missing manifest"
        return
    entries = [json.loads(l) for l in FE_MANIFEST.read_text(encoding="utf-8").splitlines() if l.strip()]
    dup_dir = FE_CORPUS / "duplicates"
    dup_dir.mkdir(exist_ok=True)
    moves = []
    for e in entries:
        if e.get("best_copy"):
            continue
        np = e.get("new_path")
        if not np:
            continue
        src = Path(np)
        if not src.exists() or "duplicates" in str(src):
            continue
        if re.search(r"_v\d| \(1\)", src.name):
            dest = dup_dir / src.name
            if dest.exists() and md5_file(src) == md5_file(dest):
                src.unlink()
            elif not dest.exists():
                shutil.move(str(src), str(dest))
            else:
                dest = dup_dir / f"{src.stem}_dup{src.suffix}"
                shutil.move(str(src), str(dest))
            moves.append({"from": str(src), "to": str(dest)})
            e["new_path"] = str(dest)
    FE_MANIFEST.write_text(
        "\n".join(json.dumps(e, ensure_ascii=False) for e in entries) + "\n", encoding="utf-8"
    )
    log["track_a_non_best_moves"] = len(moves)


def quarantine_unparseable_legs(log: dict) -> None:
    JUNK.mkdir(parents=True, exist_ok=True)
    moved = []
    for leg in list(MONAD.glob("*.leg")):
        parsed = parse_legacy_leg(leg)
        if parsed.get("format") == "unknown":
            dest = JUNK / leg.name
            if dest.exists():
                dest = JUNK / f"{leg.stem}_dup{leg.suffix}"
            shutil.move(str(leg), str(dest))
            sc = leg.with_suffix(leg.suffix + "-parse.json")
            if sc.exists():
                sc.unlink()
            moved.append(str(dest))
    log["unparseable_legs_quarantined"] = moved


def regenerate_leg_sidecars(log: dict) -> None:
    updated = 0
    for leg in MONAD.rglob("*.leg"):
        sc = leg.with_suffix(leg.suffix + "-parse.json")
        parsed = parse_legacy_leg(leg)
        sc.write_text(json.dumps(parsed, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        updated += 1
    log["leg_sidecars_regenerated"] = updated


def fe_files_in_organized() -> list[Path]:
    hits: list[Path] = []
    for p in ORG.rglob("*"):
        if p.is_file() and is_false_empire_name(p.name):
            hits.append(p)
    return hits


def fs_verify(log: dict) -> dict:
    fe_monad = [p for p in MONAD.glob("*") if p.is_file() and is_false_empire_name(p.name)]
    fe_organized = fe_files_in_organized()
    vn_best = 0
    if FE_MANIFEST.exists():
        for line in FE_MANIFEST.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            e = json.loads(line)
            if e.get("best_copy") and e.get("new_path"):
                if re.search(r"_v\d| \(1\)", Path(e["new_path"]).name):
                    vn_best += 1
    organized = sum(1 for p in ORG.rglob("*") if p.is_file())
    checks = {
        "false_empire_in_monad_math": len(fe_monad),
        "false_empire_in_organized": len(fe_organized),
        "false_empire_in_organized_paths": [str(p) for p in fe_organized],
        "track_a_best_copy_vn_names": vn_best,
        "organized_files": organized,
    }
    log["fs_verify"] = checks
    return checks


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--scratch", default="/tmp/grok-goal-5879e8737396/implementer")
    args = ap.parse_args()
    scratch = Path(args.scratch)
    scratch.mkdir(parents=True, exist_ok=True)

    log: dict = {"started_at": datetime.now(timezone.utc).isoformat()}
    move_fe_out_of_engram_tree(log)
    dedupe_fe_sidecars(log)
    update_theory_manifest_fe(log)
    canonicalize_track_a_non_best(log)
    quarantine_unparseable_legs(log)
    regenerate_leg_sidecars(log)
    junk = ORG / "_quarantine" / "junk"
    log["unparseable_legs_in_junk"] = [
        str(p) for p in junk.glob("*.leg") if parse_legacy_leg(p).get("format") == "unknown"
    ]
    checks = fs_verify(log)
    log["finished_at"] = datetime.now(timezone.utc).isoformat()

    out = scratch / "track-c-fs-cleanup.json"
    out.write_text(json.dumps(log, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"ok": True, "checks": checks, "log": str(out)}, indent=2))

    fail = (
        checks["false_empire_in_monad_math"] != 0
        or checks["false_empire_in_organized"] != 0
        or checks["track_a_best_copy_vn_names"] != 0
    )
    return 1 if fail else 0


if __name__ == "__main__":
    raise SystemExit(main())