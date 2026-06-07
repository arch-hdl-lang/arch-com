# ARCH Skill Reference Snapshot

These reference files are copied from the same `arch-com` repository commit
that contains this skill folder. They are bundled so users can install
`skills/arch-programming` into `~/.codex/skills` and still have core ARCH
guidance available outside a repo checkout.

## Bundled Sources

- `README.md` from the repository root
- `doc/Arch_AI_Reference_Card.md`
- `doc/ARCH_HDL_Specification.md`
- `doc/COMPILER_STATUS.md`
- `doc/arch_sim_cocotb.md`
- `doc/plan_arch_doc_comments.md`
- `mcp/README.md`
- `mcp/instructions.md`
- `LICENSE` or `license.txt` from the repository root, copied as `LICENSE`

## Link Notes

The copied markdown keeps upstream prose intact. Some links still point to
repo-local paths such as `doc/...`, `examples/...`, `tests/...`, `mcp/...`, or
helper scripts. Those links require a local `arch-com` checkout; they are not
all bundled inside the installed skill.

For compiler-backed syntax, prefer the ARCH MCP tools when available. For
runnable examples and full regression context, use a live `arch-com` checkout.

## Refresh

When ARCH docs change, refresh these snapshots from the repository root:

```sh
scripts/sync_skill_snapshots.sh refresh
```

The script owns the source -> snapshot mapping. The `skill-snapshots` CI
workflow runs `scripts/sync_skill_snapshots.sh check` on every PR that touches
a source doc or a bundled snapshot, and fails if the two have drifted — so a
doc change must be accompanied by a refresh in the same PR.
