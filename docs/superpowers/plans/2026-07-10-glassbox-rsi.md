# Glass-Box RSI v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every scheduled Engram loop fire a hybrid parent/child goal with a typed verify packet, and give humans a LEG Browser split-home view of that process.

**Architecture:** Process contract first (parent goals + dual_loop schema + loop prompt rewrites), then LEG `?view=glassbox` reading existing REST (`/api/block`, `/health`, activity). No auto-merge, no LEG-run-loops, optional `/api/glassbox` only if multi-fetch is too slow.

**Tech Stack:** Engram MCP goals (`mcp_engram_goal_*`), `helper:rsi_dual_loop_state`, Python JSON schema tests, vanilla JS SPA (`tools/leg-browser/index.html`), `engram serve` REST, scheduler prompts.

**Spec:** `docs/superpowers/specs/2026-07-10-glassbox-rsi-design.md`

---

## File map

| Path | Responsibility |
|------|----------------|
| `docs/schemas/dual_loop_state_v1.json` | JSON Schema for dual_loop control register |
| `docs/schemas/fire_verify_packet_v1.json` | JSON Schema for per-fire verify payload embedded in child goals |
| `docs/skills/engram-glassbox-rsi.md` | Operator skill: fire lifecycle + typed gates |
| `docs/skills/loop-prompts/dual_rsi_v2.md` | Canonical Dual RSI prompt (goal+verify) |
| `docs/skills/loop-prompts/ship_gate_v2.md` | Canonical Ship Gate prompt |
| `docs/skills/loop-prompts/pr_watch_v2.md` | Canonical PR Watch prompt |
| `docs/skills/loop-prompts/mcp_stale_v2.md` | Canonical MCP Stale prompt |
| `docs/skills/loop-prompts/aliveness_bench_v2.md` | Canonical Aliveness prompt |
| `scripts/validate_dual_loop_schema.py` | Offline schema validator for dual_loop + verify samples |
| `scripts/test_glassbox_schemas.py` | Unit tests for schema validators |
| `tools/leg-browser/index.html` | Glassbox view UI (CSS + HTML shell + `loadGlassbox`) |
| `tools/leg-browser/fixtures/glassbox-sample.json` | Static fixture for offline glassbox smoke |
| `docs/LEG_BROWSER.md` | Document `?view=glassbox` |
| `docs/AGENT_MEMORY_CONTRACT.md` | One short section: fire goals + verify (pointer to skill) |

**Do not create** `/api/glassbox` in Phase B1 unless Task 8 proves multi-fetch is unusable (>3s cold).

---

### Task 1: dual_loop + verify JSON schemas

**Files:**
- Create: `docs/schemas/dual_loop_state_v1.json`
- Create: `docs/schemas/fire_verify_packet_v1.json`
- Create: `scripts/validate_dual_loop_schema.py`
- Create: `scripts/test_glassbox_schemas.py`

- [ ] **Step 1: Write failing test**

Create `scripts/test_glassbox_schemas.py`:

```python
#!/usr/bin/env python3
"""Unit tests for Glass-Box RSI schemas."""
from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

from validate_dual_loop_schema import validate_dual_loop, validate_verify_packet  # noqa: E402


class TestDualLoopSchema(unittest.TestCase):
    def test_minimal_valid(self):
        doc = {
            "version": 1,
            "track_next": "G",
            "track_last": "S",
            "open_pr": None,
            "mcp_restart_required": False,
            "last_fire_goal": "goal:fire_dual_rsi_test_1",
            "last_verify": {
                "type": "substrate_local",
                "status": "pass",
                "at": "2026-07-10T00:00:00Z",
            },
            "parents": ["goal:dual_rsi_program"],
            "gemma": {"stage": "eval_gate", "sft_rows": 51},
        }
        errs = validate_dual_loop(doc)
        self.assertEqual(errs, [])

    def test_missing_track_next_fails(self):
        errs = validate_dual_loop({"version": 1})
        self.assertTrue(any("track_next" in e for e in errs))

    def test_verify_packet_pass(self):
        pkt = {
            "parent": "goal:dual_rsi_program",
            "loop": "dual_rsi",
            "track": "S",
            "intent": "grow corpus",
            "verify_type": "substrate_local",
            "verify_status": "pass",
            "verify_evidence": "data/lora-export/leg_geometry_sft.jsonl rows=51",
            "falsify": "disk export missing",
        }
        self.assertEqual(validate_verify_packet(pkt), [])

    def test_verify_status_invalid(self):
        pkt = {
            "parent": "goal:x",
            "loop": "ship_gate",
            "track": None,
            "intent": "ship",
            "verify_type": "ship_local",
            "verify_status": "maybe",
            "verify_evidence": "n/a",
            "falsify": "n/a",
        }
        errs = validate_verify_packet(pkt)
        self.assertTrue(any("verify_status" in e for e in errs))


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run test — expect import failure**

```bash
cd /home/a/Documents/Engram
python3 scripts/test_glassbox_schemas.py -v
```

Expected: `ModuleNotFoundError: validate_dual_loop_schema` or import error.

- [ ] **Step 3: Implement schemas + validator**

`docs/schemas/dual_loop_state_v1.json` — require at least: `version`, `track_next`, `mcp_restart_required`, `parents` (array), `last_verify` object optional with `type`/`status`/`at`.

`docs/schemas/fire_verify_packet_v1.json` — require all fields from spec §2.

`scripts/validate_dual_loop_schema.py`:

```python
#!/usr/bin/env python3
"""Validate dual_loop / fire verify packets (stdlib only)."""
from __future__ import annotations

from typing import Any

VERIFY_TYPES = {
    "substrate_local",
    "gemma_stage",
    "meta_policy",
    "ship_local",
    "ship_skip",
    "ci_status",
    "binary_vs_proc",
    "metrics_atom",
}
VERIFY_STATUS = {"pending", "pass", "fail"}
TRACKS = {"S", "G", "M", None}


def validate_dual_loop(doc: dict[str, Any]) -> list[str]:
    errs: list[str] = []
    if not isinstance(doc, dict):
        return ["root must be object"]
    if doc.get("version") != 1:
        errs.append("version must be 1")
    if doc.get("track_next") not in ("S", "G", "M"):
        errs.append("track_next must be S|G|M")
    if "mcp_restart_required" in doc and not isinstance(doc["mcp_restart_required"], bool):
        errs.append("mcp_restart_required must be bool")
    if "parents" in doc and not isinstance(doc["parents"], list):
        errs.append("parents must be array")
    lv = doc.get("last_verify")
    if lv is not None:
        if not isinstance(lv, dict):
            errs.append("last_verify must be object")
        else:
            if lv.get("status") not in VERIFY_STATUS:
                errs.append("last_verify.status invalid")
            if "type" not in lv:
                errs.append("last_verify.type required")
    return errs


def validate_verify_packet(doc: dict[str, Any]) -> list[str]:
    errs: list[str] = []
    for k in (
        "parent",
        "loop",
        "intent",
        "verify_type",
        "verify_status",
        "verify_evidence",
        "falsify",
    ):
        if k not in doc:
            errs.append(f"missing {k}")
    if doc.get("verify_type") not in VERIFY_TYPES:
        errs.append("verify_type invalid")
    if doc.get("verify_status") not in VERIFY_STATUS:
        errs.append("verify_status invalid")
    if "track" in doc and doc["track"] not in TRACKS:
        errs.append("track must be S|G|M|null")
    return errs


if __name__ == "__main__":
    import json
    import sys
    from pathlib import Path

    path = Path(sys.argv[1]) if len(sys.argv) > 1 else None
    if not path:
        print("usage: validate_dual_loop_schema.py <jsonfile> [dual_loop|verify]")
        sys.exit(2)
    doc = json.loads(path.read_text())
    mode = sys.argv[2] if len(sys.argv) > 2 else "dual_loop"
    errs = validate_dual_loop(doc) if mode == "dual_loop" else validate_verify_packet(doc)
    if errs:
        print("FAIL", errs)
        sys.exit(1)
    print("OK")
```

Also write minimal JSON Schema files documenting the same fields (for humans; Python validator is source of truth for tests).

- [ ] **Step 4: Run tests — expect pass**

```bash
python3 scripts/test_glassbox_schemas.py -v
```

Expected: `OK` / all tests passed.

- [ ] **Step 5: Commit**

```bash
git add docs/schemas/dual_loop_state_v1.json docs/schemas/fire_verify_packet_v1.json \
  scripts/validate_dual_loop_schema.py scripts/test_glassbox_schemas.py
git commit -m "feat(glassbox): dual_loop + fire verify schemas and validators"
```

---

### Task 2: Operator skill — glassbox RSI fire lifecycle

**Files:**
- Create: `docs/skills/engram-glassbox-rsi.md`
- Modify: `docs/skills/README.md` (add one row if the file has a skill table)
- Modify: `SKILLS.md` (one bullet linking glassbox skill)

- [ ] **Step 1: Write skill content**

`docs/skills/engram-glassbox-rsi.md` must include:

1. When to use (any scheduled Dual RSI / Ship / PR / Stale / Aliveness fire).
2. Parent goal table from spec.
3. Child goal mint recipe:

```text
mcp_engram_goal_create(
  goal_id="fire_<loop>_<session_key_or_job>_<unix_ts>",
  parent="goal:dual_rsi_program",  # or ship_substrate etc.
  statement="...",
  priority="medium",
  affirm="...", deny="...", reconcile="..."
)
```

4. Verify packet YAML block to paste into goal update note / `metric:verify_<id>` remember text.
5. Typed gate table from spec.
6. HARD: no stage flip / PR claim / ready-to-merge without verify_status=pass.
7. Pointer to loop prompt files under `docs/skills/loop-prompts/`.

- [ ] **Step 2: Link from SKILLS.md**

Add under public skills:

```markdown
- [docs/skills/engram-glassbox-rsi.md](docs/skills/engram-glassbox-rsi.md) — Hybrid fire goals + typed verify for scheduled loops + LEG glass box.
```

- [ ] **Step 3: Commit**

```bash
git add docs/skills/engram-glassbox-rsi.md SKILLS.md docs/skills/README.md
git commit -m "docs(skills): engram-glassbox-rsi fire lifecycle skill"
```

---

### Task 3: Canonical loop prompts v2 (goal + verify)

**Files:**
- Create: `docs/skills/loop-prompts/dual_rsi_v2.md`
- Create: `docs/skills/loop-prompts/ship_gate_v2.md`
- Create: `docs/skills/loop-prompts/pr_watch_v2.md`
- Create: `docs/skills/loop-prompts/mcp_stale_v2.md`
- Create: `docs/skills/loop-prompts/aliveness_bench_v2.md`
- Create: `docs/skills/loop-prompts/README.md`

- [ ] **Step 1: Write dual_rsi_v2.md**

Must be paste-ready for `scheduler_create`. Structure:

```markdown
# Dual RSI v2 (glassbox)

Interval: 20m (user schedules)

```
DUAL RSI v2 — ONE track + fire goal + typed verify

1. session_start(intent="dual_rsi")
2. ack_wake_queue
3. Ensure parents exist (read_concept goal:dual_rsi_program; if missing goal_create dual_rsi_program serving engram_mvp_v1)
4. read_concept(helper:rsi_dual_loop_state) → TRACK=track_next
5. goal_create fire_dual_rsi_<session>_<ts> parent=goal:dual_rsi_program
   statement="Dual RSI track TRACK one win"
   note verify_status=pending verify_type=substrate_local|gemma_stage|meta_policy
6. Execute ONE track only (S/G/M rules from v1 HARD constraints unchanged)
7. Typed verify:
   S: disk path exists OR cargo/test summary + integrity sample
   G: stage metric file/status ok
   M: dual_loop rationale written
8. goal_update_status fire → completed|blocked + remember metric:verify_* if useful
9. update helper:rsi_dual_loop_state (track_last, track_next, last_fire_goal, last_verify)
10. session_end(summary includes fire goal id + verify_status)

HARD: no packs in chat; no multi-track; no stage flip if verify fail
```
```

- [ ] **Step 2: Write ship_gate_v2.md, pr_watch_v2.md, mcp_stale_v2.md, aliveness_bench_v2.md**

Same structure as v1 prompts already used in schedulers, plus steps 3–5 and 7–9 from dual_rsi_v2 pattern:

- ship: parent `goal:ship_substrate`, verify `ship_local` or `ship_skip`
- pr_watch: parent `goal:ship_substrate`, verify `ci_status`, ready only if all required checks SUCCESS
- mcp_stale: parent `goal:dual_rsi_program` or ship, verify `binary_vs_proc`
- aliveness: parent `goal:dual_rsi_program`, verify `metrics_atom`

- [ ] **Step 3: README for loop-prompts**

```markdown
# Loop prompts (Glass-Box RSI v2)

Canonical scheduler bodies. Reschedule with scheduler_create after editing.
Do not leave verify out — LEG glassbox depends on fire goals + last_verify.
```

- [ ] **Step 4: Commit**

```bash
git add docs/skills/loop-prompts/
git commit -m "docs: glassbox loop prompts v2 with fire goals and typed verify"
```

---

### Task 4: Mint durable parent goals (operator script + dry-run notes)

**Files:**
- Create: `scripts/mint_glassbox_parent_goals.md` (runbook using MCP, not auto-MCP from shell)

MCP has no stable non-interactive batch in-repo without a client; ship a **runbook** agents execute once.

- [ ] **Step 1: Write runbook**

`scripts/mint_glassbox_parent_goals.md`:

```markdown
# One-time: mint Glass-Box RSI parent goals

Via Engram MCP (search_tool then use_tool):

1. mcp_engram_goal_create goal_id=dual_rsi_program statement="Dual RSI substrate+Gemma stage machine with typed verify" parent=goal:engram_mvp_v1 priority=high
2. mcp_engram_goal_create goal_id=ship_substrate statement="Ship substrate code with local verify then PR" parent=goal:engram_mvp_v1 priority=high
3. mcp_engram_goal_create goal_id=glassbox_leg statement="LEG Browser split-home glass box for process visibility" parent=goal:engram_mvp_v1 priority=medium
4. mcp_engram_update helper:rsi_dual_loop_state append parents list + schema fields
5. mcp_engram_promote_hot each goal:* and helper:rsi_dual_loop_state
6. Verify: goal_get_children / goal_status on dual_rsi_program
```

- [ ] **Step 2: Commit**

```bash
git add scripts/mint_glassbox_parent_goals.md
git commit -m "docs: runbook to mint glassbox parent goals via MCP"
```

---

### Task 5: AGENT_MEMORY_CONTRACT pointer

**Files:**
- Modify: `docs/AGENT_MEMORY_CONTRACT.md` (add short section near lean tools table)

- [ ] **Step 1: Insert section**

After lean tools table (or Continuity nudges), add:

```markdown
## Glass-Box RSI (scheduled fires)

Scheduled Dual RSI / Ship / PR / Stale / Aliveness fires **mint a child `goal:fire_*`**, run a **typed verify**, then update `helper:rsi_dual_loop_state.last_verify`. Do not flip stages or claim ship/PR ready without `verify_status=pass`.

See: [docs/skills/engram-glassbox-rsi.md](skills/engram-glassbox-rsi.md), [docs/superpowers/specs/2026-07-10-glassbox-rsi-design.md](superpowers/specs/2026-07-10-glassbox-rsi-design.md).
```

- [ ] **Step 2: Commit**

```bash
git add docs/AGENT_MEMORY_CONTRACT.md
git commit -m "docs: AGENT_MEMORY_CONTRACT glassbox fire-goal pointer"
```

---

### Task 6: LEG glassbox static fixture

**Files:**
- Create: `tools/leg-browser/fixtures/glassbox-sample.json`

- [ ] **Step 1: Write fixture**

```json
{
  "dual_loop": {
    "version": 1,
    "track_next": "S",
    "track_last": "G",
    "open_pr": "https://github.com/staticroostermedia-arch/engram/pull/58",
    "mcp_restart_required": false,
    "last_fire_goal": "goal:fire_dual_rsi_demo_1",
    "last_verify": {
      "type": "gemma_stage",
      "status": "pass",
      "at": "2026-07-10T20:00:00Z"
    },
    "parents": [
      "goal:dual_rsi_program",
      "goal:ship_substrate",
      "goal:glassbox_leg"
    ],
    "gemma": {
      "stage": "eval_gate",
      "sft_rows": 51,
      "eval_passed": true
    }
  },
  "aliveness": {
    "fidelity": 0.94,
    "mean_hub_crs": 0.89,
    "hermies_cos": 0.71
  },
  "parents": [
    {
      "id": "goal:dual_rsi_program",
      "status": "active",
      "last_fire": "goal:fire_dual_rsi_demo_1",
      "last_verify_status": "pass"
    },
    {
      "id": "goal:ship_substrate",
      "status": "active",
      "last_fire": "goal:fire_ship_demo_1",
      "last_verify_status": "pass"
    },
    {
      "id": "goal:glassbox_leg",
      "status": "active",
      "last_fire": null,
      "last_verify_status": "pending"
    }
  ],
  "last_fire": {
    "id": "goal:fire_dual_rsi_demo_1",
    "intent": "eval_gate advance",
    "verify_type": "gemma_stage",
    "verify_status": "pass",
    "verify_evidence": "eval_gate_metrics.json passed=true",
    "falsify": "eval fail on re-run"
  }
}
```

- [ ] **Step 2: Commit**

```bash
git add tools/leg-browser/fixtures/glassbox-sample.json
git commit -m "feat(leg-browser): glassbox sample fixture for offline smoke"
```

---

### Task 7: LEG `?view=glassbox` UI (split home)

**Files:**
- Modify: `tools/leg-browser/index.html` (large file — touch only new CSS class block, one view shell, and boot routing)

**Insertion strategy (do not rewrite the whole SPA):**

1. CSS: after existing `.glass-box-tagline` block (~line 529), add `.gb-*` styles for strip + three-column layout.
2. HTML: add `<section id="glassbox-view" class="gb-root" hidden>...</section>` near main app root (after brand header ~1696).
3. JS: add `const GLASSBOX = new URLSearchParams(location.search).get('view') === 'glassbox';` near boot; if true, hide main rails and show `#glassbox-view`, call `loadGlassbox()`.
4. `loadGlassbox()` live path: fetch `/api/block/helper:rsi_dual_loop_state`, parse JSON from text if fenced, fetch parent goals and last_fire_goal blocks, fetch `/health`, optional recent aliveness via `/api/recent?n=10` filter `metric:dual_rsi_aliveness`.
5. Offline: if `?fixture=1` or live fetch fails, load `./fixtures/glassbox-sample.json` (only works when served from tools/leg-browser directory).

- [ ] **Step 1: Add CSS for glassbox**

Minimal classes:

```css
.gb-root { display: flex; flex-direction: column; gap: 12px; padding: 12px; }
.gb-strip { display: flex; flex-wrap: wrap; gap: 8px; }
.gb-chip { border: 1px solid #333; border-radius: 6px; padding: 4px 8px; font-size: 12px; }
.gb-chip.pass { border-color: #2a6; }
.gb-chip.fail { border-color: #a33; }
.gb-chip.warn { border-color: #a80; }
.gb-main { display: grid; grid-template-columns: 1fr 280px; gap: 12px; min-height: 60vh; }
.gb-parents { display: flex; flex-direction: column; gap: 8px; }
.gb-card { border: 1px solid #333; border-radius: 8px; padding: 10px; cursor: pointer; }
.gb-activity { border: 1px solid #333; border-radius: 8px; padding: 8px; overflow: auto; max-height: 70vh; }
@media (max-width: 900px) { .gb-main { grid-template-columns: 1fr; } }
```

- [ ] **Step 2: Add HTML shell**

```html
<section id="glassbox-view" class="gb-root" hidden>
  <div id="gb-banner" class="gb-chip warn" hidden>MCP restart required</div>
  <div id="gb-strip" class="gb-strip"></div>
  <div class="gb-main">
    <div>
      <h2>Program goals</h2>
      <div id="gb-parents" class="gb-parents"></div>
      <h2>Last fire</h2>
      <div id="gb-last-fire" class="gb-card"></div>
    </div>
    <div>
      <h2>Activity</h2>
      <div id="gb-activity" class="gb-activity">Loading…</div>
    </div>
  </div>
</section>
```

- [ ] **Step 3: Implement loadGlassbox JS**

Key behaviors:

- Parse dual_loop text: if contains ` ```json `, extract fence; else try `JSON.parse` whole body.
- Render chips for fidelity (from aliveness or n/a), stage, track_next, open_pr (link), last_verify status, mcp_restart_required.
- Parent cards: for each id in `parents`, fetch `/api/block/{id}` for status snippet.
- Last fire: fetch `last_fire_goal` block; show verify fields from text if present.
- Activity: reuse existing activity poll if `loadActivity` exists; else fetch `/api/activity?limit=20`.
- Click parent/fire: call existing `openBlock(concept)` or inspector if available.

- [ ] **Step 4: Manual smoke**

```bash
# static fixture mode (from tools/leg-browser)
cd /home/a/Documents/Engram/tools/leg-browser
python3 -m http.server 8766 &
# open http://127.0.0.1:8766/index.html?view=glassbox&fixture=1
# expect: three parent cards, chips populated from fixture, no console errors
kill %1
```

Live (optional if serve up):

```bash
./scripts/leg --live
# open http://127.0.0.1:8765/?view=glassbox
# expect: dual_loop chips or graceful fallback to fixture message
```

- [ ] **Step 5: Commit**

```bash
git add tools/leg-browser/index.html
git commit -m "feat(leg-browser): glassbox split-home view (?view=glassbox)"
```

---

### Task 8: Document LEG glassbox + optional API decision

**Files:**
- Modify: `docs/LEG_BROWSER.md`
- Modify: `tools/leg-browser/README.md`

- [ ] **Step 1: LEG_BROWSER.md section**

```markdown
## Glass-Box RSI view

```bash
./scripts/leg --live
# open http://127.0.0.1:8765/?view=glassbox
```

Shows health strip (dual_loop + aliveness), parent program goals, last fire verify, and activity. Read-only. Requires process contract (fire goals + dual_loop fields) for full fidelity; otherwise chips show unknown.

Offline fixture: serve `tools/leg-browser` and open `?view=glassbox&fixture=1`.
```

- [ ] **Step 2: tools/leg-browser/README.md** — same short blurb.

- [ ] **Step 3: Decision note for /api/glassbox**

If live multi-fetch >3s on 80k stalk in practice, file follow-up: implement `GET /api/glassbox` in `serve.rs` returning dual_loop + parents + last_fire only. **Not required for B1 acceptance.**

- [ ] **Step 4: Commit**

```bash
git add docs/LEG_BROWSER.md tools/leg-browser/README.md
git commit -m "docs: LEG glassbox view usage"
```

---

### Task 9: Reschedule guidance (operator, not code)

**Files:**
- Create: `docs/skills/loop-prompts/RESCHEDULE.md`

- [ ] **Step 1: Write RESCHEDULE.md**

List current job IDs (update when known): Dual RSI 20m, Hermies 2h, Meta 8h, Aliveness 1d, Research 3d, Consciousness 30m, Ship 1d, PR 2h, MCP stale 1d.

For each: cancel old with `scheduler_delete`, create new with body from `*_v2.md`.

Note: PR watch only while open_pr set.

- [ ] **Step 2: Commit**

```bash
git add docs/skills/loop-prompts/RESCHEDULE.md
git commit -m "docs: how to reschedule loops onto glassbox v2 prompts"
```

---

### Task 10: PR #58 CI honesty (ship substrate, not LEG)

**Files:** none new; operational steps on branch `feat/leg-corpus-disk-export-peft-path`

- [ ] **Step 1: Inspect failed job**

```bash
gh run view 29121690135 --log-failed 2>&1 | tail -80
```

(or current failed run id from `gh pr checks 58`)

- [ ] **Step 2: One narrow fix** (only if failure is real and reproducible)

Typical: clippy, fmt, unused import in `leg_corpus.rs`. Fix only that; re-run:

```bash
cargo test -p engram-server --tests
cargo fmt --all -- --check
cargo clippy -p engram-server -- -D warnings
```

- [ ] **Step 3: Push if fix needed**

```bash
git push
gh pr checks 58
```

- [ ] **Step 4: Commit only if code changed**

```bash
git commit -m "fix(ci): address PR #58 build-and-test failure"
```

Acceptance: PR watch can report ready only when **all required** checks SUCCESS.

---

## Spec coverage checklist

| Spec item | Task |
|-----------|------|
| dual_loop schema fields | Task 1 |
| fire verify packet | Task 1 |
| Parent goals | Task 4 |
| Child fire lifecycle | Task 2–3 |
| Typed gates | Task 2–3 |
| Loop prompt rewrites | Task 3 |
| AGENT_MEMORY_CONTRACT pointer | Task 5 |
| LEG split home | Task 6–8 |
| CI ready policy | Task 3 (pr_watch_v2), Task 10 |
| MCP restart banner | Task 7 |
| No silent success | Tasks 2–3 HARD lines |
| Optional /api/glassbox later | Task 8 decision note |
| Phased A before B | Task order 1–5 then 6–8 |

## Self-review (plan)

- No TBD/TODO placeholders in steps.
- Schema field names consistent: `track_next`, `last_verify.status`, `verify_type`.
- LEG touches only additive CSS/HTML/JS + fixture.
- Process does not require new Rust for B1.

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-07-10-glassbox-rsi.md`.

**Two execution options:**

1. **Subagent-Driven (recommended)** — fresh subagent per task, review between tasks  
2. **Inline Execution** — this session, `executing-plans`, batch with checkpoints  

Which approach?
