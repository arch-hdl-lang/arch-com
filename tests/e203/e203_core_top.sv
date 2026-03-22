// E203 Core Top-Level Integration
// Full integration: IFU + EXU + LSU + BIU + ITCM + DTCM + CLINT
// Testbench loads instructions into ITCM via write port, then
// the IFU fetches and the core executes autonomously.
module CoreTop #(
  parameter int XLEN = 32
) (
  input logic clk,
  input logic rst_n,
  input logic itcm_wr_en,
  input logic [14-1:0] itcm_wr_addr,
  input logic [32-1:0] itcm_wr_data,
  input logic exu_redirect,
  input logic [32-1:0] exu_redirect_pc,
  output logic commit_valid,
  output logic [32-1:0] o_instr,
  output logic [32-1:0] o_pc,
  output logic o_valid,
  output logic tmr_irq
);

  logic itcm_fetch_rd_en;
  logic itcm_fetch_rd_addr;
  logic itcm_rsp_valid_d;
  logic itcm_wr_mux_en;
  logic itcm_wr_mux_addr;
  logic itcm_wr_mux_data;
  // ── ITCM write port (for testbench to load program) ────────────────
  // ── Branch redirect from EXU to IFU ────────────────────────────────
  // ── Status outputs ─────────────────────────────────────────────────
  // ── ITCM ───────────────────────────────────────────────────────────
  logic [32-1:0] itcm_fetch_rd_data;
  E203Itcm itcm (
    .clk(clk),
    .rst_n(rst_n),
    .rd_en(itcm_fetch_rd_en),
    .rd_addr(itcm_fetch_rd_addr),
    .rd_data(itcm_fetch_rd_data),
    .wr_en(itcm_wr_mux_en),
    .wr_addr(itcm_wr_mux_addr),
    .wr_data(itcm_wr_mux_data)
  );
  // ── IFU ────────────────────────────────────────────────────────────
  logic ifu_o_valid;
  logic [32-1:0] ifu_o_instr;
  logic [32-1:0] ifu_o_pc;
  logic ifu_bus_err;
  logic itcm_cmd_valid;
  logic [14-1:0] itcm_cmd_addr;
  logic ifu_rsp_ready;
  logic ifu_bpu_wait;
  logic ifu_bpu_rs1_ena;
  logic ifu_prdt_taken;
  logic [32-1:0] ifu_pc_op1;
  logic [32-1:0] ifu_pc_op2;
  logic ifu_dec_bjp;
  logic ifu_dec_lui;
  logic ifu_dec_auipc;
  IfuTop ifu (
    .clk(clk),
    .rst_n(rst_n),
    .o_ready(1'b1),
    .exu_redirect(exu_redirect),
    .exu_redirect_pc(exu_redirect_pc),
    .itcm_cmd_ready(1'b1),
    .itcm_rsp_valid(itcm_rsp_valid_d),
    .itcm_rsp_data(itcm_fetch_rd_data),
    .oitf_empty(1'b1),
    .ir_empty(1'b1),
    .ir_rs1en(1'b0),
    .jalr_rs1idx_cam_irrdidx(1'b0),
    .ir_valid_clr(1'b0),
    .rf2bpu_x1(0),
    .rf2bpu_rs1(0),
    .o_valid(ifu_o_valid),
    .o_instr(ifu_o_instr),
    .o_pc(ifu_o_pc),
    .o_bus_err(ifu_bus_err),
    .itcm_cmd_valid(itcm_cmd_valid),
    .itcm_cmd_addr(itcm_cmd_addr),
    .itcm_rsp_ready(ifu_rsp_ready),
    .bpu_wait(ifu_bpu_wait),
    .bpu2rf_rs1_ena(ifu_bpu_rs1_ena),
    .prdt_taken(ifu_prdt_taken),
    .prdt_pc_add_op1(ifu_pc_op1),
    .prdt_pc_add_op2(ifu_pc_op2),
    .dec_is_bjp(ifu_dec_bjp),
    .dec_is_lui(ifu_dec_lui),
    .dec_is_auipc(ifu_dec_auipc)
  );
  // ── EXU ────────────────────────────────────────────────────────────
  logic exu_ifu_ready;
  logic exu_bjp_valid;
  logic exu_bjp_taken;
  logic [32-1:0] exu_bjp_tgt;
  logic exu_lsu_valid;
  logic [32-1:0] exu_lsu_addr;
  logic [32-1:0] exu_lsu_wdata;
  logic exu_lsu_load;
  logic exu_lsu_store;
  logic exu_commit_valid;
  ExuTop exu (
    .clk(clk),
    .rst_n(rst_n),
    .ifu_valid(ifu_o_valid),
    .ifu_instr(ifu_o_instr),
    .ifu_pc(ifu_o_pc),
    .ifu_ready(exu_ifu_ready),
    .o_bjp_valid(exu_bjp_valid),
    .o_bjp_taken(exu_bjp_taken),
    .o_bjp_tgt(exu_bjp_tgt),
    .lsu_valid(exu_lsu_valid),
    .lsu_ready(1'b1),
    .lsu_addr(exu_lsu_addr),
    .lsu_wdata(exu_lsu_wdata),
    .lsu_load(exu_lsu_load),
    .lsu_store(exu_lsu_store),
    .lsu_resp_valid(1'b0),
    .lsu_resp_data(0),
    .o_commit_valid(exu_commit_valid)
  );
  // ── LSU ────────────────────────────────────────────────────────────
  logic [32-1:0] lsu_mem_addr;
  logic [32-1:0] lsu_mem_wdata;
  logic [4-1:0] lsu_mem_wstrb;
  logic lsu_mem_wen;
  logic [32-1:0] lsu_load_result;
  LsuCtrl lsu (
    .addr(exu_lsu_addr),
    .wdata(exu_lsu_wdata),
    .funct3(2),
    .is_load(exu_lsu_load),
    .is_store(exu_lsu_store),
    .mem_addr(lsu_mem_addr),
    .mem_wdata(lsu_mem_wdata),
    .mem_wstrb(lsu_mem_wstrb),
    .mem_wen(lsu_mem_wen),
    .mem_rdata(biu_lsu_rdata),
    .load_result(lsu_load_result)
  );
  // ── BIU ────────────────────────────────────────────────────────────
  logic [32-1:0] biu_lsu_rdata;
  logic biu_itcm_rd_en;
  logic [14-1:0] biu_itcm_rd_addr;
  logic biu_itcm_wr_en;
  logic [14-1:0] biu_itcm_wr_addr;
  logic [32-1:0] biu_itcm_wr_data;
  logic biu_dtcm_rd_en;
  logic [14-1:0] biu_dtcm_rd_addr;
  logic biu_dtcm_wr_en;
  logic [14-1:0] biu_dtcm_wr_addr;
  logic [32-1:0] biu_dtcm_wr_data;
  logic [4-1:0] biu_dtcm_wr_be;
  Biu bus (
    .lsu_addr(lsu_mem_addr),
    .lsu_wdata(lsu_mem_wdata),
    .lsu_wstrb(lsu_mem_wstrb),
    .lsu_wen(lsu_mem_wen),
    .lsu_ren(exu_lsu_load),
    .itcm_rd_data(itcm_fetch_rd_data),
    .dtcm_rd_data(dtcm_rd_data),
    .lsu_rdata(biu_lsu_rdata),
    .itcm_rd_en(biu_itcm_rd_en),
    .itcm_rd_addr(biu_itcm_rd_addr),
    .itcm_wr_en(biu_itcm_wr_en),
    .itcm_wr_addr(biu_itcm_wr_addr),
    .itcm_wr_data(biu_itcm_wr_data),
    .dtcm_rd_en(biu_dtcm_rd_en),
    .dtcm_rd_addr(biu_dtcm_rd_addr),
    .dtcm_wr_en(biu_dtcm_wr_en),
    .dtcm_wr_addr(biu_dtcm_wr_addr),
    .dtcm_wr_data(biu_dtcm_wr_data),
    .dtcm_wr_be(biu_dtcm_wr_be)
  );
  // ── DTCM ───────────────────────────────────────────────────────────
  logic [32-1:0] dtcm_rd_data;
  Dtcm dtcm (
    .clk(clk),
    .rst_n(rst_n),
    .rd_en(biu_dtcm_rd_en),
    .rd_addr(biu_dtcm_rd_addr),
    .rd_dout(dtcm_rd_data),
    .wr_en(biu_dtcm_wr_en),
    .wr_be(biu_dtcm_wr_be),
    .wr_addr(biu_dtcm_wr_addr),
    .wr_din(biu_dtcm_wr_data)
  );
  // ── CLINT Timer ────────────────────────────────────────────────────
  logic [32-1:0] timer_rdata;
  ClintTimer timer (
    .clk(clk),
    .rst(1'b0),
    .reg_addr(0),
    .reg_wdata(0),
    .reg_wen(1'b0),
    .reg_rdata(timer_rdata),
    .tmr_irq(tmr_irq)
  );
  // ── ITCM fetch: IFU cmd → ITCM read port ──────────────────────────
  // ITCM rsp_valid: delay cmd_valid by 1 cycle (ITCM latency 1)
  logic itcm_rsp_valid_r = 1'b0;
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      itcm_rsp_valid_r <= 1'b0;
    end else begin
      itcm_rsp_valid_r <= itcm_cmd_valid;
    end
  end
  // ── ITCM write mux: testbench loader vs BIU store ─────────────────
  always_comb begin
    itcm_fetch_rd_en = itcm_cmd_valid;
    itcm_fetch_rd_addr = itcm_cmd_addr;
    itcm_rsp_valid_d = itcm_rsp_valid_r;
    if (itcm_wr_en) begin
      itcm_wr_mux_en = 1'b1;
      itcm_wr_mux_addr = itcm_wr_addr;
      itcm_wr_mux_data = itcm_wr_data;
    end else if (biu_itcm_wr_en) begin
      itcm_wr_mux_en = 1'b1;
      itcm_wr_mux_addr = biu_itcm_wr_addr;
      itcm_wr_mux_data = biu_itcm_wr_data;
    end else begin
      itcm_wr_mux_en = 1'b0;
      itcm_wr_mux_addr = 0;
      itcm_wr_mux_data = 0;
    end
    o_valid = ifu_o_valid;
    o_instr = ifu_o_instr;
    o_pc = ifu_o_pc;
    commit_valid = exu_commit_valid;
  end

endmodule

