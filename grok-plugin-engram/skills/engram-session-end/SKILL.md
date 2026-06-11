---
name: engram-session-end
description: >
  Mandatory end-of-block handoff. Structured session_end packet for next
  wake continuation; include decisions, files, traces, open questions.
metadata:
  short-description: "Handoff — session_end packet"
---

# Engram Session-End

**Trigger:** End of every work block, before context compression, when handing off.

## Call (mandatory)

```
mcp_engram_session_end(
  summary="<decisions, files changed, trace names, blockers, next steps>",
  prepare_compression=true
)
```

Or: `/engram-session-end`

## Summary must include

1. Decisions — cite `trace:*` names created
2. Files touched — paths for next `context_for_edit`
3. Open blockers — falsifiable
4. Next steps — ordered

The next instance's `session_start` embeds this in `continuation_bundle`.

Full protocol: `docs/skills/engram-session-end.md`