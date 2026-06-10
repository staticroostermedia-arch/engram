# OptiX Re-Enable + Grok Build TUI Update Plan (June 2026)

**Goal**: Attempt to run with full OptiX acceleration again (now that the launcher defaults to enabled and we've done significant substrate work), while safely updating to the latest Grok Build TUI.

**Context**: Historically, full OptiX on the large ~149k manifold + ki_hijacker caused MCP starvation ("Transport closed"). Safe mode was `ENGRAM_OPTIX_ENABLED=0`. The launcher has since been improved.

---

## 1. Current State (as of this plan)

- Launcher: `/path/to/.local/bin/engram-grok`
  - Now defaults to `ENGRAM_OPTIX_ENABLED=1`
  - Prefers `~/.cargo/bin/engram` (the cargo-installed binary)
- Running processes (example PIDs from diagnostics):
  - Multiple `/path/to/.cargo/bin/engram ... mcp`
  - `grok` TUI process

---

## 2. Step-by-Step Commands

### Step 1: Kill Existing Engram MCP Processes

```bash
# Recommended safe command (kills engram MCP servers specifically)
pkill -f '/path/to/.cargo/bin/engram --store /path/to/.engram/stalks/ mcp'

# Verify
pgrep -af engram
```

If anything remains:

```bash
pkill -9 -f engram
```

### Step 2: Rebuild the Engram Binary (Get Latest Code + Any OptiX Improvements)

```bash
cargo install --path crates/engram-server --force
```

This ensures you're running the latest version of the server (including any recent fixes to bvh.rs, lazy OptiX handling, etc.).

### Step 3: Start the New Engram MCP Server with OptiX

Using the dedicated launcher (recommended):

```bash
# Full OptiX attempt (current default in launcher)
engram-grok mcp

# Or explicit
ENGRAM_OPTIX_ENABLED=1 /path/to/.local/bin/engram-grok mcp
```

**Monitor for issues**:
- Watch the log: `tail -f ~/.grok/logs/mcp/engram.stderr.log`
- If you see heavy OptiX compilation + MCP starvation symptoms, fall back immediately:

```bash
# Safe fallback (what worked before)
ENGRAM_OPTIX_ENABLED=0 /path/to/.local/bin/engram-grok mcp
```

### Step 4: Update / Restart the Grok Build TUI

Since you believe the Grok Build TUI was updated:

1. In your current TUI session, exit cleanly (usually `:q`, `exit`, or the standard quit sequence for the Grok TUI).
2. Relaunch the TUI using whatever command you normally use to start Grok Build (most likely just running `grok` or the equivalent desktop/app launcher).

The TUI should pick up the latest client-side updates on relaunch.

If you have a specific update mechanism (e.g., a CLI command or config pull), run that before relaunching.

### Step 5: Verification After Restart

Once both are running:

- Confirm the MCP server is healthy (no immediate "Transport closed").
- In the TUI, test basic Engram tools (especially spatial ones and thought_tile tools).
- If OptiX is on, monitor CPU/GPU and MCP responsiveness during heavy operations (ki_hijacker bakes, large recalls, etc.).
- If anything feels worse than the safe mode, note the symptoms and we can analyze the logs.

---

## 3. Rollback / Safe Mode

If full OptiX still causes problems:

- Always have the fallback ready: `ENGRAM_OPTIX_ENABLED=0 engram-grok mcp`
- The launcher supports this cleanly.
- We can also investigate lazy OptiX improvements further in bvh.rs if needed (we already have some history here from Item 1.5).

---

## 4. Recommended First Test Sequence

1. Kill old processes.
2. Rebuild binary.
3. Start with OptiX=1 using the launcher.
4. Exit + relaunch TUI.
5. Do a light test session (create a Tile, do some spatial work, trigger a ki bake).
6. If stable, great — we have full hardware acceleration back.
7. If not, fall back and we analyze.

---

*Plan created June 2026 alongside the revised Item 2 scope document.*