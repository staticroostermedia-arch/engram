# Engram: Contributing & Release Workflow

> **For agents and developers working on the Engram codebase itself.**
> Read this before making changes you intend to push to the public GitHub repo.

---

## The Two-Branch Model

Engram maintains a strict separation between private development and public release:

```
origin/master               ← PUBLIC canonical. This is what users clone.
                              Always sanitized. Always the most up-to-date
                              released version. Merge here via PR only.

local/master (or any branch) ← Your private working state. May contain
                              environment-specific config, local paths,
                              internal project references. NEVER push directly
                              to origin/master.

release/public-sanitized    ← The sanitization staging branch. All changes
                              destined for origin/master flow through here.
                              Regularly pushed to origin, then PR'd to master.
```

### Why This Exists

Engram was extracted from a larger private research system. During development, the codebase may accumulate:
- Environment-specific paths (e.g., referencing local project directories)
- Internal project names or branding in comments/docstrings
- Hard-coded local config that should be env-var driven

The `release/public-sanitized` branch is the audit checkpoint that prevents this from reaching public users.

---

## The Development Loop

### When Working on Features / Fixes Locally

```bash
# Work on master or a feature branch — no restrictions
git checkout master
# ... make your changes ...
git add -A && git commit -m "feat: your change"
```

### When Ready to Publish Publicly

**Step 1: Privacy Audit**

Before staging for public release, run the leak scanner:

```bash
# Scan for internal references that must not be public
grep -rn "CodeLand\|/home/[a-z]/\|StaticRooster\|staticrooster" \
  ./crates/ --include="*.rs" --include="*.json" --include="*.toml" \
  | grep -v target/ | grep -v ".git/"
```

If anything surfaces:
- Replace hardcoded paths with environment variables (see table below)
- Replace internal project names with generic descriptions in comments/docstrings
- Replace internal brand names with generic alternatives

**Step 2: Merge to Release Branch**

```bash
git checkout release/public-sanitized
git merge master --no-edit
git push origin release/public-sanitized
```

**Step 3: Open PR on GitHub**

Open a PR from `release/public-sanitized` → `master` on GitHub.

PR description should note:
- What was changed functionally
- What was sanitized (if a privacy pass was included)
- Compile status (`cargo check --workspace` result)

**Step 4: After Merge**

```bash
# Return to local master for continued work
git checkout master
```

You do **not** need to pull the merge back — your local `master` already has everything. The merge only affects the public GitHub `master`.

---

## Environment Variable Reference

All private/local configuration must be expressed as environment variables, not hardcoded values:

| Variable | Purpose | Default |
|---|---|---|
| `ENGRAM_LINKED_WORKSPACE` | Root path for AST file-watching | none (watcher disabled) |
| `ENGRAM_ORACLE_URL` | Transductive oracle endpoint | none (oracle disabled) |
| `ENGRAM_SCOUT_DAEMON` | Override path to `scout_daemon.py` | `./integrations/scout_daemon.py` |
| `ENGRAM_GENESIS_PATH` | Manifold path for benchmarks/examples | `~/.engram/manifold` |
| `ENGRAM_STORE` | Manifold storage directory | `~/.engram/stalks/` |
| `ENGRAM_EMBED_URL` | Embedding server endpoint | `http://localhost:8086/v1/embeddings` |

**Rule:** If a value is specific to a machine, user, or private project — it must be an env var, not a constant.

---

## What Must Never Be Committed to origin/master

| Type | Example | Fix |
|---|---|---|
| Absolute local paths | `/home/username/Documents/ProjectName/` | Use `ENGRAM_*` env var |
| Internal project names in code | `// ported from InternalProject/crate_name` | Remove or generalize |
| Personal email / real name in code | In docstrings, CLI strings | Keep only in `Cargo.toml` authors field |
| Patent numbers | `US19/372,256` | Keep only in `PATENT-NOTICE.md` |
| Runtime log files | `engram_serve.log`, `server.log` | Add to `.gitignore` |
| Scratch/test files | `test_hypothesis.rs`, `sem_search.rs` | Add to `.gitignore` |
| Manifold data files | `~/.engram/stalks/*.leg3` | These live outside the repo by design |

---

## For Agents: How to Know Which Version Is Current

- **`origin/master`** is always the public canonical version. The README on GitHub points here.
- **`release/public-sanitized`** is staging — it may be 1-2 commits ahead of master at any given time.
- **Local working branches** are development state — assume they contain private config.

If you are an agent operating inside the Engram development workspace and a user asks you to push changes publicly:

1. Run the privacy audit above first
2. Commit to the current local branch
3. Merge to `release/public-sanitized`
4. Push `release/public-sanitized` to origin
5. Note in the session summary that a public push was made and what was included

---

## Quick Reference: Safe PR Checklist

```
Before opening a PR to origin/master:
  [ ] cargo check --workspace passes clean
  [ ] grep for /home/$USER/ → zero results in .rs/.json/.toml
  [ ] grep for internal project names → zero results in .rs/.json/.toml  
  [ ] grep for patent numbers → only in PATENT-NOTICE.md
  [ ] .log files are in .gitignore
  [ ] genesis.json identity seed contains no personal info
  [ ] No manifold .leg3 data files staged
```
