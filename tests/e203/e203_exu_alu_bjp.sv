// E203 HBirdv2 Branch/Jump Unit
// Decodes the 17-bit info bus to determine branch type, then issues
// ALU datapath requests (add for target addr, compare for branch condition).
// Purely combinational — matches RealBench port interface.
module e203_exu_alu_bjp #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  input logic bjp_i_valid,
  output logic bjp_i_ready,
  input logic [32-1:0] bjp_i_rs1,
  input logic [32-1:0] bjp_i_rs2,
  input logic [32-1:0] bjp_i_imm,
  input logic [32-1:0] bjp_i_pc,
  input logic [17-1:0] bjp_i_info,
  output logic bjp_o_valid,
  input logic bjp_o_ready,
  output logic [32-1:0] bjp_o_wbck_wdat,
  output logic bjp_o_wbck_err,
  output logic bjp_o_cmt_bjp,
  output logic bjp_o_cmt_mret,
  output logic bjp_o_cmt_dret,
  output logic bjp_o_cmt_fencei,
  output logic bjp_o_cmt_prdt,
  output logic bjp_o_cmt_rslv,
  output logic [32-1:0] bjp_req_alu_op1,
  output logic [32-1:0] bjp_req_alu_op2,
  output logic bjp_req_alu_cmp_eq,
  output logic bjp_req_alu_cmp_ne,
  output logic bjp_req_alu_cmp_lt,
  output logic bjp_req_alu_cmp_gt,
  output logic bjp_req_alu_cmp_ltu,
  output logic bjp_req_alu_cmp_gtu,
  output logic bjp_req_alu_add,
  input logic bjp_req_alu_cmp_res,
  input logic [32-1:0] bjp_req_alu_add_res
);

  // ── Dispatch handshake ────────────────────────────────────────────
  // ── Writeback handshake ───────────────────────────────────────────
  // ── Commit signals ────────────────────────────────────────────────
  // ── ALU datapath request ──────────────────────────────────────────
  // ── ALU datapath results (from shared datapath) ───────────────────
  // ── Decode info bus ───────────────────────────────────────────────
  // info[3:0] = branch type one-hot: beq, bne, blt, bge, bltu, bgeu
  // We decode branch type from the info bus.
  logic is_beq;
  assign is_beq = bjp_i_info[0:0] != 0;
  logic is_bne;
  assign is_bne = bjp_i_info[1:1] != 0;
  logic is_blt;
  assign is_blt = bjp_i_info[2:2] != 0;
  logic is_bge;
  assign is_bge = bjp_i_info[3:3] != 0;
  logic is_bltu;
  assign is_bltu = bjp_i_info[4:4] != 0;
  logic is_bgeu;
  assign is_bgeu = bjp_i_info[5:5] != 0;
  logic is_jal;
  assign is_jal = bjp_i_info[6:6] != 0;
  logic is_jalr;
  assign is_jalr = bjp_i_info[7:7] != 0;
  logic is_mret;
  assign is_mret = bjp_i_info[8:8] != 0;
  logic is_dret;
  assign is_dret = bjp_i_info[9:9] != 0;
  logic is_fencei;
  assign is_fencei = bjp_i_info[10:10] != 0;
  logic prdt_taken;
  assign prdt_taken = bjp_i_info[11:11] != 0;
  logic is_bxx;
  assign is_bxx = is_beq | is_bne | is_blt | is_bge | is_bltu | is_bgeu;
  logic is_jump;
  assign is_jump = is_jal | is_jalr;
  // Branch resolved (taken or not)
  logic branch_taken;
  assign branch_taken = is_bxx & bjp_req_alu_cmp_res;
  logic jump_taken;
  assign jump_taken = is_jump;
  logic taken;
  assign taken = branch_taken | jump_taken;
  // Link address = PC + imm (for JAL/JALR the add_res gives target;
  // wbck_wdat = PC+4 for link)
  logic [33-1:0] pc_plus4;
  assign pc_plus4 = 33'(33'($unsigned(bjp_i_pc)) + 33'($unsigned(4)));
  logic [32-1:0] link_addr;
  assign link_addr = 32'(pc_plus4);
  always_comb begin
    // Always ready (combinational unit)
    bjp_i_ready = bjp_o_ready;
    bjp_o_valid = bjp_i_valid;
    bjp_o_wbck_err = 1'b0;
    // Writeback data: link address for JAL/JALR, else 0
    if (is_jump) begin
      bjp_o_wbck_wdat = link_addr;
    end else begin
      bjp_o_wbck_wdat = 0;
    end
    // Commit outputs
    bjp_o_cmt_bjp = is_bxx | is_jump;
    bjp_o_cmt_mret = is_mret;
    bjp_o_cmt_dret = is_dret;
    bjp_o_cmt_fencei = is_fencei;
    bjp_o_cmt_prdt = prdt_taken;
    bjp_o_cmt_rslv = taken;
    // ALU datapath requests: operands for compare and add
    bjp_req_alu_op1 = bjp_i_rs1;
    bjp_req_alu_op2 = bjp_i_rs2;
    // Compare request one-hot
    bjp_req_alu_cmp_eq = is_beq;
    bjp_req_alu_cmp_ne = is_bne;
    bjp_req_alu_cmp_lt = is_blt;
    bjp_req_alu_cmp_gt = is_bge;
    bjp_req_alu_cmp_ltu = is_bltu;
    bjp_req_alu_cmp_gtu = is_bgeu;
    bjp_req_alu_add = is_jump | is_bxx;
  end

endmodule

