// ARCH sim testbench for e203_exu_alu — E203 ALU top-level.
// Tests: i_info decode and routing to the ALU/BJP/CSR/AGU/NICE sub-paths,
// ALU op results (incl. signed slt/sra edge cases), branch resolution and
// prdt/rslv commit reporting, JAL link writeback, CSR request wiring and CSR
// read-data writeback, AGU ICB command formation (addr/read/size/usign/
// lock for AMO) and long-pipe flagging, flush suppression of writeback/
// commit/CSR/AGU/NICE valids, dispatch-ready gating, and the NICE long-pipe
// tracking register.
//
// NOTE: this replaces a stale tb (VExuAlu.h) that targeted an earlier
// revision of the fixture before the e203 fixtures were rewritten against the
// real E203 RTL, which renamed the construct to `e203_exu_alu`. The old tb
// (and its e203_exu_alu_vltor_tb.cpp Verilator twin, deleted with this
// rewrite) has not compiled since. Ported to the current class name
// (Ve203_exu_alu).
//
// Run with:
//   arch sim tests/e203/e203_exu_alu.arch --tb tests/e203/e203_exu_alu_tb.cpp

#include "Ve203_exu_alu.h"
#include <cstdio>
#include <cstdint>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static Ve203_exu_alu* dut;

// i_info bit assignments (see the .arch decode comment)
enum : uint32_t {
    OP_ALU = 1u << 0, OP_BJP = 1u << 1, OP_CSR = 1u << 2, OP_AGU = 1u << 3,
    OP_NICE = 1u << 4, OP_MRET = 1u << 5, OP_DRET = 1u << 6, OP_ECALL = 1u << 7,
    OP_EBREAK = 1u << 8, OP_FENCEI = 1u << 9, OP_WFI = 1u << 10,
    OP_RV32 = 1u << 11, OP_PRDT = 1u << 12,
    // ALU sub-ops
    A_ADD = 1u << 13, A_SUB = 1u << 14, A_XOR = 1u << 15, A_SLL = 1u << 16,
    A_SRL = 1u << 17, A_SRA = 1u << 18, A_OR = 1u << 19, A_AND = 1u << 20,
    A_SLT = 1u << 21, A_SLTU = 1u << 22, A_LUI = 1u << 23,
    // BJP sub-ops
    B_BEQ = 1u << 13, B_BNE = 1u << 14, B_BLT = 1u << 15, B_BGE = 1u << 16,
    B_BLTU = 1u << 17, B_BGEU = 1u << 18, B_JUMP = 1u << 19,
    // AGU sub-ops
    G_LOAD = 1u << 13, G_STORE = 1u << 14, G_AMO = 1u << 15,
    G_USIGN = 1u << 26,
};

static void tick() {
    dut->clk = 0; dut->eval();
    dut->clk = 1; dut->eval();
}

static void reset() {
    dut->rst_n = 0;
    dut->i_valid = 0;
    dut->nice_xs_off = 0;
    dut->oitf_empty = 1;
    dut->i_itag = 0;
    dut->i_rs1 = 0; dut->i_rs2 = 0; dut->i_imm = 0;
    dut->i_info = 0; dut->i_pc = 0; dut->i_instr = 0;
    dut->i_pc_vld = 0; dut->i_rdidx = 0; dut->i_rdwen = 0;
    dut->i_ilegl = 0; dut->i_buserr = 0; dut->i_misalgn = 0;
    dut->flush_req = 0; dut->flush_pulse = 0;
    dut->cmt_o_ready = 1;
    dut->wbck_o_ready = 1;
    dut->mdv_nob2b = 0;
    dut->nonflush_cmt_ena = 0;
    dut->csr_access_ilgl = 0;
    dut->read_csr_dat = 0;
    dut->agu_icb_cmd_ready = 1;
    dut->agu_icb_rsp_valid = 0;
    dut->agu_icb_rsp_err = 0;
    dut->agu_icb_rsp_excl_ok = 0;
    dut->agu_icb_rsp_rdata = 0;
    dut->nice_req_ready = 1;
    dut->nice_rsp_multicyc_valid = 0;
    dut->nice_longp_wbck_ready = 1;
    dut->i_nice_cmt_off_ilgl = 0;
    for (int i = 0; i < 3; i++) tick();
    dut->rst_n = 1;
    dut->eval();
}

// Present an ALU op and return the writeback data.
static uint32_t alu_op(uint32_t subop, uint32_t rs1, uint32_t rs2, uint32_t imm = 0) {
    dut->i_valid = 1;
    dut->i_rdwen = 1;
    dut->i_rdidx = 3;
    dut->i_info = OP_ALU | subop;
    dut->i_rs1 = rs1;
    dut->i_rs2 = rs2;
    dut->i_imm = imm;
    dut->eval();
    return dut->wbck_o_wdat;
}

// Present a branch/jump and return cmt_o_bjp_rslv.
static uint8_t bjp_op(uint32_t subop, uint32_t rs1, uint32_t rs2) {
    dut->i_valid = 1;
    dut->i_info = OP_BJP | subop;
    dut->i_rs1 = rs1;
    dut->i_rs2 = rs2;
    dut->eval();
    return dut->cmt_o_bjp_rslv;
}

int main() {
    dut = new Ve203_exu_alu;

    // ── Test 1: Reset / idle ─────────────────────────────────────────
    printf("Test 1: Reset state\n");
    reset();
    CHECK(dut->cmt_o_valid == 0, "cmt_o_valid should be 0 when idle, got %d", dut->cmt_o_valid);
    CHECK(dut->wbck_o_valid == 0, "wbck_o_valid should be 0 when idle, got %d", dut->wbck_o_valid);
    CHECK(dut->csr_ena == 0, "csr_ena should be 0 when idle, got %d", dut->csr_ena);
    CHECK(dut->agu_icb_cmd_valid == 0, "agu cmd_valid should be 0 when idle, got %d", dut->agu_icb_cmd_valid);
    CHECK(dut->nice_req_valid == 0, "nice_req_valid should be 0 when idle, got %d", dut->nice_req_valid);
    CHECK(dut->i_ready == 1, "i_ready should be 1 with all sinks ready, got %d", dut->i_ready);

    // ── Test 2: ALU op results ───────────────────────────────────────
    printf("Test 2: ALU ops\n");
    reset();
    CHECK(alu_op(A_ADD, 5, 7) == 12, "add 5+7 should be 12, got 0x%08x", dut->wbck_o_wdat);
    CHECK(alu_op(A_ADD, 0xFFFFFFFFu, 1) == 0, "add should wrap, got 0x%08x", dut->wbck_o_wdat);
    CHECK(alu_op(A_SUB, 3, 10) == 0xFFFFFFF9u, "3-10 should be -7, got 0x%08x", dut->wbck_o_wdat);
    CHECK(alu_op(A_XOR, 0xFF00FF00u, 0x0FF00FF0u) == 0xF0F0F0F0u, "xor wrong, got 0x%08x", dut->wbck_o_wdat);
    CHECK(alu_op(A_OR, 0xF0F00000u, 0x0F0F0000u) == 0xFFFF0000u, "or wrong, got 0x%08x", dut->wbck_o_wdat);
    CHECK(alu_op(A_AND, 0xFF00FF00u, 0x0FF00FF0u) == 0x0F000F00u, "and wrong, got 0x%08x", dut->wbck_o_wdat);
    CHECK(alu_op(A_SLL, 1, 31) == 0x80000000u, "1<<31 wrong, got 0x%08x", dut->wbck_o_wdat);
    CHECK(alu_op(A_SRL, 0x80000000u, 31) == 1, "srl by 31 wrong, got 0x%08x", dut->wbck_o_wdat);
    CHECK(alu_op(A_SRA, 0x80000000u, 31) == 0xFFFFFFFFu, "sra INT_MIN by 31 should be -1, got 0x%08x", dut->wbck_o_wdat);
    CHECK(alu_op(A_SLT, 0x80000000u, 0x7FFFFFFFu) == 1, "slt(INT_MIN,INT_MAX) should be 1, got 0x%08x", dut->wbck_o_wdat);
    CHECK(alu_op(A_SLT, 1, (uint32_t)-1) == 0, "slt(1,-1) should be 0, got 0x%08x", dut->wbck_o_wdat);
    CHECK(alu_op(A_SLTU, 1, 0xFFFFFFFFu) == 1, "sltu(1,UINT_MAX) should be 1, got 0x%08x", dut->wbck_o_wdat);
    CHECK(alu_op(A_LUI, 0, 0, 0xABCDE000u) == 0xABCDE000u, "lui should pass imm, got 0x%08x", dut->wbck_o_wdat);
    // Writeback plumbing while an ALU op is live.
    CHECK(dut->wbck_o_valid == 1, "wbck_o_valid should be 1 for rdwen op, got %d", dut->wbck_o_valid);
    CHECK(dut->wbck_o_rdidx == 3, "wbck_o_rdidx should be 3, got %d", dut->wbck_o_rdidx);
    CHECK(dut->cmt_o_valid == 1, "cmt_o_valid should be 1, got %d", dut->cmt_o_valid);
    CHECK(dut->i_longpipe == 0, "ALU op is not long-pipe, got %d", dut->i_longpipe);
    // rdwen=0 suppresses writeback.
    dut->i_rdwen = 0;
    dut->eval();
    CHECK(dut->wbck_o_valid == 0, "wbck_o_valid should be 0 without rdwen, got %d", dut->wbck_o_valid);

    // ── Test 3: Branch resolution ────────────────────────────────────
    printf("Test 3: Branch resolution\n");
    reset();
    CHECK(bjp_op(B_BEQ, 42, 42) == 1, "beq(42,42) should resolve taken");
    CHECK(bjp_op(B_BEQ, 42, 43) == 0, "beq(42,43) should resolve not-taken");
    CHECK(bjp_op(B_BNE, 42, 43) == 1, "bne(42,43) should resolve taken");
    CHECK(bjp_op(B_BLT, (uint32_t)-5, 3) == 1, "blt(-5,3) should resolve taken");
    CHECK(bjp_op(B_BGE, (uint32_t)-5, 3) == 0, "bge(-5,3) should resolve not-taken");
    CHECK(bjp_op(B_BGE, 3, 3) == 1, "bge(3,3) should resolve taken");
    CHECK(bjp_op(B_BLTU, 1, 0xFFFFFFF0u) == 1, "bltu(1,big) should resolve taken");
    CHECK(bjp_op(B_BGEU, 1, 0xFFFFFFF0u) == 0, "bgeu(1,big) should resolve not-taken");
    CHECK(bjp_op(B_JUMP, 0, 0) == 1, "jump always resolves taken");
    CHECK(dut->cmt_o_bjp == 1, "cmt_o_bjp should be 1 for a bjp op, got %d", dut->cmt_o_bjp);
    // Link address writeback: JAL at pc writes pc+4.
    dut->i_pc = 0x2000;
    dut->i_rdwen = 1;
    dut->eval();
    CHECK(dut->wbck_o_wdat == 0x2004, "jump link should be pc+4 = 0x2004, got 0x%08x", dut->wbck_o_wdat);
    // prdt bit passes through to commit.
    dut->i_info = OP_BJP | B_BEQ | OP_PRDT;
    dut->i_rs1 = 1; dut->i_rs2 = 2;   // resolves not-taken -> mispredict material
    dut->eval();
    CHECK(dut->cmt_o_bjp_prdt == 1, "cmt_o_bjp_prdt should pass through 1, got %d", dut->cmt_o_bjp_prdt);
    CHECK(dut->cmt_o_bjp_rslv == 0, "mispredicted beq resolves 0, got %d", dut->cmt_o_bjp_rslv);

    // ── Test 4: System-op commit flags + IFU faults ──────────────────
    printf("Test 4: Commit flags\n");
    reset();
    dut->i_valid = 1;
    dut->i_info = OP_MRET | OP_RV32;
    dut->i_pc = 0x400; dut->i_pc_vld = 1;
    dut->i_instr = 0x30200073;   // mret encoding
    dut->eval();
    CHECK(dut->cmt_o_mret == 1, "cmt_o_mret should be 1, got %d", dut->cmt_o_mret);
    CHECK(dut->cmt_o_rv32 == 1, "cmt_o_rv32 should be 1, got %d", dut->cmt_o_rv32);
    CHECK(dut->cmt_o_pc == 0x400, "cmt_o_pc should pass through, got 0x%08x", dut->cmt_o_pc);
    CHECK(dut->cmt_o_pc_vld == 1, "cmt_o_pc_vld should pass through, got %d", dut->cmt_o_pc_vld);
    CHECK(dut->cmt_o_instr == 0x30200073u, "cmt_o_instr should pass through, got 0x%08x", dut->cmt_o_instr);
    dut->i_info = OP_ECALL;
    dut->eval();
    CHECK(dut->cmt_o_ecall == 1, "cmt_o_ecall should be 1, got %d", dut->cmt_o_ecall);
    CHECK(dut->cmt_o_mret == 0, "cmt_o_mret should drop, got %d", dut->cmt_o_mret);
    dut->i_info = OP_EBREAK;
    dut->eval();
    CHECK(dut->cmt_o_ebreak == 1, "cmt_o_ebreak should be 1, got %d", dut->cmt_o_ebreak);
    dut->i_info = OP_WFI;
    dut->eval();
    CHECK(dut->cmt_o_wfi == 1, "cmt_o_wfi should be 1, got %d", dut->cmt_o_wfi);
    dut->i_info = OP_FENCEI;
    dut->eval();
    CHECK(dut->cmt_o_fencei == 1, "cmt_o_fencei should be 1, got %d", dut->cmt_o_fencei);
    // IFU fault flags pass through.
    dut->i_ilegl = 1; dut->i_buserr = 1; dut->i_misalgn = 1;
    dut->eval();
    CHECK(dut->cmt_o_ifu_ilegl == 1, "cmt_o_ifu_ilegl should be 1, got %d", dut->cmt_o_ifu_ilegl);
    CHECK(dut->cmt_o_ifu_buserr == 1, "cmt_o_ifu_buserr should be 1, got %d", dut->cmt_o_ifu_buserr);
    CHECK(dut->cmt_o_ifu_misalgn == 1, "cmt_o_ifu_misalgn should be 1, got %d", dut->cmt_o_ifu_misalgn);

    // ── Test 5: CSR path ─────────────────────────────────────────────
    printf("Test 5: CSR path\n");
    reset();
    dut->i_valid = 1;
    dut->i_rdwen = 1;
    dut->i_info = OP_CSR;
    dut->i_imm = 0x305;             // mtvec CSR index in imm[11:0]
    dut->i_rs1 = 0x1234;            // write data
    dut->read_csr_dat = 0xCAFE0000u;
    dut->eval();
    CHECK(dut->csr_ena == 1, "csr_ena should be 1, got %d", dut->csr_ena);
    CHECK(dut->csr_wr_en == 1, "csr_wr_en should be 1, got %d", dut->csr_wr_en);
    CHECK(dut->csr_rd_en == 1, "csr_rd_en should be 1, got %d", dut->csr_rd_en);
    CHECK(dut->csr_idx == 0x305, "csr_idx should be imm[11:0]=0x305, got 0x%x", dut->csr_idx);
    CHECK(dut->wbck_csr_dat == 0x1234, "wbck_csr_dat should be rs1, got 0x%08x", dut->wbck_csr_dat);
    CHECK(dut->wbck_o_wdat == 0xCAFE0000u, "CSR read data should write back, got 0x%08x", dut->wbck_o_wdat);

    // ── Test 6: AGU path ─────────────────────────────────────────────
    printf("Test 6: AGU path\n");
    reset();
    dut->i_valid = 1;
    dut->i_info = OP_AGU | G_LOAD | (2u << 24) | G_USIGN;  // LWU-style: size=2, unsigned
    dut->i_rs1 = 0x1000;
    dut->i_imm = 0x20;
    dut->i_rs2 = 0x55AA55AAu;
    dut->i_itag = 1;
    dut->eval();
    CHECK(dut->agu_icb_cmd_valid == 1, "agu cmd_valid should be 1, got %d", dut->agu_icb_cmd_valid);
    CHECK(dut->agu_icb_cmd_addr == 0x1020, "agu addr should be rs1+imm=0x1020, got 0x%08x", dut->agu_icb_cmd_addr);
    CHECK(dut->agu_icb_cmd_read == 1, "agu cmd_read should be 1 for a load, got %d", dut->agu_icb_cmd_read);
    CHECK(dut->agu_icb_cmd_size == 2, "agu cmd_size should be info[25:24]=2, got %d", dut->agu_icb_cmd_size);
    CHECK(dut->agu_icb_cmd_usign == 1, "agu cmd_usign should be info[26]=1, got %d", dut->agu_icb_cmd_usign);
    CHECK(dut->agu_icb_cmd_itag == 1, "agu cmd_itag should pass i_itag, got %d", dut->agu_icb_cmd_itag);
    CHECK(dut->agu_icb_cmd_lock == 0, "plain load should not lock, got %d", dut->agu_icb_cmd_lock);
    CHECK(dut->i_longpipe == 1, "AGU op is long-pipe, got %d", dut->i_longpipe);
    CHECK(dut->cmt_o_ld == 1, "cmt_o_ld should be 1, got %d", dut->cmt_o_ld);
    CHECK(dut->cmt_o_stamo == 0, "cmt_o_stamo should be 0 for a load, got %d", dut->cmt_o_stamo);
    CHECK(dut->cmt_o_badaddr == 0x1020, "cmt_o_badaddr should carry the agu addr, got 0x%08x", dut->cmt_o_badaddr);
    // Load data writes back from the ICB response.
    dut->i_rdwen = 1;
    dut->agu_icb_rsp_rdata = 0x99887766u;
    dut->eval();
    CHECK(dut->wbck_o_wdat == 0x99887766u, "agu wbck data should be rsp_rdata, got 0x%08x", dut->wbck_o_wdat);
    // Store: write data + stamo commit flag.
    dut->i_info = OP_AGU | G_STORE;
    dut->eval();
    CHECK(dut->agu_icb_cmd_read == 0, "store cmd_read should be 0, got %d", dut->agu_icb_cmd_read);
    CHECK(dut->agu_icb_cmd_wdata == 0x55AA55AAu, "store wdata should be rs2, got 0x%08x", dut->agu_icb_cmd_wdata);
    CHECK(dut->cmt_o_stamo == 1, "cmt_o_stamo should be 1 for a store, got %d", dut->cmt_o_stamo);
    // AMO: lock/excl/back2agu asserted; amo_wait when OITF is not empty.
    dut->i_info = OP_AGU | G_AMO;
    dut->oitf_empty = 0;
    dut->eval();
    CHECK(dut->agu_icb_cmd_lock == 1, "AMO should lock, got %d", dut->agu_icb_cmd_lock);
    CHECK(dut->agu_icb_cmd_excl == 1, "AMO should be exclusive, got %d", dut->agu_icb_cmd_excl);
    CHECK(dut->agu_icb_cmd_back2agu == 1, "AMO should set back2agu, got %d", dut->agu_icb_cmd_back2agu);
    CHECK(dut->amo_wait == 1, "amo_wait should assert while OITF is busy, got %d", dut->amo_wait);
    dut->oitf_empty = 1;
    dut->eval();
    CHECK(dut->amo_wait == 0, "amo_wait should clear when OITF drains, got %d", dut->amo_wait);
    // Dispatch-ready gating: AGU op stalls when the ICB command is not ready.
    dut->agu_icb_cmd_ready = 0;
    dut->eval();
    CHECK(dut->i_ready == 0, "i_ready should drop when agu cmd not ready, got %d", dut->i_ready);
    dut->agu_icb_cmd_ready = 1;
    dut->eval();
    CHECK(dut->i_ready == 1, "i_ready should recover, got %d", dut->i_ready);

    // ── Test 7: Flush suppression ────────────────────────────────────
    printf("Test 7: Flush suppression\n");
    reset();
    dut->i_valid = 1;
    dut->i_rdwen = 1;
    dut->i_info = OP_ALU | A_ADD;
    dut->i_rs1 = 1; dut->i_rs2 = 2;
    dut->flush_req = 1;
    dut->eval();
    CHECK(dut->wbck_o_valid == 0, "flush must suppress wbck_o_valid, got %d", dut->wbck_o_valid);
    CHECK(dut->cmt_o_valid == 0, "flush must suppress cmt_o_valid, got %d", dut->cmt_o_valid);
    dut->i_info = OP_CSR;
    dut->eval();
    CHECK(dut->csr_ena == 0, "flush must suppress csr_ena, got %d", dut->csr_ena);
    dut->i_info = OP_AGU | G_LOAD;
    dut->eval();
    CHECK(dut->agu_icb_cmd_valid == 0, "flush must suppress agu cmd_valid, got %d", dut->agu_icb_cmd_valid);
    dut->i_info = OP_NICE;
    dut->eval();
    CHECK(dut->nice_req_valid == 0, "flush must suppress nice_req_valid, got %d", dut->nice_req_valid);

    // ── Test 8: NICE long-pipe tracking ──────────────────────────────
    printf("Test 8: NICE long-pipe register\n");
    reset();
    dut->i_valid = 1;
    dut->i_info = OP_NICE;
    dut->i_instr = 0x0200008Bu;   // custom-0 NICE instruction word
    dut->i_rs1 = 0xAA; dut->i_rs2 = 0xBB;
    dut->eval();
    CHECK(dut->nice_req_valid == 1, "nice_req_valid should be 1, got %d", dut->nice_req_valid);
    CHECK(dut->nice_req_instr == 0x0200008Bu, "nice_req_instr should pass through, got 0x%08x", dut->nice_req_instr);
    CHECK(dut->nice_req_rs1 == 0xAA, "nice_req_rs1 should pass through, got 0x%08x", dut->nice_req_rs1);
    CHECK(dut->nice_req_rs2 == 0xBB, "nice_req_rs2 should pass through, got 0x%08x", dut->nice_req_rs2);
    CHECK(dut->i_longpipe == 1, "NICE op is long-pipe, got %d", dut->i_longpipe);
    CHECK(dut->nice_longp_wbck_valid == 0, "no longp wbck before the response, got %d", dut->nice_longp_wbck_valid);
    tick();                        // nice_longp_r loads (req accepted)
    dut->i_valid = 0;
    dut->nice_rsp_multicyc_valid = 1;
    dut->eval();
    CHECK(dut->nice_longp_wbck_valid == 1, "longp wbck should assert on the multicyc response, got %d",
          dut->nice_longp_wbck_valid);
    tick();                        // response clears nice_longp_r
    dut->eval();
    CHECK(dut->nice_longp_wbck_valid == 0, "longp wbck should clear after the response, got %d",
          dut->nice_longp_wbck_valid);
    dut->nice_rsp_multicyc_valid = 0;
    // flush_pulse clears a pending long-pipe track.
    dut->i_valid = 1;
    dut->i_info = OP_NICE;
    dut->eval();
    tick();                        // nice_longp_r loads again
    dut->i_valid = 0;
    dut->flush_pulse = 1;
    tick();                        // pulse clears it
    dut->flush_pulse = 0;
    dut->nice_rsp_multicyc_valid = 1;
    dut->eval();
    CHECK(dut->nice_longp_wbck_valid == 0, "flush_pulse must clear the longp track, got %d",
          dut->nice_longp_wbck_valid);

    printf("\n=== e203_exu_alu: %d failure(s) ===\n", fail_count);
    return fail_count ? 1 : 0;
}
