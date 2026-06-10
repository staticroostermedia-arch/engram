---
name: Bug report
about: Create a report to help us improve Engram (geometric memory + rituals + MCP)
labels: 'bug'
---

**Describe the bug**
A clear and concise description of what the bug is.

**To Reproduce**
Steps to reproduce the behavior:
1. Go to '...'
2. Click on '....'
3. Scroll down to '....'
4. See error

**Expected behavior**
A clear and concise description of what you expected to happen.

**Screenshots**
If applicable, add screenshots to help explain your problem.

**Enagram-specific checks (ritual hygiene)**
- [ ] Ran `mcp_engram_verify_manifold_integrity` / `mcp_engram_verify_block_lawfulness` / `mcp_engram_spatial_status` / `mcp_engram_genesis status` before/after?
- [ ] Spatial recon done (`context_for_file`, `recall_in_file`)?
- [ ] Trace recorded for the issue (A/D/R)?
- [ ] Current build used (target/debug/engram or cargo run)?
- [ ] Non-flat invariants / rituals affected? (scar/verify/continuation etc.)

**Additional context**
Add any other context about the problem here.

**Environment**
- OS: [e.g. Linux, macOS]
- Binary: target/debug/engram or /path/to/local/engram (version?); prefer current build target/debug per hygiene
- MCP client: [Claude, Cursor, etc.]