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

When ARCH docs change, refresh this snapshot from the repository root:

```sh
cp README.md skills/arch-programming/references/README.md
cp doc/Arch_AI_Reference_Card.md skills/arch-programming/references/Arch_AI_Reference_Card.md
cp doc/ARCH_HDL_Specification.md skills/arch-programming/references/ARCH_HDL_Specification.md
cp doc/COMPILER_STATUS.md skills/arch-programming/references/COMPILER_STATUS.md
cp doc/arch_sim_cocotb.md skills/arch-programming/references/arch_sim_cocotb.md
cp doc/plan_arch_doc_comments.md skills/arch-programming/references/plan_arch_doc_comments.md
cp mcp/README.md skills/arch-programming/references/mcp_README.md
cp mcp/instructions.md skills/arch-programming/references/mcp_instructions.md
cp LICENSE skills/arch-programming/references/LICENSE
```
