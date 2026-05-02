# Contributing to Engram

We welcome pull requests! Since this is a high-performance vector database, please ensure all new features merge cleanly with the `.leg3` logophysical file system. Before submitting a PR, run `cargo clippy` and `cargo check --workspace`.

---

## Branch Strategy

Engram uses a two-branch public release model:

| Branch | Purpose |
|---|---|
| `master` | Public canonical. What users clone. Always sanitized. Merge here via PR only. |
| `release/public-sanitized` | Staging branch. All public-destined changes flow through here first. |

**Never push private config, local paths, or internal project references to `master`.**

See [`integrations/workflows/contributing.md`](integrations/workflows/contributing.md) for the full release workflow, privacy audit checklist, and environment variable reference.

---

## Environment Variables

All site-specific configuration must be expressed as environment variables — not hardcoded paths or constants. Key variables:

| Variable | Purpose |
|---|---|
| `ENGRAM_LINKED_WORKSPACE` | Root path for AST file-watching |
| `ENGRAM_ORACLE_URL` | Transductive oracle endpoint (opt-in) |
| `ENGRAM_SCOUT_DAEMON` | Override path to `scout_daemon.py` |
| `ENGRAM_STORE` | Manifold storage directory |
| `ENGRAM_EMBED_URL` | Embedding server endpoint |

---

## Submitting a PR

1. Run the privacy audit: `grep -rn "/home/[a-z]/" ./crates/ --include="*.rs" | grep -v target/`
2. Ensure `cargo check --workspace` is clean
3. Run `cargo clippy` and address warnings
4. Merge your work to `release/public-sanitized` locally
5. Push `release/public-sanitized` to origin
6. Open a PR from `release/public-sanitized` → `master`

The PR description should include compile status and what was changed. If a privacy pass was performed, note what was sanitized.
