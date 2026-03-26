// E203 HBirdv2 IFU Mini Decoder
// Lightweight combinational decoder used inside the IFU to quickly classify
// fetched instructions before full decode in the EXU. Extracts just enough
// info for branch/jump handling.
module IfuMinidec (
  input logic [32-1:0] instr,
  output logic o_is_bjp,
  output logic o_is_jal,
  output logic o_is_jalr,
  output logic o_is_bxx,
  output logic o_is_lui,
  output logic o_is_auipc,
  output logic signed [21-1:0] o_bjp_imm,
  output logic [5-1:0] o_rs1_idx
);

  // ── Instruction field extraction ─────────────────────────────────────
  logic [7-1:0] opcode;
  assign opcode = instr[6:0];
  // ── Opcode classification ────────────────────────────────────────────
  logic dec_bxx;
  assign dec_bxx = opcode == 'h63;
  // Branch: BEQ/BNE/BLT/BGE/BLTU/BGEU
  logic dec_jal;
  assign dec_jal = opcode == 'h6F;
  // JAL
  logic dec_jalr;
  assign dec_jalr = opcode == 'h67;
  // JALR
  logic dec_lui;
  assign dec_lui = opcode == 'h37;
  // LUI
  logic dec_auipc;
  assign dec_auipc = opcode == 'h17;
  // AUIPC
  // ── B-type immediate: {instr[31], instr[7], instr[30:25], instr[11:8], 1'b0} ──
  logic [32-1:0] bimm_12;
  assign bimm_12 = 32'($unsigned(instr[31:31])) << 12;
  logic [32-1:0] bimm_11;
  assign bimm_11 = 32'($unsigned(instr[7:7])) << 11;
  logic [32-1:0] bimm_10_5;
  assign bimm_10_5 = 32'($unsigned(instr[30:25])) << 5;
  logic [32-1:0] bimm_4_1;
  assign bimm_4_1 = 32'($unsigned(instr[11:8])) << 1;
  logic [13-1:0] bimm_raw;
  assign bimm_raw = 13'(bimm_12 | bimm_11 | bimm_10_5 | bimm_4_1);
  logic signed [21-1:0] bimm_se;
  assign bimm_se = {{(21-$bits(bimm_raw)){bimm_raw[$bits(bimm_raw)-1]}}, bimm_raw};
  // ── J-type immediate: {instr[31], instr[19:12], instr[20], instr[30:21], 1'b0} ──
  logic [32-1:0] jimm_20;
  assign jimm_20 = 32'($unsigned(instr[31:31])) << 20;
  logic [32-1:0] jimm_19_12;
  assign jimm_19_12 = 32'($unsigned(instr[19:12])) << 12;
  logic [32-1:0] jimm_11;
  assign jimm_11 = 32'($unsigned(instr[20:20])) << 11;
  logic [32-1:0] jimm_10_1;
  assign jimm_10_1 = 32'($unsigned(instr[30:21])) << 1;
  logic [21-1:0] jimm_raw;
  assign jimm_raw = 21'(jimm_20 | jimm_19_12 | jimm_11 | jimm_10_1);
  logic signed [21-1:0] jimm_se;
  assign jimm_se = {{(21-$bits(jimm_raw)){jimm_raw[$bits(jimm_raw)-1]}}, jimm_raw};
  // ── I-type immediate (JALR): sign-extend instr[31:20] ──
  logic [12-1:0] iimm_raw;
  assign iimm_raw = instr[31:20];
  logic signed [21-1:0] iimm_se;
  assign iimm_se = {{(21-$bits(iimm_raw)){iimm_raw[$bits(iimm_raw)-1]}}, iimm_raw};
  // ── Output logic ─────────────────────────────────────────────────────
  always_comb begin
    o_is_bxx = dec_bxx;
    o_is_jal = dec_jal;
    o_is_jalr = dec_jalr;
    o_is_bjp = dec_bxx | dec_jal | dec_jalr;
    o_is_lui = dec_lui;
    o_is_auipc = dec_auipc;
    o_rs1_idx = instr[19:15];
    if (dec_bxx) begin
      o_bjp_imm = bimm_se;
    end else if (dec_jal) begin
      o_bjp_imm = jimm_se;
    end else if (dec_jalr) begin
      o_bjp_imm = iimm_se;
    end else begin
      o_bjp_imm = 0;
    end
  end

endmodule

