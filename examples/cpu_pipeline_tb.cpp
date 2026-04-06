#include "VCpuPipe.h"
#include "verilated.h"
#include <cstdio>
#include <cstdlib>

// CpuPipe pipeline stages: Fetch → Decode → Execute → Writeback
//
// Timing: when instruction is fed at cycle N:
//   Cycle N:   Fetch captures imem_data, pc=branch_target
//   Cycle N+1: Decode reads Fetch.instr (opcode, rd) AND rs1_data/rs2_data ports
//              → rs1_data/rs2_data must be held valid through this cycle
//   Cycle N+2: Execute computes alu_result = rs1_val + rs2_val, we = (opcode!=0)
//   Cycle N+3: Writeback outputs wb_data, wb_rd, wb_we
//
// To avoid Decode stall (load-use hazard), use rd=0 (bits [11:7] = 0).
// Stall condition: execute_we && (execute_rd == fetch_instr[11:7]) && (execute_rd != 0)

int main(int argc, char** argv) {
    Verilated::commandArgs(argc, argv);
    VCpuPipe* dut = new VCpuPipe;

    int errors = 0;
    int cycle = 0;

    dut->clk = 0;
    dut->rst = 1;
    dut->imem_data = 0;
    dut->imem_valid = 1;
    dut->rs1_data = 0;
    dut->rs2_data = 0;
    dut->branch_taken = 0;
    dut->branch_target = 0;

    auto tick = [&]() {
        dut->clk = 0; dut->eval();
        dut->clk = 1; dut->eval();
        cycle++;
    };

    auto check = [&](bool cond, const char* msg) {
        if (!cond) {
            printf("  FAIL [C%02d]: %s (wb_data=%d wb_rd=%d wb_we=%d pc=%08x)\n",
                   cycle, msg, dut->wb_data, dut->wb_rd, dut->wb_we, dut->pc_out);
            errors++;
        }
    };

    // ────────────────────────────────────────────
    // Test 1: Reset
    // ────────────────────────────────────────────
    printf("=== Test 1: Reset ===\n");
    for (int i = 0; i < 3; i++) tick();
    check(dut->wb_data == 0, "wb_data=0");
    check(dut->wb_we == 0,   "wb_we=0");
    check(dut->pc_out == 0,  "pc_out=0");
    printf("  PASS\n");

    // ────────────────────────────────────────────
    // Test 2: Single instruction through pipeline
    // Instruction: 0x00000060 → opcode=0x60(nonzero), rd=0(lower 5 bits)
    // rd=0 ensures no decode stall (condition requires execute_rd!=0)
    // ────────────────────────────────────────────
    printf("\n=== Test 2: Single instruction flow ===\n");
    dut->rst = 0;

    // Cycle N: Feed instruction and register data
    dut->imem_data = 0x00000060;
    dut->rs1_data = 100;
    dut->rs2_data = 200;
    dut->branch_target = 0x1000;
    tick();  // N: Fetch captures instr=0x60, pc=0x1000

    // Cycle N+1: Hold rs1/rs2 for Decode. Feed NOP on imem.
    // Decode reads: opcode=instr[6:0]=0x60, rd=instr[11:7]=0, rs1_val=100, rs2_val=200
    dut->imem_data = 0;
    tick();  // N+1: Decode captures

    // Cycle N+2: Can clear rs1/rs2 now. Execute computes 100+200=300, we=(0x60!=0)=1
    dut->rs1_data = 0;
    dut->rs2_data = 0;
    tick();  // N+2: Execute

    // Cycle N+3: Writeback outputs result
    tick();  // N+3: Writeback

    check(dut->wb_data == 300, "wb_data=100+200=300");
    check(dut->wb_we == 1,    "wb_we=1 (opcode nonzero)");
    check(dut->wb_rd == 0,    "wb_rd=0");
    check(dut->pc_out == 0x1000, "pc_out=0x1000");
    printf("  PASS\n");

    // Drain pipeline (4 NOP cycles)
    dut->imem_data = 0;
    for (int i = 0; i < 4; i++) tick();

    // ────────────────────────────────────────────
    // Test 3: Two back-to-back instructions
    // A: 0x60, rs1=10, rs2=20 → wb_data=30
    // B: 0x40, rs1=50, rs2=60 → wb_data=110
    // Both have rd=0, so no hazard stall
    // ────────────────────────────────────────────
    printf("\n=== Test 3: Back-to-back instructions ===\n");
    // rs1_data/rs2_data pair with the instruction currently in Decode
    // (one cycle after Fetch). So present A's reg data at cycle N+1.

    // Cycle N: Feed A on imem. rs1/rs2 don't care yet (Decode has NOP from drain)
    dut->imem_data = 0x00000060;
    dut->rs1_data = 0;
    dut->rs2_data = 0;
    dut->branch_target = 0x2000;
    tick();  // Fetch A

    // Cycle N+1: Feed B on imem. Present A's reg data for Decode A.
    dut->imem_data = 0x00000040;
    dut->rs1_data = 10;
    dut->rs2_data = 20;
    dut->branch_target = 0x2004;
    tick();  // Fetch B, Decode A (reads opcode from A, rs1=10, rs2=20)

    // Cycle N+2: Feed NOP. Present B's reg data for Decode B. Execute A.
    dut->imem_data = 0;
    dut->rs1_data = 50;
    dut->rs2_data = 60;
    tick();  // Decode B (reads opcode from B, rs1=50, rs2=60), Execute A

    // Cycle N+3: Execute B, Writeback A
    dut->rs1_data = 0;
    dut->rs2_data = 0;
    tick();

    check(dut->wb_data == 30, "A: wb_data=10+20=30");
    check(dut->wb_we == 1,    "A: wb_we=1");

    // Cycle N+4: Writeback B
    tick();

    check(dut->wb_data == 110, "B: wb_data=50+60=110");
    check(dut->wb_we == 1,     "B: wb_we=1");
    printf("  PASS\n");

    // Drain
    dut->imem_data = 0;
    for (int i = 0; i < 4; i++) tick();

    // ────────────────────────────────────────────
    // Test 4: Fetch stall (imem_valid=0)
    // Feed instruction, then stall Fetch. Instruction should still
    // propagate through since Fetch captured it before stall.
    // ────────────────────────────────────────────
    printf("\n=== Test 4: Fetch stall ===\n");

    // Feed instruction
    dut->imem_data = 0x00000060;
    dut->rs1_data = 50;
    dut->rs2_data = 25;
    dut->branch_target = 0x3000;
    dut->imem_valid = 1;
    tick();  // Fetch captures

    // Stall Fetch. Hold rs1/rs2 for Decode.
    // fetch_stall = !imem_valid || decode_stall = 1 || 0 = 1
    // Fetch doesn't update (holds instr=0x60, valid_r=1)
    // Decode: decode_stall=0, so Decode updates.
    //   decode_valid_r <= fetch_stall ? 0 : fetch_valid_r
    //   fetch_stall=1, so decode_valid_r=0 (bubble!)
    //   But Decode still latches the data (opcode, rd, rs1_val, rs2_val)
    dut->imem_valid = 0;
    tick();  // Decode: latches data but valid_r=0 (bubble)

    // Data still flows through Execute and Writeback, but valid_r=0
    // propagates as a bubble. wb_we = writeback_valid AND writeback_valid_r.
    // Since writeback_valid_r=0 (bubble reached Writeback), wb_we=0.
    // This correctly suppresses output for stall-inserted bubbles.
    tick();  // Execute
    tick();  // Writeback

    check(dut->wb_data == 75, "wb_data=50+25=75 (data still flows through)");
    check(dut->wb_we == 0,   "wb_we=0 (valid_r gates output: bubble suppressed)");

    // Unstall
    dut->imem_valid = 1;
    printf("  PASS\n");

    // Drain
    dut->imem_data = 0;
    dut->rs1_data = 0;
    dut->rs2_data = 0;
    for (int i = 0; i < 4; i++) tick();

    // ────────────────────────────────────────────
    // Test 5: Flush on branch_taken
    // Feed instruction A through to Execute, then flush Fetch+Decode.
    // A should still complete. Then feed B at branch target.
    // ────────────────────────────────────────────
    printf("\n=== Test 5: Flush ===\n");

    // Use a clean state
    dut->rst = 1;
    tick(); tick();
    dut->rst = 0;

    // Feed instruction A
    dut->imem_data = 0x00000060;
    dut->rs1_data = 10;
    dut->rs2_data = 20;
    dut->branch_target = 0x4000;
    dut->branch_taken = 0;
    tick();  // Fetch A

    // Hold rs1/rs2 for Decode A
    dut->imem_data = 0;
    tick();  // Decode A

    dut->rs1_data = 0;
    dut->rs2_data = 0;
    tick();  // Execute A

    // Now A is in Writeback cycle. Also trigger flush.
    dut->branch_taken = 1;
    tick();  // Writeback A, flush Fetch+Decode

    check(dut->wb_data == 30, "A: wb_data=10+20=30 (survives flush)");
    check(dut->wb_we == 1,    "A: wb_we=1");

    // Feed instruction B at branch target
    dut->branch_taken = 0;
    dut->imem_data = 0x00000060;
    dut->rs1_data = 500;
    dut->rs2_data = 600;
    dut->branch_target = 0x8000;
    tick();  // Fetch B (fetch_valid_r was cleared by flush, now set to 1)

    // Hold for Decode B
    dut->imem_data = 0;
    tick();  // Decode B

    dut->rs1_data = 0;
    dut->rs2_data = 0;
    tick();  // Execute B

    tick();  // Writeback B

    check(dut->wb_data == 1100, "B: wb_data=500+600=1100");
    check(dut->wb_we == 1,      "B: wb_we=1");
    printf("  PASS\n");

    // ────────────────────────────────────────────
    // Test 6: Zero opcode → wb_we=0
    // ────────────────────────────────────────────
    printf("\n=== Test 6: Zero opcode ===\n");

    // Drain
    for (int i = 0; i < 4; i++) tick();

    dut->imem_data = 0x00000000;  // opcode=0
    dut->rs1_data = 42;
    dut->rs2_data = 58;
    dut->branch_target = 0x9000;
    tick();  // Fetch

    dut->imem_data = 0;
    // hold rs1/rs2 for Decode
    tick();  // Decode

    dut->rs1_data = 0;
    dut->rs2_data = 0;
    tick();  // Execute: we = (0!=0) = 0

    tick();  // Writeback

    check(dut->wb_we == 0,     "wb_we=0 (zero opcode)");
    check(dut->wb_data == 100, "wb_data=42+58=100 (ALU still computes)");
    printf("  PASS\n");

    // ────────────────────────────────────────────
    // Test 7: Mid-operation reset
    // ────────────────────────────────────────────
    printf("\n=== Test 7: Mid-op reset ===\n");
    dut->imem_data = 0x00000060;
    dut->rs1_data = 999;
    dut->rs2_data = 1;
    dut->branch_target = 0xA000;
    tick();
    tick();

    dut->rst = 1;
    tick(); tick();

    check(dut->wb_data == 0, "wb_data=0 after reset");
    check(dut->wb_we == 0,  "wb_we=0 after reset");
    check(dut->pc_out == 0, "pc_out=0 after reset");
    printf("  PASS\n");

    // ────────────────────────────────────────────
    dut->final();
    delete dut;

    if (errors == 0) {
        printf("\nALL TESTS PASSED\n");
        return 0;
    } else {
        printf("\n%d TEST(S) FAILED\n", errors);
        return 1;
    }
}
