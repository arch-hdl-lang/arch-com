module static_branch_predict (
  input logic [31:0] fetch_rdata_i,
  input logic [31:0] fetch_pc_i,
  input logic fetch_valid_i,
  input logic [31:0] register_addr_i,
  output logic predict_branch_taken_o,
  output logic [31:0] predict_branch_pc_o
);

  // Opcodes
  logic [6:0] OPCODE_BRANCH;
  assign OPCODE_BRANCH = 7'd99;
  logic [6:0] OPCODE_JAL;
  assign OPCODE_JAL = 7'd111;
  logic [6:0] OPCODE_JALR;
  assign OPCODE_JALR = 7'd103;
  // Alias
  logic [31:0] instr;
  assign instr = fetch_rdata_i;
  logic [6:0] opcode;
  assign opcode = instr[6:0];
  // Instruction type detection
  logic instr_j;
  assign instr_j = opcode == OPCODE_JAL;
  logic instr_b;
  assign instr_b = opcode == OPCODE_BRANCH;
  logic instr_jalr;
  assign instr_jalr = opcode == OPCODE_JALR;
  // Immediate extraction - JAL (J-type)
  logic [31:0] imm_j_type;
  assign imm_j_type = {{12{instr[31]}}, instr[19:12], instr[20], instr[30:21], 1'd0};
  // Immediate extraction - Branch (B-type)
  logic [31:0] imm_b_type;
  assign imm_b_type = {{20{instr[31]}}, instr[7], instr[30:25], instr[11:8], 1'd0};
  // Immediate extraction - JALR (I-type)
  logic [31:0] imm_i_type;
  assign imm_i_type = {{20{instr[31]}}, instr[31:20]};
  // Branch taken if offset is negative (sign bit = 1)
  logic instr_b_taken;
  assign instr_b_taken = instr[31];
  // Select branch immediate
  logic [31:0] branch_imm;
  always_comb begin
    if (instr_j) begin
      branch_imm = imm_j_type;
    end else if (instr_jalr) begin
      branch_imm = imm_i_type;
    end else if (instr_b) begin
      branch_imm = imm_b_type;
    end else begin
      branch_imm = 32'd0;
    end
  end
  // Output logic
  always_comb begin
    if (fetch_valid_i) begin
      if (instr_j | instr_jalr) begin
        predict_branch_taken_o = 1'b1;
      end else if (instr_b & instr_b_taken) begin
        predict_branch_taken_o = 1'b1;
      end else begin
        predict_branch_taken_o = 1'b0;
      end
      predict_branch_pc_o = 32'(fetch_pc_i + branch_imm);
    end else begin
      predict_branch_taken_o = 1'b0;
      predict_branch_pc_o = fetch_pc_i;
    end
  end

endmodule

