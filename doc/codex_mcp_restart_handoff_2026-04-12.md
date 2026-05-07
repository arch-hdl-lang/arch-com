# Codex MCP Restart Handoff (2026-04-12)

## What we did

- Confirmed the repo includes an ARCH MCP server:
  - `mcp/arch_mcp_server.py`
  - setup docs in `mcp/README.md`
  - workflow guidance in `mcp/instructions.md`
- Verified local prerequisites already exist:
  - `mcp/.venv/bin/python`
  - `target/release/arch`
- Started the server process locally with:

```bash
mcp/.venv/bin/python mcp/arch_mcp_server.py
```

## Current blocker

- This Codex VSCode plugin session did **not** expose any registered MCP servers/resources after the process was started.
- `list_mcp_resources` returned no resources.
- `arch-hdl` was not a known MCP server in this session.
- Conclusion: the server process can run locally, but this session cannot hot-attach to it. The plugin likely needs to be restarted after MCP config is available.

## CVDP context saved

Current benchmark log:
- `doc/cvdp_benchmark_log.md`

Remaining CVDP modules we mapped to local `.arch` files:
- `tests/cvdp/sgd_linear_regression.arch`
- `tests/cvdp/vga_controller.arch`
- `tests/cvdp/Data_Reduction.arch`
- `tests/cvdp/load_store_unit.arch`
- `tests/cvdp/microcode_sequencer.arch`
- `tests/cvdp/secure_read_write_register_bank.arch`
- `tests/cvdp/digital_dice_roller.arch`
- `tests/cvdp/low_pass_filter.arch`
- `tests/cvdp/gf_mac.arch`
- `tests/cvdp/dig_stopwatch.arch`
- `tests/cvdp/halfband_fir.arch`
- `tests/cvdp/inter_block.arch`
- `tests/cvdp/apb_dsp_op.arch`

Special note:
- `field_extract` does not have a matching `tests/cvdp/field_extract.arch`.
- The benchmark reference points to `TOPLEVEL = field_extract` inside `tests/cvdp/medium_specs/ethernet_parser.txt`.

## Best next target after restart

- First choice: `tests/cvdp/apb_dsp_op.arch`
  - benchmark note says `14/15` passing
  - likely a single remaining timing edge case
- Alternative quick targets:
  - `tests/cvdp/dig_stopwatch.arch`
  - `tests/cvdp/digital_dice_roller.arch`

## Exact next step after plugin restart

1. Check whether MCP server `arch-hdl` is visible in the new session.
2. If visible, use the ARCH MCP workflow before editing any `.arch` file:
   - fetch construct syntax
   - write/check
   - build/lint
3. Then open and debug `tests/cvdp/apb_dsp_op.arch`.

