module static_branch_predict (
  input logic [31:0] fetch_rdata_i,
  input logic [31:0] fetch_pc_i,
  input logic fetch_valid_i,
  input logic [31:0] register_addr_i,
  output logic predict_branch_taken_o,
  output logic [31:0] predict_branch_pc_o,
  output logic [7:0] predict_confidence_o,
  output logic predict_exception_o,
  output logic [2:0] predict_branch_type_o,
  output logic [31:0] predict_branch_offset_o
);

  logic [6:0] OPCODE_BRANCH;
  assign OPCODE_BRANCH = 7'd99;
  logic [6:0] OPCODE_JAL;
  assign OPCODE_JAL = 7'd111;
  logic [6:0] OPCODE_JALR;
  assign OPCODE_JALR = 7'd103;
  logic [2:0] BRANCH_TYPE_NONE;
  assign BRANCH_TYPE_NONE = 3'd0;
  logic [2:0] BRANCH_TYPE_JAL;
  assign BRANCH_TYPE_JAL = 3'd1;
  logic [2:0] BRANCH_TYPE_JALR;
  assign BRANCH_TYPE_JALR = 3'd2;
  logic [2:0] BRANCH_TYPE_BRANCH;
  assign BRANCH_TYPE_BRANCH = 3'd3;
  logic [31:0] instr;
  assign instr = fetch_rdata_i;
  logic [6:0] opcode;
  assign opcode = instr[6:0];
  logic instr_j;
  assign instr_j = opcode == OPCODE_JAL;
  logic instr_b;
  assign instr_b = opcode == OPCODE_BRANCH;
  logic instr_jalr;
  assign instr_jalr = opcode == OPCODE_JALR;
  logic [31:0] imm_j_type;
  assign imm_j_type = {{12{instr[31]}}, instr[19:12], instr[20], instr[30:21], 1'd0};
  logic [31:0] imm_b_type;
  assign imm_b_type = {{20{instr[31]}}, instr[7], instr[30:25], instr[11:8], 1'd0};
  logic [31:0] imm_i_type;
  assign imm_i_type = {{20{instr[31]}}, instr[31:20]};
  logic instr_b_taken;
  assign instr_b_taken = instr[31];
  logic [31:0] branch_imm;
  logic branch_taken;
  logic [2:0] branch_type;
  logic [7:0] confidence;
  always_comb begin
    if (instr_j) begin
      branch_imm = imm_j_type;
      branch_taken = 1'b1;
      branch_type = BRANCH_TYPE_JAL;
      confidence = 100;
    end else if (instr_jalr) begin
      branch_imm = imm_i_type;
      branch_taken = 1'b1;
      branch_type = BRANCH_TYPE_JALR;
      confidence = 100;
    end else if (instr_b) begin
      branch_imm = imm_b_type;
      branch_taken = instr_b_taken;
      branch_type = BRANCH_TYPE_BRANCH;
      if (instr_b_taken) begin
        confidence = 90;
      end else begin
        confidence = 50;
      end
    end else begin
      branch_imm = 0;
      branch_taken = 1'b0;
      branch_type = BRANCH_TYPE_NONE;
      confidence = 0;
    end
  end
  always_comb begin
    predict_exception_o = 1'b0;
    if (fetch_valid_i) begin
      predict_branch_taken_o = branch_taken;
      predict_branch_pc_o = 32'(fetch_pc_i + branch_imm);
      predict_confidence_o = confidence;
      predict_branch_type_o = branch_type;
      predict_branch_offset_o = branch_imm;
    end else begin
      predict_branch_taken_o = 1'b0;
      predict_branch_pc_o = fetch_pc_i;
      predict_confidence_o = 0;
      predict_branch_type_o = BRANCH_TYPE_NONE;
      predict_branch_offset_o = 0;
    end
  end

endmodule

