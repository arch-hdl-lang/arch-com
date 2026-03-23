// Functional testbench for E203 IfuTop — arch sim
// Models ITCM as a simple memory array and verifies the full
// fetch pipeline: request → ITCM → response → instruction delivery.
#include "VIfuTop.h"
#include <cstdio>
#include <cstdint>
#include <cstring>

static int errors = 0;
static int test_num = 0;

#define BOOL(x) ((x) & 1)
#define CHECK(cond, ...) do { \
    test_num++; \
    if (!(cond)) { errors++; printf("FAIL test %d: ", test_num); printf(__VA_ARGS__); printf("\n"); } \
    else { printf("PASS test %d\n", test_num); } \
} while(0)

// Simple ITCM model: 16K words
static uint32_t itcm[16384];

static void settle(VIfuTop &m) {
    for (int i = 0; i < 8; i++) m.eval();
}

// Clock tick with ITCM model: respond to cmd with 1-cycle latency via rsp
static void tick(VIfuTop &m) {
    // Drive ITCM response based on previous cycle's cmd
    // (Ift2Icb has its own 1-cycle pipeline, so we respond immediately)
    if (BOOL(m.itcm_cmd_valid) && BOOL(m.itcm_cmd_ready)) {
        m.itcm_rsp_valid = 1;
        m.itcm_rsp_data = itcm[m.itcm_cmd_addr & 0x3FFF];
    } else {
        m.itcm_rsp_valid = 0;
    }
    m.clk = 0; settle(m);
    m.clk = 1; settle(m);
}

// RV32I encoding helpers
static uint32_t rv_addi(int rd, int rs1, int imm) {
    return ((imm & 0xFFF) << 20) | (rs1 << 15) | (0b000 << 12) | (rd << 7) | 0b0010011;
}
static uint32_t rv_add(int rd, int rs1, int rs2) {
    return (0b0000000 << 25) | (rs2 << 20) | (rs1 << 15) | (0b000 << 12) | (rd << 7) | 0b0110011;
}
static uint32_t rv_jal(int rd, int imm21) {
    // J-type: {imm[20], imm[10:1], imm[11], imm[19:12], rd, 1101111}
    uint32_t i20   = (imm21 >> 20) & 1;
    uint32_t i10_1 = (imm21 >> 1) & 0x3FF;
    uint32_t i11   = (imm21 >> 11) & 1;
    uint32_t i19_12= (imm21 >> 12) & 0xFF;
    return (i20 << 31) | (i10_1 << 21) | (i11 << 20) | (i19_12 << 12) | (rd << 7) | 0b1101111;
}
static uint32_t rv_beq(int rs1, int rs2, int imm13) {
    // B-type: {imm[12], imm[10:5], rs2, rs1, 000, imm[4:1], imm[11], 1100011}
    uint32_t i12  = (imm13 >> 12) & 1;
    uint32_t i10_5= (imm13 >> 5) & 0x3F;
    uint32_t i4_1 = (imm13 >> 1) & 0xF;
    uint32_t i11  = (imm13 >> 11) & 1;
    return (i12 << 31) | (i10_5 << 25) | (rs2 << 20) | (rs1 << 15) |
           (0b000 << 12) | (i4_1 << 8) | (i11 << 7) | 0b1100011;
}
static uint32_t rv_nop() { return rv_addi(0, 0, 0); }

// Wait for o_valid to assert, return the delivered instruction
// Returns true if o_valid was seen within max_cycles
static bool wait_valid(VIfuTop &m, int max_cycles, uint32_t &instr, uint32_t &pc) {
    for (int i = 0; i < max_cycles; i++) {
        tick(m);
        settle(m);
        if (BOOL(m.o_valid)) {
            instr = m.o_instr;
            pc = m.o_pc;
            return true;
        }
    }
    return false;
}

int main() {
    VIfuTop m;
    memset(itcm, 0, sizeof(itcm));

    // Load a small program at ITCM base (RESET_PC = 0x80000000)
    // But ITCM is only 64KB, so PC 0x80000000 wraps to word addr 0x0
    // Actually, Ift2Icb uses ifu_req_pc[15:2] — so 0x80000000 → addr 0x0000
    // Let's put instructions at word offset 0
    itcm[0] = rv_addi(1, 0, 42);     // ADDI x1, x0, 42
    itcm[1] = rv_addi(2, 0, 10);     // ADDI x2, x0, 10
    itcm[2] = rv_add(3, 1, 2);       // ADD  x3, x1, x2
    itcm[3] = rv_nop();              // NOP
    itcm[4] = rv_nop();              // NOP

    // Also load instructions at PC 0x2000 for redirect test
    // 0x2000 → word addr 0x800
    itcm[0x800] = rv_addi(5, 0, 99); // ADDI x5, x0, 99
    itcm[0x801] = rv_addi(6, 0, 77); // ADDI x6, x0, 77

    // Reset
    m.clk = 0; m.rst_n = 0;
    m.o_ready = 1;
    m.exu_redirect = 0; m.exu_redirect_pc = 0;
    m.itcm_cmd_ready = 1;
    m.itcm_rsp_valid = 0; m.itcm_rsp_data = 0;
    m.oitf_empty = 1; m.ir_empty = 1; m.ir_rs1en = 0;
    m.jalr_rs1idx_cam_irrdidx = 0; m.ir_valid_clr = 0;
    m.rf2bpu_x1 = 0; m.rf2bpu_rs1 = 0;
    m.eval();
    tick(m); tick(m);
    m.rst_n = 1;
    tick(m);

    // ── Test 1: After reset, IFU issues fetch request ─────────────────
    settle(m);
    CHECK(BOOL(m.itcm_cmd_valid), "post-reset: cmd_valid=%d", m.itcm_cmd_valid);

    // ── Test 2: First instruction delivered (ADDI x1, x0, 42) ────────
    uint32_t got_instr, got_pc;
    bool ok = wait_valid(m, 10, got_instr, got_pc);
    CHECK(ok, "first instr: o_valid not seen within 10 cycles");
    CHECK(got_instr == rv_addi(1, 0, 42),
          "first instr: got 0x%08X exp 0x%08X", got_instr, rv_addi(1, 0, 42));

    // ── Test 4: Second instruction delivered (ADDI x2, x0, 10) ───────
    ok = wait_valid(m, 10, got_instr, got_pc);
    CHECK(ok, "second instr: o_valid not seen");
    CHECK(got_instr == rv_addi(2, 0, 10),
          "second instr: got 0x%08X exp 0x%08X", got_instr, rv_addi(2, 0, 10));

    // ── Test 6: Third instruction (ADD x3, x1, x2) ──────────────────
    ok = wait_valid(m, 10, got_instr, got_pc);
    CHECK(ok, "third instr: o_valid not seen");
    CHECK(got_instr == rv_add(3, 1, 2),
          "third instr: got 0x%08X exp 0x%08X", got_instr, rv_add(3, 1, 2));

    // ── Test 8: PC increments by 4 each fetch ────────────────────────
    // got_pc should correspond to instruction at word offset 2
    // PC = 0x80000000 + 8 = 0x80000008
    CHECK(got_pc == 0x80000008u,
          "third instr PC: got 0x%08X exp 0x80000008", got_pc);

    // ── Test 9: Branch redirect to 0x2000 ────────────────────────────
    m.exu_redirect = 1;
    m.exu_redirect_pc = 0x2000;
    tick(m);
    m.exu_redirect = 0;

    // Wait for instruction from new PC
    ok = wait_valid(m, 15, got_instr, got_pc);
    CHECK(ok, "redirect: o_valid not seen after redirect");
    CHECK(got_instr == rv_addi(5, 0, 99),
          "redirect instr: got 0x%08X exp 0x%08X", got_instr, rv_addi(5, 0, 99));

    // ── Test 11: Instruction after redirect is sequential ────────────
    ok = wait_valid(m, 10, got_instr, got_pc);
    CHECK(ok, "post-redirect seq: o_valid not seen");
    CHECK(got_instr == rv_addi(6, 0, 77),
          "post-redirect seq: got 0x%08X exp 0x%08X", got_instr, rv_addi(6, 0, 77));

    // ── Test 13: Backpressure — deassert o_ready ─────────────────────
    m.o_ready = 0;
    // Let several cycles pass; IFU should stall (no new o_valid)
    for (int i = 0; i < 5; i++) tick(m);
    // Re-enable
    m.o_ready = 1;
    ok = wait_valid(m, 10, got_instr, got_pc);
    CHECK(ok, "backpressure: o_valid recovered after ready re-asserted");

    // ── Test 14: Mini-decoder flags ──────────────────────────────────
    // Load a JAL at next expected fetch location and check dec_is_bjp
    // The current PC after redirect sequence is around 0x2008+
    // Instead, redirect to a known location with JAL
    uint32_t jal_addr = 0x3000;
    itcm[jal_addr >> 2] = rv_jal(0, 0x100);  // JAL x0, +256
    m.exu_redirect = 1;
    m.exu_redirect_pc = jal_addr;
    tick(m);
    m.exu_redirect = 0;

    ok = wait_valid(m, 15, got_instr, got_pc);
    CHECK(ok, "JAL fetch: o_valid seen");
    settle(m);
    CHECK(BOOL(m.dec_is_bjp), "JAL: dec_is_bjp=%d", m.dec_is_bjp);
    CHECK(BOOL(m.prdt_taken), "JAL: prdt_taken=%d (JAL always taken)", m.prdt_taken);

    // ── Test 17: BEQ instruction — backward branch predicted taken ───
    uint32_t beq_addr = 0x4000;
    // BEQ x0, x0, -8 (backward branch → predicted taken by LiteBpu)
    itcm[beq_addr >> 2] = rv_beq(0, 0, -8 & 0x1FFF);
    m.exu_redirect = 1;
    m.exu_redirect_pc = beq_addr;
    tick(m);
    m.exu_redirect = 0;

    ok = wait_valid(m, 15, got_instr, got_pc);
    CHECK(ok, "BEQ fetch: o_valid seen");
    settle(m);
    CHECK(BOOL(m.dec_is_bjp), "BEQ: dec_is_bjp=%d", m.dec_is_bjp);
    // Backward branch → prdt_taken should be 1
    CHECK(BOOL(m.prdt_taken), "BEQ backward: prdt_taken=%d", m.prdt_taken);

    // ── Test 19: Non-branch instruction → dec_is_bjp=0 ──────────────
    uint32_t nop_addr = 0x5000;
    itcm[nop_addr >> 2] = rv_nop();
    m.exu_redirect = 1;
    m.exu_redirect_pc = nop_addr;
    tick(m);
    m.exu_redirect = 0;

    ok = wait_valid(m, 15, got_instr, got_pc);
    CHECK(ok, "NOP fetch: o_valid seen");
    settle(m);
    CHECK(!BOOL(m.dec_is_bjp), "NOP: dec_is_bjp=%d", m.dec_is_bjp);
    CHECK(!BOOL(m.prdt_taken), "NOP: prdt_taken=%d", m.prdt_taken);

    printf("\n=== IfuTop functional test: %d tests, %d errors ===\n", test_num, errors);
    return errors ? 1 : 0;
}
