// E203 HBirdv2 Instruction Fetch Unit — integration wrapper
// Instantiates e203_ifu_ifetch (PC gen + fetch FSM) and e203_ifu_ift2icb (fetch-to-ICB bridge).
// Exposes full RealBench port list; hardwires ifu_active = true.
module e203_ifu (
  input logic clk,
  input logic rst_n,
  output logic [32-1:0] inspect_pc,
  output logic ifu_active,
  input logic itcm_nohold,
  input logic [32-1:0] pc_rtvec,
  input logic ifu2itcm_holdup,
  input logic [32-1:0] itcm_region_indic,
  output logic ifu2itcm_icb_cmd_valid,
  input logic ifu2itcm_icb_cmd_ready,
  output logic [16-1:0] ifu2itcm_icb_cmd_addr,
  input logic ifu2itcm_icb_rsp_valid,
  output logic ifu2itcm_icb_rsp_ready,
  input logic ifu2itcm_icb_rsp_err,
  input logic [64-1:0] ifu2itcm_icb_rsp_rdata,
  output logic ifu2biu_icb_cmd_valid,
  input logic ifu2biu_icb_cmd_ready,
  output logic [32-1:0] ifu2biu_icb_cmd_addr,
  input logic ifu2biu_icb_rsp_valid,
  output logic ifu2biu_icb_rsp_ready,
  input logic ifu2biu_icb_rsp_err,
  input logic [32-1:0] ifu2biu_icb_rsp_rdata,
  output logic [32-1:0] ifu_o_ir,
  output logic [32-1:0] ifu_o_pc,
  output logic ifu_o_pc_vld,
  output logic ifu_o_misalgn,
  output logic ifu_o_buserr,
  output logic [5-1:0] ifu_o_rs1idx,
  output logic [5-1:0] ifu_o_rs2idx,
  output logic ifu_o_prdt_taken,
  output logic ifu_o_muldiv_b2b,
  output logic ifu_o_valid,
  input logic ifu_o_ready,
  output logic pipe_flush_ack,
  input logic pipe_flush_req,
  input logic [32-1:0] pipe_flush_add_op1,
  input logic [32-1:0] pipe_flush_add_op2,
  input logic [32-1:0] pipe_flush_pc,
  input logic ifu_halt_req,
  output logic ifu_halt_ack,
  input logic oitf_empty,
  input logic [32-1:0] rf2ifu_x1,
  input logic [32-1:0] rf2ifu_rs1,
  input logic dec2ifu_rden,
  input logic dec2ifu_rs1en,
  input logic [5-1:0] dec2ifu_rdidx,
  input logic dec2ifu_mulhsu,
  input logic dec2ifu_div,
  input logic dec2ifu_rem,
  input logic dec2ifu_divu,
  input logic dec2ifu_remu
);

  // ── Inspect / status ───────────────────────────────────────────────
  // ── ITCM config ────────────────────────────────────────────────────
  // ── ITCM ICB master ────────────────────────────────────────────────
  // ── BIU ICB master (external memory path) ──────────────────────────
  // ── Instruction output to decode ───────────────────────────────────
  // ── Pipe flush ─────────────────────────────────────────────────────
  // ── Halt ───────────────────────────────────────────────────────────
  // ── OITF / regfile / decode feedback ───────────────────────────────
  // ── Hardwired outputs ──────────────────────────────────────────────
  assign ifu_active = 1'b1;
  // ── Internal wires: ifetch <-> ift2icb ─────────────────────────────
  logic ifu_req_valid_w;
  logic ifu_req_ready_w;
  logic [32-1:0] ifu_req_pc_w;
  logic ifu_rsp_valid_w;
  logic ifu_rsp_ready_w;
  logic [32-1:0] ifu_rsp_instr_w;
  // ── Ifetch -> ift2icb wires (seq/last_pc) ────────────────────────────
  logic ifu_req_seq_w;
  logic ifu_req_seq_rv32_w;
  logic [32-1:0] ifu_req_last_pc_w;
  logic ifu_rsp_err_w;
  // ── e203_ifu_ifetch: PC generation + fetch state machine ───────────
  e203_ifu_ifetch ifetch (
    .clk(clk),
    .rst_n(rst_n),
    .pc_rtvec(pc_rtvec),
    .ifu_req_ready(ifu_req_ready_w),
    .ifu_rsp_valid(ifu_rsp_valid_w),
    .ifu_rsp_err(1'b0),
    .ifu_rsp_instr(ifu_rsp_instr_w),
    .ifu_o_ready(ifu_o_ready),
    .pipe_flush_req(pipe_flush_req),
    .pipe_flush_add_op1(pipe_flush_add_op1),
    .pipe_flush_add_op2(pipe_flush_add_op2),
    .pipe_flush_pc(pipe_flush_pc),
    .ifu_halt_req(ifu_halt_req),
    .oitf_empty(oitf_empty),
    .rf2ifu_x1(rf2ifu_x1),
    .rf2ifu_rs1(rf2ifu_rs1),
    .dec2ifu_rs1en(dec2ifu_rs1en),
    .dec2ifu_rden(dec2ifu_rden),
    .dec2ifu_rdidx(dec2ifu_rdidx),
    .dec2ifu_mulhsu(dec2ifu_mulhsu),
    .dec2ifu_div(dec2ifu_div),
    .dec2ifu_rem(dec2ifu_rem),
    .dec2ifu_divu(dec2ifu_divu),
    .dec2ifu_remu(dec2ifu_remu),
    .inspect_pc(inspect_pc),
    .ifu_req_valid(ifu_req_valid_w),
    .ifu_req_pc(ifu_req_pc_w),
    .ifu_req_seq(ifu_req_seq_w),
    .ifu_req_seq_rv32(ifu_req_seq_rv32_w),
    .ifu_req_last_pc(ifu_req_last_pc_w),
    .ifu_rsp_ready(ifu_rsp_ready_w),
    .ifu_o_ir(ifu_o_ir),
    .ifu_o_pc(ifu_o_pc),
    .ifu_o_pc_vld(ifu_o_pc_vld),
    .ifu_o_rs1idx(ifu_o_rs1idx),
    .ifu_o_rs2idx(ifu_o_rs2idx),
    .ifu_o_prdt_taken(ifu_o_prdt_taken),
    .ifu_o_misalgn(ifu_o_misalgn),
    .ifu_o_buserr(ifu_o_buserr),
    .ifu_o_muldiv_b2b(ifu_o_muldiv_b2b),
    .ifu_o_valid(ifu_o_valid),
    .pipe_flush_ack(pipe_flush_ack),
    .ifu_halt_ack(ifu_halt_ack)
  );
  // ── e203_ifu_ift2icb: fetch-to-ITCM ICB bridge ────────────────────
  e203_ifu_ift2icb icb (
    .clk(clk),
    .rst_n(rst_n),
    .itcm_nohold(itcm_nohold),
    .ifu_req_valid(ifu_req_valid_w),
    .ifu_req_pc(ifu_req_pc_w),
    .ifu_req_seq(ifu_req_seq_w),
    .ifu_req_seq_rv32(ifu_req_seq_rv32_w),
    .ifu_req_last_pc(ifu_req_last_pc_w),
    .ifu_rsp_ready(ifu_rsp_ready_w),
    .itcm_region_indic(itcm_region_indic),
    .ifu2itcm_icb_cmd_ready(ifu2itcm_icb_cmd_ready),
    .ifu2itcm_icb_rsp_valid(ifu2itcm_icb_rsp_valid),
    .ifu2itcm_icb_rsp_err(ifu2itcm_icb_rsp_err),
    .ifu2itcm_icb_rsp_rdata(ifu2itcm_icb_rsp_rdata),
    .ifu2biu_icb_cmd_ready(ifu2biu_icb_cmd_ready),
    .ifu2biu_icb_rsp_valid(ifu2biu_icb_rsp_valid),
    .ifu2biu_icb_rsp_err(ifu2biu_icb_rsp_err),
    .ifu2biu_icb_rsp_rdata(ifu2biu_icb_rsp_rdata),
    .ifu2itcm_holdup(ifu2itcm_holdup),
    .ifu_req_ready(ifu_req_ready_w),
    .ifu_rsp_valid(ifu_rsp_valid_w),
    .ifu_rsp_err(ifu_rsp_err_w),
    .ifu_rsp_instr(ifu_rsp_instr_w),
    .ifu2itcm_icb_cmd_valid(ifu2itcm_icb_cmd_valid),
    .ifu2itcm_icb_cmd_addr(ifu2itcm_icb_cmd_addr),
    .ifu2itcm_icb_rsp_ready(ifu2itcm_icb_rsp_ready),
    .ifu2biu_icb_cmd_valid(ifu2biu_icb_cmd_valid),
    .ifu2biu_icb_cmd_addr(ifu2biu_icb_cmd_addr),
    .ifu2biu_icb_rsp_ready(ifu2biu_icb_rsp_ready)
  );

endmodule

