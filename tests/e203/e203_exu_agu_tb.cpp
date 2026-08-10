// ARCH sim testbench for e203_exu_agu — E203 address generation unit.
// Tests: effective-address computation (base + signed offset, wrap-around),
// word alignment of the ICB command address, store byte-enable and store-data
// lane alignment for SB/SH/SW, load result extraction with sign/zero
// extension for LB/LBU/LH/LHU/LW, and the dispatch/ICB/writeback handshakes.
//
// NOTE: this replaces a stale tb (VExuAgu.h) that targeted an earlier
// revision of the fixture before the e203 fixtures were rewritten against the
// real E203 RTL, which renamed the construct to `e203_exu_agu`. The old tb has
// not compiled since. Ported to the current class name (Ve203_exu_agu).
//
// The module is purely combinational (no clock/reset ports): drive inputs,
// eval(), check.
//
// Run with:
//   arch sim tests/e203/e203_exu_agu.arch --tb tests/e203/e203_exu_agu_tb.cpp

#include "Ve203_exu_agu.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_exu_agu* dut;

// funct3 encodings (RV32I load/store)
enum { F3_B = 0, F3_H = 1, F3_W = 2, F3_BU = 4, F3_HU = 5 };

static void clear_inputs() {
    dut->i_valid = 0;
    dut->i_rs1 = 0;
    dut->i_rs2 = 0;
    dut->i_imm = 0;
    dut->i_load = 0;
    dut->i_store = 0;
    dut->i_rd_idx = 0;
    dut->i_rd_en = 0;
    dut->i_funct3 = 0;
    dut->icb_cmd_ready = 1;
    dut->icb_rsp_valid = 0;
    dut->icb_rsp_rdata = 0;
    dut->o_ready = 1;
    dut->eval();
}

// Drive a store request and eval.
static void drive_store(uint32_t rs1, uint32_t imm, uint32_t rs2, uint8_t funct3) {
    clear_inputs();
    dut->i_valid = 1;
    dut->i_store = 1;
    dut->i_rs1 = rs1;
    dut->i_imm = imm;
    dut->i_rs2 = rs2;
    dut->i_funct3 = funct3;
    dut->eval();
}

// Drive a load request with a response present, and eval.
static void drive_load(uint32_t rs1, uint32_t imm, uint8_t funct3, uint32_t rdata) {
    clear_inputs();
    dut->i_valid = 1;
    dut->i_load = 1;
    dut->i_rs1 = rs1;
    dut->i_imm = imm;
    dut->i_funct3 = funct3;
    dut->i_rd_idx = 7;
    dut->i_rd_en = 1;
    dut->icb_rsp_valid = 1;
    dut->icb_rsp_rdata = rdata;
    dut->eval();
}

int main() {
    dut = new Ve203_exu_agu;

    // ── Test 1: Idle — no command without valid ──────────────────────
    printf("Test 1: Idle state\n");
    clear_inputs();
    CHECK(dut->icb_cmd_valid == 0, "cmd_valid should be 0 when i_valid is 0, got %d", dut->icb_cmd_valid);
    CHECK(dut->o_valid == 0, "o_valid should be 0 when idle, got %d", dut->o_valid);
    // i_valid without load or store must not issue a memory command.
    dut->i_valid = 1;
    dut->eval();
    CHECK(dut->icb_cmd_valid == 0, "cmd_valid should stay 0 without load/store, got %d", dut->icb_cmd_valid);

    // ── Test 2: Effective address = rs1 + imm, word-aligned on the bus ─
    printf("Test 2: Address computation\n");
    drive_store(0x1000, 0x10, 0xDEADBEEF, F3_W);
    CHECK(dut->icb_cmd_valid == 1, "cmd_valid should be 1 for a store, got %d", dut->icb_cmd_valid);
    CHECK(dut->icb_cmd_addr == 0x1010, "SW addr should be 0x1010, got 0x%08x", dut->icb_cmd_addr);
    CHECK(dut->icb_cmd_read == 0, "cmd_read should be 0 for a store, got %d", dut->icb_cmd_read);
    CHECK(dut->icb_cmd_wmask == 0xF, "SW wmask should be 0xF, got 0x%x", dut->icb_cmd_wmask);
    CHECK(dut->icb_cmd_wdata == 0xDEADBEEF, "SW wdata should pass through, got 0x%08x", dut->icb_cmd_wdata);

    // Negative offset: imm = -4 (two's complement) → 0x1000 - 4 = 0xFFC.
    drive_store(0x1000, 0xFFFFFFFCu, 0x12345678, F3_W);
    CHECK(dut->icb_cmd_addr == 0xFFC, "addr with imm=-4 should be 0xFFC, got 0x%08x", dut->icb_cmd_addr);

    // 32-bit wrap-around: 0xFFFFFFFF + 2 = 0x1 → word addr 0x0.
    drive_store(0xFFFFFFFFu, 2, 0xAB, F3_B);
    CHECK(dut->icb_cmd_addr == 0x0, "wrapped addr should word-align to 0x0, got 0x%08x", dut->icb_cmd_addr);

    // Unaligned byte address is word-aligned on the bus (offset moves to wmask).
    drive_store(0x2001, 2, 0xAB, F3_B);   // eff 0x2003 → word 0x2000, byte lane 3
    CHECK(dut->icb_cmd_addr == 0x2000, "byte store addr should word-align, got 0x%08x", dut->icb_cmd_addr);

    // ── Test 3: Store byte-enables per offset ────────────────────────
    printf("Test 3: Store byte-enables\n");
    for (uint32_t off = 0; off < 4; off++) {
        drive_store(0x3000 + off, 0, 0x5A, F3_B);
        uint8_t exp = 1u << off;
        CHECK(dut->icb_cmd_wmask == exp, "SB@+%u wmask should be 0x%x, got 0x%x", off, exp, dut->icb_cmd_wmask);
        CHECK(dut->icb_cmd_wdata == (0x5Au << (8 * off)),
              "SB@+%u wdata should be byte in lane %u, got 0x%08x", off, off, dut->icb_cmd_wdata);
    }
    // Halfword: low half → 0x3, high half → 0xC.
    drive_store(0x3000, 0, 0xBEEF, F3_H);
    CHECK(dut->icb_cmd_wmask == 0x3, "SH@+0 wmask should be 0x3, got 0x%x", dut->icb_cmd_wmask);
    CHECK(dut->icb_cmd_wdata == 0xBEEF, "SH@+0 wdata should be 0xBEEF, got 0x%08x", dut->icb_cmd_wdata);
    drive_store(0x3002, 0, 0xBEEF, F3_H);
    CHECK(dut->icb_cmd_wmask == 0xC, "SH@+2 wmask should be 0xC, got 0x%x", dut->icb_cmd_wmask);
    CHECK(dut->icb_cmd_wdata == 0xBEEF0000u, "SH@+2 wdata should be shifted, got 0x%08x", dut->icb_cmd_wdata);
    // Loads carry no write mask.
    drive_load(0x3000, 0, F3_W, 0);
    CHECK(dut->icb_cmd_wmask == 0, "load wmask should be 0, got 0x%x", dut->icb_cmd_wmask);
    CHECK(dut->icb_cmd_read == 1, "cmd_read should be 1 for a load, got %d", dut->icb_cmd_read);

    // ── Test 4: Load extraction and extension ────────────────────────
    printf("Test 4: Load sign/zero extension\n");
    const uint32_t MEMW = 0x80FF7F01u;  // bytes: [3]=0x80 [2]=0xFF [1]=0x7F [0]=0x01
    // LW passes through.
    drive_load(0x4000, 0, F3_W, MEMW);
    CHECK(dut->o_wdat == MEMW, "LW should pass 0x%08x, got 0x%08x", MEMW, dut->o_wdat);
    // LB: byte 0 = 0x01 → 0x00000001 (positive).
    drive_load(0x4000, 0, F3_B, MEMW);
    CHECK(dut->o_wdat == 0x1, "LB@+0 should sext 0x01 -> 0x1, got 0x%08x", dut->o_wdat);
    // LB: byte 2 = 0xFF → sign-extends to all ones.
    drive_load(0x4002, 0, F3_B, MEMW);
    CHECK(dut->o_wdat == 0xFFFFFFFFu, "LB@+2 should sext 0xFF -> -1, got 0x%08x", dut->o_wdat);
    // LBU: byte 3 = 0x80 → zero-extends.
    drive_load(0x4003, 0, F3_BU, MEMW);
    CHECK(dut->o_wdat == 0x80, "LBU@+3 should zext 0x80, got 0x%08x", dut->o_wdat);
    // LB: byte 3 = 0x80 → sign-extends.
    drive_load(0x4003, 0, F3_B, MEMW);
    CHECK(dut->o_wdat == 0xFFFFFF80u, "LB@+3 should sext 0x80, got 0x%08x", dut->o_wdat);
    // LH: low half 0x7F01 positive.
    drive_load(0x4000, 0, F3_H, MEMW);
    CHECK(dut->o_wdat == 0x7F01, "LH@+0 should sext 0x7F01, got 0x%08x", dut->o_wdat);
    // LH: high half 0x80FF negative.
    drive_load(0x4002, 0, F3_H, MEMW);
    CHECK(dut->o_wdat == 0xFFFF80FFu, "LH@+2 should sext 0x80FF, got 0x%08x", dut->o_wdat);
    // LHU: high half zero-extends.
    drive_load(0x4002, 0, F3_HU, MEMW);
    CHECK(dut->o_wdat == 0x80FF, "LHU@+2 should zext 0x80FF, got 0x%08x", dut->o_wdat);

    // ── Test 5: Handshake plumbing ───────────────────────────────────
    printf("Test 5: Handshakes\n");
    // i_ready mirrors icb_cmd_ready.
    drive_store(0x5000, 0, 0, F3_W);
    CHECK(dut->i_ready == 1, "i_ready should follow cmd_ready=1, got %d", dut->i_ready);
    dut->icb_cmd_ready = 0;
    dut->eval();
    CHECK(dut->i_ready == 0, "i_ready should follow cmd_ready=0, got %d", dut->i_ready);
    // icb_rsp_ready mirrors o_ready.
    drive_load(0x5000, 0, F3_W, 0x42);
    CHECK(dut->icb_rsp_ready == 1, "rsp_ready should follow o_ready=1, got %d", dut->icb_rsp_ready);
    dut->o_ready = 0;
    dut->eval();
    CHECK(dut->icb_rsp_ready == 0, "rsp_ready should follow o_ready=0, got %d", dut->icb_rsp_ready);

    // ── Test 6: Writeback gating ─────────────────────────────────────
    printf("Test 6: Writeback gating\n");
    drive_load(0x6000, 0, F3_W, 0x99);
    CHECK(dut->o_valid == 1, "o_valid should be 1 with rsp_valid & load, got %d", dut->o_valid);
    CHECK(dut->o_rd_en == 1, "o_rd_en should be 1 for a load with rd_en, got %d", dut->o_rd_en);
    CHECK(dut->o_rd_idx == 7, "o_rd_idx should pass through 7, got %d", dut->o_rd_idx);
    // No response → no writeback.
    dut->icb_rsp_valid = 0;
    dut->eval();
    CHECK(dut->o_valid == 0, "o_valid should drop without rsp_valid, got %d", dut->o_valid);
    // Stores never write back even if a (spurious) response is present.
    drive_store(0x6000, 0, 0x1, F3_W);
    dut->icb_rsp_valid = 1;
    dut->i_rd_en = 1;
    dut->eval();
    CHECK(dut->o_valid == 0, "o_valid should be 0 for a store, got %d", dut->o_valid);
    CHECK(dut->o_rd_en == 0, "o_rd_en should be 0 for a store, got %d", dut->o_rd_en);

    printf("\n=== e203_exu_agu: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
