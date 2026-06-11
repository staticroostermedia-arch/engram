# xAI Grok Build Marketplace Submission — engram-geometric

**Plugin id:** `engram-geometric`  
**Repo subdirectory:** `grok-plugin-engram/` (or full repo with plugin at root path)  
**Plan:** [design/GROK_MARKETPLACE_EXCELLENCE_PLAN.md](../design/GROK_MARKETPLACE_EXCELLENCE_PLAN.md)

---

## Prerequisites (checklist)

- [ ] `grok plugin validate grok-plugin-engram/` passes
- [ ] `./scripts/engram-mcp-health.sh` → OK
- [ ] `agent-memory` harness green locally and in CI
- [ ] Git tag pushed (e.g. `v0.5.1-plugin`)
- [ ] Release asset uploaded (`engram-v0.5.1-plugin-linux-x86_64.tar.gz`)
- [ ] Demo doc tested: [examples/marketplace_demo.md](examples/marketplace_demo.md)

---

## Catalog entry (for `xai-org/plugin-marketplace`)

Fork https://github.com/xai-org/plugin-marketplace and add to `.grok-plugin/marketplace.json` under `plugins`:

```json
{
  "name": "engram-geometric",
  "description": "Local geometric memory for Grok Build: one-call wake, anchor-first recall, edit-scoped code context (context_for_edit), structured session handoff. 8-tool lean contract — survives 200k-block stores without OOM. Not a vector DB wrapper.",
  "category": "development",
  "source": {
    "source": "url",
    "url": "https://github.com/staticroostermedia-arch/engram.git",
    "sha": "REPLACE_WITH_FULL_40_CHAR_COMMIT_SHA"
  },
  "homepage": "https://github.com/staticroostermedia-arch/engram",
  "keywords": [
    "memory",
    "persistent-memory",
    "mcp",
    "session-handoff",
    "geometric-memory",
    "code-context",
    "engram-geometric"
  ],
  "author": "Static Rooster Media",
  "tags": ["memory", "mcp", "local-first"]
}
```

Pin SHA:

```bash
git ls-remote https://github.com/staticroostermedia-arch/engram.git HEAD
# Use the commit on your release tag, not floating HEAD
```

Regenerate index (upstream maintainer or PR author):

```bash
python3 scripts/generate-plugin-index.py
python3 scripts/validate-catalog.py
```

---

## PR steps

1. Merge Engram marketplace work to `master` on your repo
2. Tag: `git tag v0.5.1-plugin && git push origin v0.5.1-plugin`
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
| Push branch + merge PR | **You** | GitHub merge to `master` (branch protection may require PR) |
| Create release tag | **You** | `git tag v0.5.1-plugin && git push origin v0.5.1-plugin` |
| Fork marketplace repo | **You** | GitHub fork `xai-org/plugin-marketplace` |
| Open marketplace PR | **You** | Submit catalog entry; xAI reviews |
| Trust plugin locally | **You** | Already done: `grok plugin install ... --trust` |

No xAI API key required for local plugin. Marketplace listing is a GitHub PR to their catalog repo.

---

## Post-listing

- Monitor install issues (binary path, CUDA optional)
- Respond to reviews on pinned SHA updates
- Bump `sha` in catalog when shipping plugin fixes