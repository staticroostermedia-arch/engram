# xAI Grok Build Marketplace Submission — engram-geometric

**Plugin id:** `engram` (or `engram-geometric` for disambiguation)  
**Repo subdirectory:** `grok-plugin-engram/` (self-contained plugin bundle with .mcp.json + plugin.json)  
**Current status:** `v0.7.0-beta.1` — LEG Browser beta, slim wake bundle, harness continuity. Remote registration via xai-org/plugin-marketplace (Option A).

---

## Prerequisites (checklist)

- [ ] `grok plugin validate grok-plugin-engram/` passes
- [ ] `./scripts/engram-mcp-health.sh` → OK
- [ ] `agent-memory` harness green locally and in CI
- [ ] Git tag pushed (e.g. `v0.7.0-beta.1`)
- [ ] Release asset uploaded (`engram-v0.7.0-beta.1-linux-x86_64.tar.gz`)
- [ ] Demo doc tested: [examples/marketplace_demo.md](examples/marketplace_demo.md)

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
    "sha": "<pin-commit-on-release-tag>"
  },
  "homepage": "https://github.com/staticroostermedia-arch/engram",
  "keywords": ["memory", "mcp", "agent", "geometric", "geometric-memory", "session-handoff", "leg-browser"],
  "version": "0.7.0-beta.1",
  "author": "staticroostermedia-arch"
}
```

Pin SHA:

```bash
git ls-remote https://github.com/staticroostermedia-arch/engram.git refs/tags/v0.7.0-beta.1
# Use the commit on your release tag, not floating HEAD
```

Regenerate index (upstream maintainer or PR author):

```bash
python3 scripts/generate-plugin-index.py
python3 scripts/validate-catalog.py
```

---

## PR steps

1. Merge Engram marketplace work to `main` on your repo
2. Tag: `git tag v0.7.0-beta.1 && git push origin v0.7.0-beta.1`
3. Wait for Release workflow + harness green
4. Fork `xai-org/plugin-marketplace`
5. Add catalog entry with **pinned SHA** from the tag
6. Run validators; open PR
7. In PR description, link:
   - [GROK_BUILD_MEMORY.md](GROK_BUILD_MEMORY.md)
   - [marketplace_demo.md](examples/marketplace_demo.md)
   - Harness CI badge / job link

---

## What YOU must do manually

| Step | Who | Action |
|------|-----|--------|
| Push branch + merge PR | **You** | GitHub merge to `main` (branch protection may require PR) |
| Create release tag | **You** | `git tag v0.7.0-beta.1 && git push origin v0.7.0-beta.1` |
| Fork marketplace repo | **You** | GitHub fork `xai-org/plugin-marketplace` |
| Open marketplace PR | **You** | Submit catalog entry; xAI reviews |
| Trust plugin locally | **You** | `grok plugin install ... --trust` |

No xAI API key required for local plugin. Marketplace listing is a GitHub PR to their catalog repo.

---

## Post-listing

- Monitor install issues (binary path, CUDA optional)
- Respond to reviews on pinned SHA updates
- Bump `sha` in catalog when shipping plugin fixes