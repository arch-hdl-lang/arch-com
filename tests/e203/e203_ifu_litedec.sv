// E203 Lightweight Pre-Decoder
// Determines if an instruction is 16-bit (RV32C compressed) or 32-bit.
// In E203, this is used by the IFU to know how much to advance PC.
// Also extracts basic info for branch prediction: is it a branch/JAL/JALR,
// and the register indices for BPU RAW hazard checking.
module IfuLiteDec (
  input logic [32-1:0] instr,
  output logic is_32bit,
  output logic is_bjp,
  output logic is_jal,
  output logic is_jalr,
  output logic is_branch,
  output logic is_lui,
  output logic is_auipc,
  output logic [5-1:0] rs1_idx,
  output logic [5-1:0] rd_idx,
  output logic rs1_en,
  output logic [32-1:0] bjp_imm
);

  // Length detection
  // Quick decode for BPU
  // branch, JAL, or JALR
  // Register indices (for BPU hazard check)
  // Branch/JAL immediate (sign-extended, for BPU target calc)
  // ── 16-bit vs 32-bit ───────────────────────────────────────────
  // Per RV spec: if bits [1:0] != 2'b11, it's a 16-bit instruction
  logic [2-1:0] opcode_1_0;
  assign opcode_1_0 = instr[1:0];
  logic is_32;
  assign is_32 = (opcode_1_0 == 3);
  // ── Opcode fields (32-bit) ─────────────────────────────────────
  logic [7-1:0] opcode;
  assign opcode = instr[6:0];
  logic [3-1:0] funct3;
  assign funct3 = instr[14:12];
  // ── Instruction type detection ─────────────────────────────────
  logic dec_jal;
  assign dec_jal = (opcode == 'h6F);
  // JAL
  logic dec_jalr;
  assign dec_jalr = (opcode == 'h67);
  // JALR
  logic dec_branch;
  assign dec_branch = (opcode == 'h63);
  // Bxx
  logic dec_lui;
  assign dec_lui = (opcode == 'h37);
  // LUI
  logic dec_auipc;
  assign dec_auipc = (opcode == 'h17);
  // AUIPC
  // ── Register indices ───────────────────────────────────────────
  logic [5-1:0] rs1_field;
  assign rs1_field = instr[19:15];
  logic [5-1:0] rd_field;
  assign rd_field = instr[11:7];
  // rs1 used by JALR and Bxx
  logic rs1_used;
  assign rs1_used = (dec_jalr | dec_branch);
  // ── Immediate extraction ───────────────────────────────────────
  // JAL: imm[20|10:1|11|19:12] -> bits [31|30:21|20|19:12]
  logic [32-1:0] jal_imm;
  assign jal_imm = {{12{(instr[31:31] != 0)}}, instr[19:12], instr[20:20], instr[30:21], 1'b0};
  // Branch: imm[12|10:5|4:1|11] -> bits [31|30:25|11:8|7]
  logic [32-1:0] branch_imm;
  assign branch_imm = {{20{(instr[31:31] != 0)}}, instr[7:7], instr[30:25], instr[11:8], 1'b0};
  always_comb begin
    is_32bit = is_32;
    is_jal = (dec_jal & is_32);
    is_jalr = (dec_jalr & is_32);
    is_branch = (dec_branch & is_32);
    is_bjp = (((dec_jal | dec_jalr) | dec_branch) & is_32);
    is_lui = (dec_lui & is_32);
    is_auipc = (dec_auipc & is_32);
    rs1_idx = rs1_field;
    rd_idx = rd_field;
    rs1_en = (rs1_used & is_32);
    if (dec_jal) begin
      bjp_imm = jal_imm;
    end else begin
      bjp_imm = branch_imm;
    end
  end

endmodule

// Immediate: JAL or Branch
