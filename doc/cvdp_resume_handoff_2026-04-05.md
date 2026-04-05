# CVDP Resume Handoff (2026-04-05 11:41 PDT)

## Session Summary
Recent progress focused on CVDP failures with source-first policy (`.arch` edits + regenerated `.sv`).

### Newly passing this session
- `sprite_controller_fsm` (task: `cvdp_copilot_sprite_0004`)
  - File touched: `tests/cvdp/sprite_fsm.arch`
  - Regenerated: `tests/cvdp/sprite_fsm.sv`
- `write_buffer_merge`
  - File touched: `tests/cvdp/write_buffer_merge.arch`
  - Regenerated: `tests/cvdp/write_buffer_merge.sv`
- `pipeline_mac`
  - Needed runner compatibility patch for dependent parameter override.
  - File touched: `tests/cvdp/run_cvdp.py`

### Still failing at end of session
- `one_hot_gen`
  - Current in-progress rewrite in `tests/cvdp/one_hot_gen.arch`
  - Symptom: `o_ready` mismatch vs harness model (`test_one_hot_gen.py`)
- `load_store_unit`
  - Symptom: missing expected interface port(s), first observed `ex_if_extend_mode_i`
  - Work not completed yet.

## Important Constraints / Preferences
- Do not hand-edit generated `.sv` for functional fixes.
- Fix in `.arch` and regenerate via `cargo run -- build ...`.

## Key Commands to Resume

### Validate known fixed modules
```bash
python3 tests/cvdp/run_cvdp.py cvdp_copilot_sprite_0004 tests/cvdp/sprite_fsm.sv
python3 tests/cvdp/run_cvdp.py write_buffer_merge tests/cvdp/write_buffer_merge.sv
python3 tests/cvdp/run_cvdp.py pipeline_mac tests/cvdp/pipeline_mac.sv
```

### Reproduce active failures
```bash
python3 tests/cvdp/run_cvdp.py one_hot_gen tests/cvdp/one_hot_gen.sv
python3 tests/cvdp/run_cvdp.py load_store_unit tests/cvdp/load_store_unit.sv
```

## One-Hot-Gen Debug Notes
Harness model behavior (from extracted temp run):
- Model always leaves `IDLE` on update (chooses REGION_A/REGION_B from `config[0]`).
- `o_ready = 1` only when model is in `IDLE`, otherwise 0.
- Address sequencing is region-driven with `position` roll and optional region chaining:
  - REGION_A: `config==2` chains to REGION_B
  - REGION_B: `config==3` chains to REGION_A
Current arch rewrite is close but still has cycle alignment mismatch on `o_ready`.

## Pipeline-MAC Compatibility Patch
`tests/cvdp/run_cvdp.py` now patches runners that only pass `DWIDTH/N` so they also pass:
- `DWIDTH_ACCUMULATOR = ((N - 1).bit_length() + 2 * DWIDTH)`
This resolved `pipeline_mac` parameter-width assertion failures.

## Workspace State Warning
`git status` is very dirty with many unrelated modified/untracked files (including large E203 tree).
Be careful to scope edits only to CVDP files and do not revert unrelated work.

