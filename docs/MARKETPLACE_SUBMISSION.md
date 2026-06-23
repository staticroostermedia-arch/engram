# xAI Grok Build Marketplace Submission — engram-geometric

**Plugin id:** `engram` (or `engram-geometric` for disambiguation)  
**Repo subdirectory:** `grok-plugin-engram/` (self-contained plugin bundle with .mcp.json + plugin.json)  
**Current release:** `v0.7.0-beta.3` — code atlas continuity v2, 79 MCP tools, hard wake gate default, large-store perf.  
**Remote registration:** xai-org/plugin-marketplace (Option A).

---

## Prerequisites (checklist)

- [ ] `grok plugin validate grok-plugin-engram/` passes
- [ ] `./scripts/engram-mcp-health.sh` → OK
- [ ] `agent-memory` harness green locally and in CI
- [ ] Git tag pushed: `v0.7.0-beta.3`
- [ ] Release asset uploaded (`engram-v0.7.0-beta.3-linux-x86_64.tar.gz`) — if using Release workflow
- [ ] Demo doc tested: [examples/marketplace_demo.md](examples/marketplace_demo.md)
- [ ] GitHub social preview: upload [docs/images/engram-share-x.png](images/engram-share-x.png) (Settings → General → Social preview)

---

## Catalog entry (for `xai-org/plugin-marketplace`)

Fork https://github.com/xai-org/plugin-marketplace and add to `.grok-plugin/marketplace.json` under `plugins`:

```json
{
  "name": "engram",
  "description": "Persistent geometric memory substrate for AI agents. Local, one-call slim wake (session_start + harness injection), spatial code context (context_for_edit), structured session handoff. 8-tool lean contract. LEG Browser beta for human review. Not a vector DB or flat RAG.",
  "category": "development",
  "source": {
    "source": "url",
    "url": "https://github.com/staticroostermedia-arch/engram",
    "sha": "de41275d9bf3dcfaf2cec9f802cf9ee3500c6ee4"
  },
  "homepage": "https://github.com/staticroostermedia-arch/engram",
  "keywords": ["memory", "mcp", "agent", "geometric", "geometric-memory", "session-handoff", "leg-browser"],
  "version": "0.7.0-beta.3",
  "author": "staticroostermedia-arch"
}
```

**Pin SHA (after tag push):**

```bash
git fetch --tags origin
git rev-parse v0.7.0-beta.3^{commit}
# v0.7.0-beta.3 tag → de41275d9bf3dcfaf2cec9f802cf9ee3500c6ee4 (release after merge PR #36)
```

Regenerate index (upstream maintainer or PR author):

```bash
python3 scripts/generate-plugin-index.py
python3 scripts/validate-catalog.py
```

---

## PR steps

1. Merge doc + release work to `master` on your repo
2. Tag: `git tag v0.7.0-beta.3 && git push origin v0.7.0-beta.3`
3. Wait for Release workflow + harness green (if enabled)
4. Fork `xai-org/plugin-marketplace`
5. Add catalog entry with **pinned SHA** from `git rev-parse v0.7.0-beta.3^{commit}`
6. Run validators; open PR
7. In PR description, link:
   - [GROK_BUILD_MEMORY.md](GROK_BUILD_MEMORY.md)
   - [marketplace_demo.md](examples/marketplace_demo.md)
   - [FIRST_RUN.md](../FIRST_RUN.md) + [AGENT_MEMORY_CONTRACT.md](AGENT_MEMORY_CONTRACT.md)
   - Harness CI badge / job link

---

## What YOU must do manually

| Step | Who | Action |
|------|-----|--------|
| Push branch + merge PR | **You** | GitHub merge to `master` |
| Create release tag | **You** | `git tag v0.7.0-beta.2 && git push origin v0.7.0-beta.2` |
| GitHub social preview | **You** | Upload `docs/images/engram-share-x.png` in repo Settings |
| Fork marketplace repo | **You** | GitHub fork `xai-org/plugin-marketplace` |
| Open marketplace PR | **You** | Submit catalog entry with pinned SHA; xAI reviews |
| Trust plugin locally | **You** | `grok plugin install ... --trust` |

---

## Version history (marketplace pins)

| Tag | Notes |
|-----|-------|
| `v0.7.0-beta.1` | LEG Browser beta, presentation stratum, harness continuity |
| `v0.7.0-beta.2` | Public face polish, JIT deformation, tool matrix, wiki docs, cold-start fork |

Always pin marketplace `sha` to the **tag commit**, not `main` HEAD.