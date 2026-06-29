#!/usr/bin/env python3
"""Parse legacy CodeLand/monad .leg text artifacts (NOT Engram .leg3)."""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path


def _parse_ldgp_container(raw: str, path: Path) -> dict | None:
    """LDGP multi-section: {"container":"LEG",...} --- {body} --- {lrp}."""
    start = raw.lstrip()
    if not start.startswith('{"container"'):
        return None
    first_line = start.splitlines()[0]
    try:
        meta = json.loads(first_line)
    except json.JSONDecodeError:
        return None
    if meta.get("container") != "LEG":
        return None
    sections = [s.strip() for s in raw.split("---") if s.strip()]
    statement = ""
    title = meta.get("name") or path.stem
    for sec in sections[1:]:
        try:
            obj = json.loads(sec)
            if isinstance(obj, dict) and obj.get("statement"):
                statement = obj["statement"]
                break
            if isinstance(obj, dict) and obj.get("type"):
                statement = json.dumps(obj, ensure_ascii=False)[:1500]
        except json.JSONDecodeError:
            continue
    return {
        "path": str(path),
        "format": "legacy_leg_ldgp_container_v1",
        "header": f"ldgp:{meta.get('name', path.stem)}",
        "title": title[:500],
        "statement_preview": statement[:2000],
        "line_count": len(raw.splitlines()),
        "is_engram_leg3": False,
        "schema": meta.get("schema"),
    }


def _parse_leg_kit_json(raw: str, path: Path) -> dict | None:
    """Self-contained monad kit manifest: {"leg_version", "anchor", "title", "files": [...]}."""
    start = raw.lstrip()
    if not start.startswith("{"):
        return None
    brace = 0
    end = 0
    for i, ch in enumerate(start):
        if ch == "{":
            brace += 1
        elif ch == "}":
            brace -= 1
            if brace == 0:
                end = i + 1
                break
    if end == 0:
        return None
    try:
        obj = json.loads(start[:end])
    except json.JSONDecodeError:
        return None
    if not isinstance(obj, dict) or "leg_version" not in obj or "header" in obj:
        return None
    title = obj.get("title") or obj.get("anchor") or path.stem
    files = obj.get("files") or []
    preview = f"anchor={obj.get('anchor','')}; files={len(files)}"
    return {
        "path": str(path),
        "format": "legacy_leg_kit_v1",
        "header": f"kit:{obj.get('anchor', path.stem)}",
        "title": str(title)[:500],
        "statement_preview": preview[:2000],
        "line_count": len(raw.splitlines()),
        "is_engram_leg3": False,
        "file_count": len(files),
    }


def _parse_spec_yaml(raw: str, path: Path) -> dict | None:
    """SPEC-ROOT style: spec_id: / spec_name: key-value header blocks."""
    if not raw.lstrip().startswith("spec_id:"):
        return None
    fields: dict[str, str] = {}
    for line in raw.splitlines()[:12]:
        if ":" in line and not line.startswith("---") and not line.startswith("---"):
            k, _, v = line.partition(":")
            fields[k.strip()] = v.strip()
    title = fields.get("spec_name") or fields.get("spec_id") or path.stem
    body_idx = raw.find("1. PURPOSE")
    preview = raw[body_idx : body_idx + 1800].strip() if body_idx >= 0 else raw[:1800]
    return {
        "path": str(path),
        "format": "legacy_leg_spec_v1",
        "header": f"spec:{fields.get('spec_id', path.stem)}",
        "title": title[:500],
        "statement_preview": preview[:2000],
        "line_count": len(raw.splitlines()),
        "is_engram_leg3": False,
        "spec_version": fields.get("spec_version"),
    }


def _parse_project_receipt(raw: str, path: Path) -> dict | None:
    if not raw.lstrip().startswith("Project Receipt"):
        return None
    first = raw.splitlines()[0].strip()
    title = first.split("—", 1)[-1].strip().strip('"').strip("“").strip("”") if "—" in first else path.stem
    preview = "\n".join(raw.splitlines()[2:22]).strip()
    return {
        "path": str(path),
        "format": "legacy_leg_project_receipt_v1",
        "header": f"receipt:{path.stem}",
        "title": title[:500],
        "statement_preview": preview[:2000],
        "line_count": len(raw.splitlines()),
        "is_engram_leg3": False,
    }


def _parse_cognitive_leg(raw: str, path: Path) -> dict | None:
    if not raw.lstrip().startswith("=== LEG/COGNITIVE ==="):
        return None
    title = path.stem
    for line in raw.splitlines()[1:8]:
        if line.startswith("title:"):
            title = line.split(":", 1)[1].strip().strip('"')
            break
    outline_idx = raw.find("outline:")
    preview = raw[outline_idx : outline_idx + 1800].strip() if outline_idx >= 0 else raw[:1800]
    return {
        "path": str(path),
        "format": "legacy_leg_cognitive_v1",
        "header": f"cognitive:{path.stem}",
        "title": title[:500],
        "statement_preview": preview[:2000],
        "line_count": len(raw.splitlines()),
        "is_engram_leg3": False,
    }


def _parse_leg_header_end(raw: str, path: Path) -> dict | None:
    stripped = raw.lstrip()
    if not (stripped.startswith("LEG\nHEADER") or stripped.startswith("LEG HEADER")):
        return None
    mode = ""
    for line in raw.splitlines()[:10]:
        if line.startswith("mode:"):
            mode = line.split(":", 1)[1].strip()
            break
    body_idx = raw.find("BODY")
    preview = raw[body_idx : body_idx + 1800].strip() if body_idx >= 0 else raw[:1800]
    return {
        "path": str(path),
        "format": "legacy_leg_header_end_v1",
        "header": f"manifest:{mode or path.stem}",
        "title": mode or path.stem,
        "statement_preview": preview[:2000],
        "line_count": len(raw.splitlines()),
        "is_engram_leg3": False,
    }


def _parse_anchor_yaml(raw: str, path: Path) -> dict | None:
    if not (raw.lstrip().startswith(".leg\nheader:") or raw.lstrip().startswith(".leg header:")):
        return None
    purpose = ""
    for line in raw.splitlines()[:12]:
        if "purpose:" in line:
            purpose = line.split(":", 1)[1].strip().strip('"')
            break
    return {
        "path": str(path),
        "format": "legacy_leg_anchor_yaml_v1",
        "header": f"anchor:{path.stem}",
        "title": purpose or path.stem,
        "statement_preview": raw[:2000],
        "line_count": len(raw.splitlines()),
        "is_engram_leg3": False,
    }


def _parse_json_wrapped(raw: str, path: Path) -> dict | None:
    """Monad/CodeLand JSON envelope: {header, body, footer} optionally followed by LEG://."""
    start = raw.lstrip()
    if not start.startswith("{"):
        return None
    # Some files append LEG:// trailer after closing brace.
    brace = 0
    end = 0
    for i, ch in enumerate(start):
        if ch == "{":
            brace += 1
        elif ch == "}":
            brace -= 1
            if brace == 0:
                end = i + 1
                break
    if end == 0:
        return None
    try:
        obj = json.loads(start[:end])
    except json.JSONDecodeError:
        return None
    if not isinstance(obj, dict) or "header" not in obj:
        return None
    hdr = obj.get("header") or {}
    body = obj.get("body") or {}
    name = hdr.get("name") or path.stem
    title = body.get("title") or body.get("human_summary") or name
    statement = body.get("statement") or body.get("human_summary") or ""
    if isinstance(statement, dict):
        statement = json.dumps(statement, ensure_ascii=False)
    trailer = start[end:].strip()
    leg_id = ""
    if trailer.startswith("LEG://"):
        leg_id = trailer.splitlines()[0].strip()
    return {
        "path": str(path),
        "format": "legacy_leg_json_v1",
        "header": leg_id or f"json:{name}",
        "title": str(title)[:500],
        "statement_preview": str(statement)[:2000],
        "line_count": len(raw.splitlines()),
        "is_engram_leg3": False,
        "envelope": {"name": name, "parent_head": hdr.get("parent_head")},
    }


def _parse_markdown_capsule(raw: str, path: Path) -> dict | None:
    """Markdown .leg capsule: '# .leg — Title' with ## Header / ## Body sections."""
    if not raw.lstrip().startswith("# .leg"):
        return None
    first = raw.splitlines()[0].strip()
    title = first.split("—", 1)[-1].strip() if "—" in first else path.stem
    body_idx = raw.find("## Body")
    preview = raw[body_idx + 7 : body_idx + 2200].strip() if body_idx >= 0 else raw[:2000]
    return {
        "path": str(path),
        "format": "legacy_leg_markdown_v1",
        "header": f"markdown:{path.stem}",
        "title": title[:500],
        "statement_preview": preview[:2000],
        "line_count": len(raw.splitlines()),
        "is_engram_leg3": False,
    }


def _parse_artifacts_ledger(raw: str, path: Path) -> dict | None:
    """JSON ledger index: {"artifacts": {"foo.leg": "hash", ...}}."""
    start = raw.lstrip()
    if not start.startswith('{"artifacts"'):
        return None
    try:
        obj = json.loads(start.splitlines()[0] if "\n" not in start[:500] else start[: start.find("}") + 1])
    except json.JSONDecodeError:
        # multi-line JSON object
        brace = 0
        end = 0
        for i, ch in enumerate(start):
            if ch == "{":
                brace += 1
            elif ch == "}":
                brace -= 1
                if brace == 0:
                    end = i + 1
                    break
        if end == 0:
            return None
        try:
            obj = json.loads(start[:end])
        except json.JSONDecodeError:
            return None
    if not isinstance(obj, dict) or "artifacts" not in obj:
        return None
    arts = obj.get("artifacts") or {}
    keys = list(arts.keys())[:8]
    return {
        "path": str(path),
        "format": "legacy_leg_artifacts_ledger_v1",
        "header": f"ledger:{path.stem}",
        "title": path.stem,
        "statement_preview": f"Artifact ledger with {len(arts)} entries; sample: {', '.join(keys)}",
        "line_count": len(raw.splitlines()),
        "is_engram_leg3": False,
        "artifact_count": len(arts),
    }


def _parse_leg_header_block(raw: str, path: Path) -> dict | None:
    """---LEG HEADER--- / ---BODY--- transcript envelope (also '--- .leg HEADER ---')."""
    stripped = raw.lstrip()
    if not (
        stripped.startswith("---LEG HEADER---")
        or stripped.startswith("--- .leg HEADER ---")
        or stripped.startswith("--- HEADER ---")
        or stripped.startswith("---HEADER---")
    ):
        return None
    project = ""
    for line in raw.splitlines()[1:30]:
        if line.startswith("project:"):
            project = line.split(":", 1)[1].strip()
            break
        if line.startswith("artifact:"):
            project = line.split(":", 1)[1].strip()
    body_idx = raw.find("---BODY---")
    if body_idx < 0:
        body_idx = raw.find("--- BODY ---")
    preview = raw[body_idx + 10 : body_idx + 2100].strip() if body_idx >= 0 else ""
    title = project or path.stem
    return {
        "path": str(path),
        "format": "legacy_leg_header_block_v1",
        "header": f"header-block:{project or path.stem}",
        "title": title[:500],
        "statement_preview": preview[:2000],
        "line_count": len(raw.splitlines()),
        "is_engram_leg3": False,
    }


def _parse_project_doc(raw: str, path: Path) -> dict | None:
    """DeepLaw-style plain header: Project:/Document:/Version: blocks."""
    if not raw.startswith("Project:"):
        return None
    title = path.stem
    doc_line = next((ln for ln in raw.splitlines() if ln.startswith("Document:")), "")
    if doc_line:
        title = doc_line.split(":", 1)[1].strip()
    preview_lines = [ln for ln in raw.splitlines()[4:24] if ln.strip()]
    return {
        "path": str(path),
        "format": "legacy_leg_project_doc_v1",
        "header": f"project-doc:{path.stem}",
        "title": title[:500],
        "statement_preview": "\n".join(preview_lines)[:2000],
        "line_count": len(raw.splitlines()),
        "is_engram_leg3": False,
    }


def _finalize_legacy_result(result: dict) -> dict:
    """Normalize all successful legacy parses to canonical format=legacy_leg_v1."""
    fmt = result.get("format")
    if fmt and fmt != "legacy_leg_v1" and fmt != "unknown":
        result["format_variant"] = fmt
        result["format"] = "legacy_leg_v1"
    return result


def parse_legacy_leg(path: Path) -> dict:
    raw = path.read_text(encoding="utf-8", errors="replace")
    lines = raw.splitlines()
    header = lines[0].strip() if lines else ""

    json_result = _parse_json_wrapped(raw, path)
    if json_result:
        return _finalize_legacy_result(json_result)

    ldgp_result = _parse_ldgp_container(raw, path)
    if ldgp_result:
        return _finalize_legacy_result(ldgp_result)

    kit_result = _parse_leg_kit_json(raw, path)
    if kit_result:
        return _finalize_legacy_result(kit_result)

    spec_result = _parse_spec_yaml(raw, path)
    if spec_result:
        return _finalize_legacy_result(spec_result)

    receipt_result = _parse_project_receipt(raw, path)
    if receipt_result:
        return _finalize_legacy_result(receipt_result)

    cognitive_result = _parse_cognitive_leg(raw, path)
    if cognitive_result:
        return _finalize_legacy_result(cognitive_result)

    header_end_result = _parse_leg_header_end(raw, path)
    if header_end_result:
        return _finalize_legacy_result(header_end_result)

    anchor_result = _parse_anchor_yaml(raw, path)
    if anchor_result:
        return _finalize_legacy_result(anchor_result)

    project_result = _parse_project_doc(raw, path)
    if project_result:
        return _finalize_legacy_result(project_result)

    md_capsule = _parse_markdown_capsule(raw, path)
    if md_capsule:
        return _finalize_legacy_result(md_capsule)

    artifacts_ledger = _parse_artifacts_ledger(raw, path)
    if artifacts_ledger:
        return _finalize_legacy_result(artifacts_ledger)

    header_block = _parse_leg_header_block(raw, path)
    if header_block:
        return _finalize_legacy_result(header_block)

    # LEG:// may appear on first line or after JSON trailer.
    leg_line = next((ln.strip() for ln in lines if ln.strip().startswith("LEG://")), "")
    if leg_line:
        header = leg_line
    elif not header.startswith("LEG://"):
        return {
            "path": str(path),
            "format": "unknown",
            "error": "missing LEG:// header",
            "title": path.stem,
            "is_engram_leg3": False,
        }
    title = ""
    for line in lines[1:6]:
        if line.startswith("TITLE:"):
            title = line.split(":", 1)[1].strip()
            break
    statement = ""
    in_stmt = False
    stmt_lines: list[str] = []
    for line in lines:
        if line.strip() == "STATEMENT (B1)" or line.startswith("STATEMENT"):
            in_stmt = True
            continue
        if in_stmt:
            if line.startswith("PROOF") or line.startswith("REMARKS") or line.startswith("DEPENDENCIES"):
                break
            stmt_lines.append(line)
    statement = "\n".join(stmt_lines).strip()[:2000]
    return {
        "path": str(path),
        "format": "legacy_leg_v1",
        "header": header,
        "title": title or path.stem,
        "statement_preview": statement,
        "line_count": len(lines),
        "is_engram_leg3": False,
    }


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: legacy_leg_parse.py <file.leg> [out.json]", file=sys.stderr)
        return 1
    path = Path(sys.argv[1])
    result = parse_legacy_leg(path)
    out = sys.argv[2] if len(sys.argv) > 2 else None
    text = json.dumps(result, indent=2, ensure_ascii=False)
    if out:
        Path(out).write_text(text, encoding="utf-8")
    else:
        print(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())