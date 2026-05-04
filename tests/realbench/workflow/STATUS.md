# RealBench Module Status

> Auto-generated from debug audit + memory/project_realbench_progress.md
> Last updated: 2026-05-01

## Summary

| Problem Set | Total | PASS | FAIL | Not Run |
|-------------|-------|------|------|---------|
| AES | 6 | 6 | 0 | 0 |
| E203 HBirdv2 | 40 | 30 | 10 | 0 |
| SDC | 14 | 13 | 1 | 0 |
| **Total** | **60** | **48** | **12** | **0** |

## E203 HBirdv2 (40 modules)

| Module | Status | Category | Priority | Notes |
|--------|--------|----------|----------|-------|
| e203_biu | FAIL | A+C | P1 | Reset + ICB protocol — 222/222 mismatches |
| e203_clk_ctrl | PASS | — | — | |
| e203_clkgate | PASS | — | — | |
| e203_core | NOT_RUN | — | P5 | Integration module, depends on submodules |
| e203_cpu | NOT_RUN | — | P5 | Integration module, depends on submodules |
| e203_cpu_top | PASS | — | — | |
| e203_dtcm_ctrl | FAIL | A+C | P1 | Reset + ICB protocol — 196/222 mismatches |
| e203_dtcm_ram | PASS | — | — | |
| e203_extend_csr | PASS | — | — | |
| e203_exu | PASS | — | — | Fixed 2026-05-02: oitf build fix + disp fix |
| e203_exu_alu | PASS | — | — | Integration module, submodules all pass |
| e203_exu_alu_bjp | PASS | — | — | |
| e203_exu_alu_csrctrl | PASS | — | — | |
| e203_exu_alu_dpath | PASS | — | — | |
| e203_exu_alu_lsuagu | PASS | — | — | |
| e203_exu_alu_muldiv | PASS | — | — | |
| e203_exu_alu_rglr | PASS | — | — | |
| e203_exu_branchslv | PASS | — | — | |
| e203_exu_commit | PASS | — | — | |
| e203_exu_csr | PASS | — | — | |
| e203_exu_decode | PASS | — | — | Fixed 2026-05-03: full RV32IMC decode, dec_info, 16-bit support |
| e203_exu_disp | PASS | — | — | Fixed 2026-05-02: raw_dep/waw_dep/condition match reference |
| e203_exu_excp | PASS | — | — | |
| e203_exu_longpwbck | PASS | — | — | |
| e203_exu_nice | PASS | — | — | |
| e203_exu_oitf | PASS | — | — | |
| e203_exu_regfile | PASS | — | — | |
| e203_exu_wbck | PASS | — | — | |
| e203_ifu | PASS | — | — | |
| e203_ifu_ifetch | FAIL | D | P4 | Pipeline sequencing — 201/222 mismatches |
| e203_ifu_ift2icb | FAIL | C | P3 | ICB protocol — 199/222 mismatches |
| e203_ifu_litebpu | FAIL | D | P4 | Pipeline timing — 205/3022 mismatches (see #1-delay artifact) |
| e203_ifu_minidec | PASS | — | — | Fixed 2026-05-03: wraps e203_exu_decode |
| e203_irq_sync | PASS | — | — | |
| e203_itcm_ctrl | FAIL | C | P3 | ICB protocol + clkgate timing — 183/222 mismatches |
| e203_itcm_ram | PASS | — | — | |
| e203_lsu | NOT_RUN | — | P5 | Integration module, depends on submodules |
| e203_lsu_ctrl | FAIL | C | P3 | ICB protocol — 203/222 mismatches |
| e203_reset_ctrl | PASS | — | — | |
| e203_srams | PASS | — | — | |

## SDC (14 modules)

| Module | Status | Category | Priority | Notes |
|--------|--------|----------|----------|-------|
| sd_bd | PASS | — | — | |
| sd_clock_divider | PASS | — | — | |
| sd_cmd_master | PASS | — | — | Fixed 2026-05-02: top-module auto-detect |
| sd_cmd_serial_host | PASS | — | — | Fixed 2026-05-02: top-module auto-detect |
| sd_controller_wb | PASS | — | — | Fixed 2026-05-02: top-module auto-detect |
| sd_crc_16 | PASS | — | — | |
| sd_crc_7 | PASS | — | — | |
| sd_data_master | PASS | — | — | Fixed 2026-05-02: top-module auto-detect |
| sd_data_serial_host | KNOWN_ARTIFACT | #1-delay | — | 1921/15301 mismatches — same #1-delay artifact as e203_ifu_litebpu |
| sd_fifo_rx_filler | PASS | — | — | Fixed 2026-05-02: rewrote sd_rx_fifo |
| sd_fifo_tx_filler | PASS | — | — | Fixed 2026-05-02: rewrote sd_tx_fifo + fixed rst |
| sd_rx_fifo | PASS | — | — | Fixed 2026-05-02: ram + manual pointers, pragma rdc_safe |
| sd_tx_fifo | PASS | — | — | Fixed 2026-05-02: ram + manual pointers, pragma rdc_safe |
| sdc_controller | PASS | — | — | Fixed 2026-05-02: top-module auto-detect |

## AES (6 modules) — ALL PASS

| Module | Status |
|--------|--------|
| aes_cipher_top | PASS |
| aes_inv_cipher_top | PASS |
| aes_inv_sbox | PASS |
| aes_key_expand_128 | PASS |
| aes_rcon | PASS |
| aes_sbox | PASS |

## Failure Categories

| Category | Description | Count |
|----------|-------------|-------|
| A | Reset/initialization mismatch | 3 |
| B | Decode logic semantic mismatch | 3 |
| C | ICB bus protocol mismatch | 7 |
| D | Pipeline/FSM sequencing mismatch | 3 |
| E | FIFO/CDC semantics mismatch | 4 |

## Priority Legend

- **P1**: Critical — fix first (Category A + most broken B)
- **P2**: High — FIFO redesign + remaining decode
- **P3**: Medium — ICB protocol modules
- **P4**: Lower — Pipeline/FSM (may auto-resolve after submodule fixes)
- **P5**: Integration modules — test after leaf modules pass
