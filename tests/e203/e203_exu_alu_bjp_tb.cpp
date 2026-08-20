// ARCH sim testbench for e203_exu_alu_bjp — E203 branch/jump resolution unit.
// Tests: info-bus decode into the shared-ALU compare/add request one-hots,
// branch resolution (taken = cmp result, only for conditional branches),
// jump always-taken, JAL/JALR link writeback (PC+4), mret/dret/fencei commit
// flags, prediction bit pass-through (mispredict detection = prdt vs rslv is
// commit's job), and handshake pass-through.
//
// NOTE: this replaces a stale tb (VBjpUnit.h) that targeted an earlier
// revision of the fixture before the e203 fixtures were rewritten against the
// real E203 RTL, which renamed the construct to `e203_exu_alu_bjp`. The old tb
// has not compiled since. Ported to the current class name (Ve203_exu_alu_bjp).
//
// The module is purely combinational (clk/rst_n ports exist but no state):
// drive inputs, eval(), check.
//
// Run with:
//   arch sim tests/e203/e203_exu_alu_bjp.arch --tb tests/e203/e203_exu_alu_bjp_tb.cpp

#include "Ve203_exu_alu_bjp.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_exu_alu_bjp* dut;

// info bus bit positions (see the .arch decode)
enum {
    I_BEQ = 1 << 0, I_BNE = 1 << 1, I_BLT = 1 << 2, I_BGE = 1 << 3,
    I_BLTU = 1 << 4, I_BGEU = 1 << 5, I_JAL = 1 << 6, I_JALR = 1 << 7,
    I_MRET = 1 << 8, I_DRET = 1 << 9, I_FENCEI = 1 << 10, I_PRDT = 1 << 11,
};

static void clear_inputs() {
    dut->clk = 0;
    dut->rst_n = 1;
    dut->bjp_i_valid = 0;
    dut->bjp_i_rs1 = 0;
    dut->bjp_i_rs2 = 0;
    dut->bjp_i_imm = 0;
    dut->bjp_i_pc = 0;
    dut->bjp_i_info = 0;
    dut->bjp_o_ready = 1;
    dut->bjp_req_alu_cmp_res = 0;
    dut->bjp_req_alu_add_res = 0;
    dut->eval();
}

// Present an op with the given info bits and shared-ALU cmp result.
static void drive(uint32_t info, uint8_t cmp_res, uint32_t pc = 0x1000) {
    clear_inputs();
    dut->bjp_i_valid = 1;
    dut->bjp_i_info = info;
    dut->bjp_i_pc = pc;
    dut->bjp_i_rs1 = 0x11;
    dut->bjp_i_rs2 = 0x22;
    dut->bjp_req_alu_cmp_res = cmp_res;
    dut->eval();
}

int main() {
    dut = new Ve203_exu_alu_bjp;

    // ── Test 1: Compare-request one-hot decode per branch type ───────
    printf("Test 1: Branch type -> cmp request decode\n");
    struct { uint32_t info; const char* name;
             uint8_t eq, ne, lt, gt, ltu, gtu; } cases[] = {
        { I_BEQ,  "beq",  1,0,0,0,0,0 },
        { I_BNE,  "bne",  0,1,0,0,0,0 },
        { I_BLT,  "blt",  0,0,1,0,0,0 },
        { I_BGE,  "bge",  0,0,0,1,0,0 },
        { I_BLTU, "bltu", 0,0,0,0,1,0 },
        { I_BGEU, "bgeu", 0,0,0,0,0,1 },
    };
    for (auto& c : cases) {
        drive(c.info, 0);
        CHECK(dut->bjp_req_alu_cmp_eq == c.eq,  "%s: cmp_eq should be %d",  c.name, c.eq);
        CHECK(dut->bjp_req_alu_cmp_ne == c.ne,  "%s: cmp_ne should be %d",  c.name, c.ne);
        CHECK(dut->bjp_req_alu_cmp_lt == c.lt,  "%s: cmp_lt should be %d",  c.name, c.lt);
        CHECK(dut->bjp_req_alu_cmp_gt == c.gt,  "%s: cmp_gt should be %d",  c.name, c.gt);
        CHECK(dut->bjp_req_alu_cmp_ltu == c.ltu, "%s: cmp_ltu should be %d", c.name, c.ltu);
        CHECK(dut->bjp_req_alu_cmp_gtu == c.gtu, "%s: cmp_gtu should be %d", c.name, c.gtu);
        CHECK(dut->bjp_req_alu_add == 1, "%s: add should be requested for target calc", c.name);
        CHECK(dut->bjp_req_alu_op1 == 0x11, "%s: op1 should be rs1", c.name);
        CHECK(dut->bjp_req_alu_op2 == 0x22, "%s: op2 should be rs2", c.name);
        CHECK(dut->bjp_o_cmt_bjp == 1, "%s: cmt_bjp should be 1", c.name);
    }

    // ── Test 2: Branch resolution follows the cmp result ─────────────
    printf("Test 2: Branch resolution\n");
    drive(I_BEQ, 1);                       // beq, operands equal
    CHECK(dut->bjp_o_cmt_rslv == 1, "beq with cmp_res=1 should resolve taken, got %d", dut->bjp_o_cmt_rslv);
    drive(I_BEQ, 0);                       // beq, operands unequal
    CHECK(dut->bjp_o_cmt_rslv == 0, "beq with cmp_res=0 should resolve not-taken, got %d", dut->bjp_o_cmt_rslv);
    // cmp_res on a non-branch op must not fake a taken branch.
    drive(I_MRET, 1);
    CHECK(dut->bjp_o_cmt_rslv == 0, "mret must not resolve taken from a stray cmp_res, got %d", dut->bjp_o_cmt_rslv);

    // ── Test 3: Jumps are unconditionally taken, write link = PC+4 ───
    printf("Test 3: JAL/JALR\n");
    drive(I_JAL, 0, 0x2000);               // jal: taken regardless of cmp_res
    CHECK(dut->bjp_o_cmt_rslv == 1, "jal should always resolve taken, got %d", dut->bjp_o_cmt_rslv);
    CHECK(dut->bjp_o_cmt_bjp == 1, "jal: cmt_bjp should be 1, got %d", dut->bjp_o_cmt_bjp);
    CHECK(dut->bjp_o_wbck_wdat == 0x2004, "jal link should be pc+4 = 0x2004, got 0x%08x", dut->bjp_o_wbck_wdat);
    CHECK(dut->bjp_req_alu_add == 1, "jal should request the adder, got %d", dut->bjp_req_alu_add);
    drive(I_JALR, 0, 0xFFFFFFFCu);         // jalr at the top of memory: pc+4 wraps
    CHECK(dut->bjp_o_cmt_rslv == 1, "jalr should always resolve taken, got %d", dut->bjp_o_cmt_rslv);
    CHECK(dut->bjp_o_wbck_wdat == 0x0, "jalr link at 0xFFFFFFFC should wrap to 0x0, got 0x%08x", dut->bjp_o_wbck_wdat);
    // Branches write no link data.
    drive(I_BNE, 1, 0x2000);
    CHECK(dut->bjp_o_wbck_wdat == 0, "branch wbck_wdat should be 0, got 0x%08x", dut->bjp_o_wbck_wdat);

    // ── Test 4: mret / dret / fencei commit flags ────────────────────
    printf("Test 4: System op commit flags\n");
    drive(I_MRET, 0);
    CHECK(dut->bjp_o_cmt_mret == 1, "mret flag should be 1, got %d", dut->bjp_o_cmt_mret);
    CHECK(dut->bjp_o_cmt_bjp == 0, "mret is not a bjp commit, got %d", dut->bjp_o_cmt_bjp);
    CHECK(dut->bjp_req_alu_add == 0, "mret should not request the adder, got %d", dut->bjp_req_alu_add);
    drive(I_DRET, 0);
    CHECK(dut->bjp_o_cmt_dret == 1, "dret flag should be 1, got %d", dut->bjp_o_cmt_dret);
    CHECK(dut->bjp_o_cmt_mret == 0, "dret must not set mret, got %d", dut->bjp_o_cmt_mret);
    drive(I_FENCEI, 0);
    CHECK(dut->bjp_o_cmt_fencei == 1, "fencei flag should be 1, got %d", dut->bjp_o_cmt_fencei);

    // ── Test 5: Prediction bit pass-through (mispredict material) ────
    printf("Test 5: Prediction pass-through\n");
    // Predicted-taken beq that resolves not-taken -> commit sees prdt=1, rslv=0.
    drive(I_BEQ | I_PRDT, 0);
    CHECK(dut->bjp_o_cmt_prdt == 1, "prdt should pass through as 1, got %d", dut->bjp_o_cmt_prdt);
    CHECK(dut->bjp_o_cmt_rslv == 0, "mispredicted beq resolves 0, got %d", dut->bjp_o_cmt_rslv);
    // Predicted-not-taken bne that resolves taken -> prdt=0, rslv=1.
    drive(I_BNE, 1);
    CHECK(dut->bjp_o_cmt_prdt == 0, "prdt should pass through as 0, got %d", dut->bjp_o_cmt_prdt);
    CHECK(dut->bjp_o_cmt_rslv == 1, "taken bne resolves 1, got %d", dut->bjp_o_cmt_rslv);

    // ── Test 6: Handshake pass-through ───────────────────────────────
    printf("Test 6: Handshakes\n");
    drive(I_JAL, 0);
    CHECK(dut->bjp_o_valid == 1, "o_valid should mirror i_valid=1, got %d", dut->bjp_o_valid);
    CHECK(dut->bjp_i_ready == 1, "i_ready should mirror o_ready=1, got %d", dut->bjp_i_ready);
    CHECK(dut->bjp_o_wbck_err == 0, "wbck_err is tied low, got %d", dut->bjp_o_wbck_err);
    dut->bjp_o_ready = 0;
    dut->eval();
    CHECK(dut->bjp_i_ready == 0, "i_ready should mirror o_ready=0, got %d", dut->bjp_i_ready);
    dut->bjp_i_valid = 0;
    dut->eval();
    CHECK(dut->bjp_o_valid == 0, "o_valid should mirror i_valid=0, got %d", dut->bjp_o_valid);

    printf("\n=== e203_exu_alu_bjp: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
