// Testbench for E203 ExuDisp — arch sim
#include "VExuDisp.h"
#include <cstdio>

static int errors = 0;
static int test_num = 0;

#define CHECK(cond, ...) do { \
    test_num++; \
    if (!(cond)) { errors++; printf("FAIL test %d: ", test_num); printf(__VA_ARGS__); printf("\n"); } \
    else { printf("PASS test %d\n", test_num); } \
} while(0)

int main() {
    VExuDisp m;
    // Zero everything
    m.disp_valid = 0; m.i_rs1 = 0; m.i_rs2 = 0; m.i_pc = 0; m.i_imm = 0;
    m.i_rd_idx = 0; m.i_rd_en = 0;
    m.i_alu = 0; m.i_bjp = 0; m.i_agu = 0; m.i_load = 0; m.i_store = 0;
    m.i_mul = 0; m.i_div = 0;
    m.i_alu_add = 0; m.i_alu_sub = 0; m.i_alu_xor = 0;
    m.i_alu_sll = 0; m.i_alu_srl = 0; m.i_alu_sra = 0;
    m.i_alu_or = 0; m.i_alu_and = 0; m.i_alu_slt = 0;
    m.i_alu_sltu = 0; m.i_alu_lui = 0;
    m.i_beq = 0; m.i_bne = 0; m.i_blt = 0; m.i_bge = 0;
    m.i_bltu = 0; m.i_bgeu = 0; m.i_jump = 0;
    m.alu_ready = 1; m.mdv_ready = 1; m.lsu_ready = 1;
    m.eval();

    // ── ALU dispatch ─────────────────────────────────────────────────
    m.disp_valid = 1; m.i_alu = 1; m.i_alu_add = 1;
    m.i_rs1 = 100; m.i_rs2 = 200; m.i_pc = 0x1000; m.i_imm = 42;
    m.i_rd_idx = 5; m.i_rd_en = 1;
    m.eval();
    CHECK(m.alu_valid == 1, "ALU: alu_valid=%d", m.alu_valid);
    CHECK(m.mdv_valid == 0, "ALU: mdv_valid=%d", m.mdv_valid);
    CHECK(m.lsu_valid == 0, "ALU: lsu_valid=%d", m.lsu_valid);
    CHECK(m.disp_ready == 1, "ALU: disp_ready=%d", m.disp_ready);
    CHECK(m.alu_rs1 == 100, "ALU: rs1=%d", m.alu_rs1);
    CHECK(m.alu_rs2 == 200, "ALU: rs2=%d", m.alu_rs2);
    CHECK(m.alu_pc == 0x1000, "ALU: pc=0x%X", m.alu_pc);
    CHECK(m.alu_imm == 42, "ALU: imm=%d", m.alu_imm);
    CHECK(m.alu_rdidx == 5, "ALU: rdidx=%d", m.alu_rdidx);
    CHECK(m.alu_op_add == 1, "ALU: op_add=%d", m.alu_op_add);

    // ── BJP dispatch ─────────────────────────────────────────────────
    m.i_alu = 0; m.i_alu_add = 0; m.i_bjp = 1; m.i_beq = 1;
    m.eval();
    CHECK(m.alu_valid == 1, "BJP: routes to ALU, valid=%d", m.alu_valid);
    CHECK(m.alu_is_bjp == 1, "BJP: is_bjp=%d", m.alu_is_bjp);
    CHECK(m.alu_beq == 1, "BJP: beq=%d", m.alu_beq);

    // ── MulDiv dispatch ──────────────────────────────────────────────
    m.i_bjp = 0; m.i_beq = 0; m.i_mul = 1;
    m.i_rs1 = 7; m.i_rs2 = 3;
    m.eval();
    CHECK(m.mdv_valid == 1, "MUL: mdv_valid=%d", m.mdv_valid);
    CHECK(m.alu_valid == 0, "MUL: alu_valid=%d", m.alu_valid);
    CHECK(m.mdv_rs1 == 7, "MUL: rs1=%d", m.mdv_rs1);
    CHECK(m.mdv_mul == 1, "MUL: mul=%d", m.mdv_mul);
    CHECK(m.disp_ready == 1, "MUL: ready=%d", m.disp_ready);

    // ── LSU dispatch (store) ─────────────────────────────────────────
    m.i_mul = 0; m.i_store = 1;
    m.i_rs1 = 0x2000; m.i_rs2 = 0xABCD; m.i_imm = 8;
    m.eval();
    CHECK(m.lsu_valid == 1, "SW: lsu_valid=%d", m.lsu_valid);
    CHECK(m.alu_valid == 0, "SW: alu_valid=%d", m.alu_valid);
    CHECK(m.lsu_rs1 == 0x2000, "SW: rs1=0x%X", m.lsu_rs1);
    CHECK(m.lsu_rs2 == 0xABCD, "SW: rs2=0x%X", m.lsu_rs2);
    CHECK(m.lsu_imm == 8, "SW: imm=%d", m.lsu_imm);
    CHECK(m.lsu_store == 1, "SW: store=%d", m.lsu_store);
    CHECK(m.lsu_load == 0, "SW: load=%d", m.lsu_load);

    // ── Backpressure: ALU not ready ──────────────────────────────────
    m.i_store = 0; m.i_alu = 1; m.i_alu_add = 1;
    m.alu_ready = 0;
    m.eval();
    CHECK(m.disp_ready == 0, "backpressure: disp_ready=%d", m.disp_ready);
    CHECK(m.alu_valid == 1, "backpressure: alu_valid still 1");

    // ── No dispatch when invalid ─────────────────────────────────────
    m.disp_valid = 0;
    m.eval();
    CHECK(m.alu_valid == 0, "invalid: alu_valid=%d", m.alu_valid);

    printf("\n=== ExuDisp: %d tests, %d errors ===\n", test_num, errors);
    return errors ? 1 : 0;
}
