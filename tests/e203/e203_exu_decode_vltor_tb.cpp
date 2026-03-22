// Verilator cross-check testbench for ExuDecode
#include "VExuDecode.h"
#include "verilated.h"
#include <cstdio>
#include <cstdint>
#include <cstdlib>

static int fail_count = 0;

#define CHECK(cond, fmt, ...) do { \
    if (!(cond)) { \
        printf("  FAIL: " fmt "\n", ##__VA_ARGS__); \
        fail_count++; \
    } \
} while(0)

static uint32_t r_type(uint32_t f7, uint32_t rs2, uint32_t rs1, uint32_t f3, uint32_t rd, uint32_t op) {
    return (f7<<25)|(rs2<<20)|(rs1<<15)|(f3<<12)|(rd<<7)|op;
}
static uint32_t i_type(uint32_t imm, uint32_t rs1, uint32_t f3, uint32_t rd, uint32_t op) {
    return ((imm&0xFFF)<<20)|(rs1<<15)|(f3<<12)|(rd<<7)|op;
}
static uint32_t s_type(uint32_t imm, uint32_t rs2, uint32_t rs1, uint32_t f3, uint32_t op) {
    return ((imm>>5&0x7F)<<25)|(rs2<<20)|(rs1<<15)|(f3<<12)|((imm&0x1F)<<7)|op;
}
static uint32_t b_type(int32_t off, uint32_t rs2, uint32_t rs1, uint32_t f3, uint32_t op) {
    uint32_t i=(uint32_t)off;
    return ((i>>12&1)<<31)|((i>>5&0x3F)<<25)|(rs2<<20)|(rs1<<15)|(f3<<12)|((i>>1&0xF)<<8)|((i>>11&1)<<7)|op;
}
static uint32_t u_type(uint32_t imm20, uint32_t rd, uint32_t op) {
    return (imm20<<12)|(rd<<7)|op;
}
static uint32_t j_type(int32_t off, uint32_t rd, uint32_t op) {
    uint32_t i=(uint32_t)off;
    return ((i>>20&1)<<31)|((i>>1&0x3FF)<<21)|((i>>11&1)<<20)|((i>>12&0xFF)<<12)|(rd<<7)|op;
}

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VExuDecode* dut = new VExuDecode();

    auto apply = [&](uint32_t instr) {
        dut->instr = instr;
        dut->eval();
    };

    printf("=== ExuDecode Verilator cross-check ===\n");

    // Test 1: ADD
    printf("Test 1: ADD x3, x1, x2\n");
    apply(r_type(0x00,2,1,0x0,3,0x33));
    CHECK(dut->o_alu==1,"o_alu=%d",dut->o_alu);
    CHECK(dut->o_alu_add==1,"o_alu_add=%d",dut->o_alu_add);
    CHECK(dut->o_rs1_idx==1,"rs1=%d",dut->o_rs1_idx);
    CHECK(dut->o_rs2_idx==2,"rs2=%d",dut->o_rs2_idx);
    CHECK(dut->o_rd_idx==3,"rd=%d",dut->o_rd_idx);

    // Test 2: SUB
    printf("Test 2: SUB x5, x3, x4\n");
    apply(r_type(0x20,4,3,0x0,5,0x33));
    CHECK(dut->o_alu_sub==1,"o_alu_sub=%d",dut->o_alu_sub);

    // Test 3-10: R-type ops
    printf("Test 3: XOR\n");
    apply(r_type(0x00,2,1,0x4,6,0x33));
    CHECK(dut->o_alu_xor==1,"xor=%d",dut->o_alu_xor);

    printf("Test 4: SLL\n");
    apply(r_type(0x00,2,1,0x1,7,0x33));
    CHECK(dut->o_alu_sll==1,"sll=%d",dut->o_alu_sll);

    printf("Test 5: SRL\n");
    apply(r_type(0x00,2,1,0x5,8,0x33));
    CHECK(dut->o_alu_srl==1,"srl=%d",dut->o_alu_srl);

    printf("Test 6: SRA\n");
    apply(r_type(0x20,2,1,0x5,9,0x33));
    CHECK(dut->o_alu_sra==1,"sra=%d",dut->o_alu_sra);

    printf("Test 7: OR\n");
    apply(r_type(0x00,2,1,0x6,10,0x33));
    CHECK(dut->o_alu_or==1,"or=%d",dut->o_alu_or);

    printf("Test 8: AND\n");
    apply(r_type(0x00,2,1,0x7,11,0x33));
    CHECK(dut->o_alu_and==1,"and=%d",dut->o_alu_and);

    printf("Test 9: SLT\n");
    apply(r_type(0x00,2,1,0x2,12,0x33));
    CHECK(dut->o_alu_slt==1,"slt=%d",dut->o_alu_slt);

    printf("Test 10: SLTU\n");
    apply(r_type(0x00,2,1,0x3,13,0x33));
    CHECK(dut->o_alu_sltu==1,"sltu=%d",dut->o_alu_sltu);

    // Test 11-12: I-type
    printf("Test 11: ADDI x1, x2, 42\n");
    apply(i_type(42,2,0x0,1,0x13));
    CHECK(dut->o_alu_add==1,"add=%d",dut->o_alu_add);
    CHECK(dut->o_imm==42,"imm=0x%08X",dut->o_imm);

    printf("Test 12: ADDI x1, x2, -5\n");
    apply(i_type((-5)&0xFFF,2,0x0,1,0x13));
    CHECK(dut->o_imm==0xFFFFFFFB,"imm=0x%08X exp 0xFFFFFFFB",dut->o_imm);

    // Test 13: LUI
    printf("Test 13: LUI x5, 0xDEADB\n");
    apply(u_type(0xDEADB,5,0x37));
    CHECK(dut->o_alu_lui==1,"lui=%d",dut->o_alu_lui);
    CHECK(dut->o_imm==0xDEADB000,"imm=0x%08X exp 0xDEADB000",dut->o_imm);

    // Test 14: AUIPC
    printf("Test 14: AUIPC x6, 0x12345\n");
    apply(u_type(0x12345,6,0x17));
    CHECK(dut->o_alu_add==1,"add=%d (AUIPC)",dut->o_alu_add);
    CHECK(dut->o_imm==0x12345000,"imm=0x%08X",dut->o_imm);

    // Test 15-16: Branch
    printf("Test 15: BEQ +8\n");
    apply(b_type(8,2,1,0x0,0x63));
    CHECK(dut->o_beq==1,"beq=%d",dut->o_beq);
    CHECK(dut->o_imm==8,"imm=0x%08X",dut->o_imm);

    printf("Test 16: BNE -16\n");
    apply(b_type(-16,4,3,0x1,0x63));
    CHECK(dut->o_bne==1,"bne=%d",dut->o_bne);
    CHECK(dut->o_imm==0xFFFFFFF0,"imm=0x%08X exp 0xFFFFFFF0",dut->o_imm);

    // Test 17: JAL +1024
    printf("Test 17: JAL +1024\n");
    apply(j_type(1024,1,0x6F));
    CHECK(dut->o_jump==1,"jump=%d",dut->o_jump);
    CHECK(dut->o_imm==1024,"imm=0x%08X",dut->o_imm);

    // Test 18: JAL -256
    printf("Test 18: JAL -256\n");
    apply(j_type(-256,1,0x6F));
    CHECK(dut->o_imm==0xFFFFFF00,"imm=0x%08X exp 0xFFFFFF00",dut->o_imm);

    // Test 19: JALR
    printf("Test 19: JALR x1, x5, 100\n");
    apply(i_type(100,5,0x0,1,0x67));
    CHECK(dut->o_jump==1,"jump=%d",dut->o_jump);
    CHECK(dut->o_imm==100,"imm=0x%08X",dut->o_imm);

    // Test 20: LW
    printf("Test 20: LW x3, -4(x1)\n");
    apply(i_type((-4)&0xFFF,1,0x2,3,0x03));
    CHECK(dut->o_load==1,"load=%d",dut->o_load);
    CHECK(dut->o_imm==0xFFFFFFFC,"imm=0x%08X exp 0xFFFFFFFC",dut->o_imm);

    // Test 21: SW
    printf("Test 21: SW x2, -8(x1)\n");
    apply(s_type((-8)&0xFFF,2,1,0x2,0x23));
    CHECK(dut->o_store==1,"store=%d",dut->o_store);
    CHECK(dut->o_imm==0xFFFFFFF8,"imm=0x%08X exp 0xFFFFFFF8",dut->o_imm);

    // Test 22: SRAI
    printf("Test 22: SRAI x3, x1, 5\n");
    apply(i_type((0x20<<5)|5,1,0x5,3,0x13));
    CHECK(dut->o_alu_sra==1,"sra=%d",dut->o_alu_sra);

    if (fail_count==0) printf("\nAll 22 Verilator tests PASSED\n");
    else printf("\n%d test(s) FAILED\n",fail_count);

    delete dut;
    return fail_count?1:0;
}
