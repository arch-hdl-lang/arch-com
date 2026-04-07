// PG021-compatible AXI DMA — top-level integration.
// Supports Simple DMA mode (LENGTH write) and Scatter-Gather mode (TAILDESC write).
module AxiDmaTop (
  input logic clk,
  input logic rst,
  input logic s_axil_aw_valid,
  output logic s_axil_aw_ready,
  input logic [8-1:0] s_axil_aw_addr,
  input logic s_axil_w_valid,
  output logic s_axil_w_ready,
  input logic [32-1:0] s_axil_w_data,
  input logic [4-1:0] s_axil_w_strb,
  output logic s_axil_b_valid,
  input logic s_axil_b_ready,
  output logic [2-1:0] s_axil_b_resp,
  input logic s_axil_ar_valid,
  output logic s_axil_ar_ready,
  input logic [8-1:0] s_axil_ar_addr,
  output logic s_axil_r_valid,
  input logic s_axil_r_ready,
  output logic [32-1:0] s_axil_r_data,
  output logic [2-1:0] s_axil_r_resp,
  output logic m_axi_mm2s_ar_valid,
  input logic m_axi_mm2s_ar_ready,
  output logic [32-1:0] m_axi_mm2s_ar_addr,
  output logic [1-1:0] m_axi_mm2s_ar_id,
  output logic [8-1:0] m_axi_mm2s_ar_len,
  output logic [3-1:0] m_axi_mm2s_ar_size,
  output logic [2-1:0] m_axi_mm2s_ar_burst,
  input logic m_axi_mm2s_r_valid,
  output logic m_axi_mm2s_r_ready,
  input logic [32-1:0] m_axi_mm2s_r_data,
  input logic [1-1:0] m_axi_mm2s_r_id,
  input logic [2-1:0] m_axi_mm2s_r_resp,
  input logic m_axi_mm2s_r_last,
  output logic m_axi_s2mm_aw_valid,
  input logic m_axi_s2mm_aw_ready,
  output logic [32-1:0] m_axi_s2mm_aw_addr,
  output logic [1-1:0] m_axi_s2mm_aw_id,
  output logic [8-1:0] m_axi_s2mm_aw_len,
  output logic [3-1:0] m_axi_s2mm_aw_size,
  output logic [2-1:0] m_axi_s2mm_aw_burst,
  output logic m_axi_s2mm_w_valid,
  input logic m_axi_s2mm_w_ready,
  output logic [32-1:0] m_axi_s2mm_w_data,
  output logic [4-1:0] m_axi_s2mm_w_strb,
  output logic m_axi_s2mm_w_last,
  input logic m_axi_s2mm_b_valid,
  output logic m_axi_s2mm_b_ready,
  input logic [1-1:0] m_axi_s2mm_b_id,
  input logic [2-1:0] m_axi_s2mm_b_resp,
  output logic m_axi_mm2s_sg_ar_valid,
  input logic m_axi_mm2s_sg_ar_ready,
  output logic [32-1:0] m_axi_mm2s_sg_ar_addr,
  output logic [1-1:0] m_axi_mm2s_sg_ar_id,
  output logic [8-1:0] m_axi_mm2s_sg_ar_len,
  output logic [3-1:0] m_axi_mm2s_sg_ar_size,
  output logic [2-1:0] m_axi_mm2s_sg_ar_burst,
  input logic m_axi_mm2s_sg_r_valid,
  output logic m_axi_mm2s_sg_r_ready,
  input logic [32-1:0] m_axi_mm2s_sg_r_data,
  input logic [1-1:0] m_axi_mm2s_sg_r_id,
  input logic [2-1:0] m_axi_mm2s_sg_r_resp,
  input logic m_axi_mm2s_sg_r_last,
  output logic m_axi_mm2s_sg_aw_valid,
  input logic m_axi_mm2s_sg_aw_ready,
  output logic [32-1:0] m_axi_mm2s_sg_aw_addr,
  output logic [1-1:0] m_axi_mm2s_sg_aw_id,
  output logic [8-1:0] m_axi_mm2s_sg_aw_len,
  output logic [3-1:0] m_axi_mm2s_sg_aw_size,
  output logic [2-1:0] m_axi_mm2s_sg_aw_burst,
  output logic m_axi_mm2s_sg_w_valid,
  input logic m_axi_mm2s_sg_w_ready,
  output logic [32-1:0] m_axi_mm2s_sg_w_data,
  output logic [4-1:0] m_axi_mm2s_sg_w_strb,
  output logic m_axi_mm2s_sg_w_last,
  input logic m_axi_mm2s_sg_b_valid,
  output logic m_axi_mm2s_sg_b_ready,
  input logic [1-1:0] m_axi_mm2s_sg_b_id,
  input logic [2-1:0] m_axi_mm2s_sg_b_resp,
  output logic m_axi_s2mm_sg_ar_valid,
  input logic m_axi_s2mm_sg_ar_ready,
  output logic [32-1:0] m_axi_s2mm_sg_ar_addr,
  output logic [1-1:0] m_axi_s2mm_sg_ar_id,
  output logic [8-1:0] m_axi_s2mm_sg_ar_len,
  output logic [3-1:0] m_axi_s2mm_sg_ar_size,
  output logic [2-1:0] m_axi_s2mm_sg_ar_burst,
  input logic m_axi_s2mm_sg_r_valid,
  output logic m_axi_s2mm_sg_r_ready,
  input logic [32-1:0] m_axi_s2mm_sg_r_data,
  input logic [1-1:0] m_axi_s2mm_sg_r_id,
  input logic [2-1:0] m_axi_s2mm_sg_r_resp,
  input logic m_axi_s2mm_sg_r_last,
  output logic m_axi_s2mm_sg_aw_valid,
  input logic m_axi_s2mm_sg_aw_ready,
  output logic [32-1:0] m_axi_s2mm_sg_aw_addr,
  output logic [1-1:0] m_axi_s2mm_sg_aw_id,
  output logic [8-1:0] m_axi_s2mm_sg_aw_len,
  output logic [3-1:0] m_axi_s2mm_sg_aw_size,
  output logic [2-1:0] m_axi_s2mm_sg_aw_burst,
  output logic m_axi_s2mm_sg_w_valid,
  input logic m_axi_s2mm_sg_w_ready,
  output logic [32-1:0] m_axi_s2mm_sg_w_data,
  output logic [4-1:0] m_axi_s2mm_sg_w_strb,
  output logic m_axi_s2mm_sg_w_last,
  input logic m_axi_s2mm_sg_b_valid,
  output logic m_axi_s2mm_sg_b_ready,
  input logic [1-1:0] m_axi_s2mm_sg_b_id,
  input logic [2-1:0] m_axi_s2mm_sg_b_resp,
  output logic m_axis_mm2s_tvalid,
  input logic m_axis_mm2s_tready,
  output logic [32-1:0] m_axis_mm2s_tdata,
  output logic m_axis_mm2s_tlast,
  output logic [4-1:0] m_axis_mm2s_tkeep,
  input logic s_axis_s2mm_tvalid,
  output logic s_axis_s2mm_tready,
  input logic [32-1:0] s_axis_s2mm_tdata,
  input logic s_axis_s2mm_tlast,
  input logic [4-1:0] s_axis_s2mm_tkeep,
  output logic mm2s_introut,
  output logic s2mm_introut
);

  // ── AXI4-Lite slave (register access) ───────────────────────────────
  // ── AXI4 masters ────────────────────────────────────────────────────
  // MM2S data reads
  // S2MM data writes
  // MM2S SG descriptor access
  // S2MM SG descriptor access
  // ── AXI4-Stream ─────────────────────────────────────────────────────
  // ── Interrupts ──────────────────────────────────────────────────────
  // ── Internal wires ──────────────────────────────────────────────────
  // Register block ↔ data FSMs
  logic mm2s_start_w;
  logic [32-1:0] mm2s_src_addr_w;
  logic [8-1:0] mm2s_num_beats_w;
  logic mm2s_done_w;
  logic mm2s_halted_w;
  logic mm2s_idle_w;
  logic s2mm_start_w;
  logic [32-1:0] s2mm_dst_addr_w;
  logic [8-1:0] s2mm_num_beats_w;
  logic s2mm_done_w;
  logic s2mm_halted_w;
  logic s2mm_idle_w;
  // Register block ↔ SG
  logic mm2s_sg_start_w;
  logic [32-1:0] mm2s_curdesc_w;
  logic [32-1:0] mm2s_taildesc_w;
  logic mm2s_sg_done_w;
  logic s2mm_sg_start_w;
  logic [32-1:0] s2mm_curdesc_w;
  logic [32-1:0] s2mm_taildesc_w;
  logic s2mm_sg_done_w;
  // SG ↔ data FSM (transfer triggers)
  logic mm2s_sg_xfer_start_w;
  logic [32-1:0] mm2s_sg_xfer_addr_w;
  logic [8-1:0] mm2s_sg_xfer_beats_w;
  logic s2mm_sg_xfer_start_w;
  logic [32-1:0] s2mm_sg_xfer_addr_w;
  logic [8-1:0] s2mm_sg_xfer_beats_w;
  logic mm2s_introut_w;
  logic s2mm_introut_w;
  // FIFO wires
  logic mm2s_push_valid_w;
  logic mm2s_push_ready_w;
  logic [32-1:0] mm2s_push_data_w;
  logic mm2s_pop_valid_w;
  logic mm2s_pop_ready_w;
  logic [32-1:0] mm2s_pop_data_w;
  logic s2mm_push_valid_w;
  logic s2mm_push_ready_w;
  logic [32-1:0] s2mm_push_data_w;
  logic s2mm_pop_valid_w;
  logic s2mm_pop_ready_w;
  logic [32-1:0] s2mm_pop_data_w;
  // Muxed data FSM control
  logic mm2s_fsm_start_w;
  logic [32-1:0] mm2s_fsm_addr_w;
  logic [8-1:0] mm2s_fsm_beats_w;
  logic s2mm_fsm_start_w;
  logic [32-1:0] s2mm_fsm_addr_w;
  logic [8-1:0] s2mm_fsm_beats_w;
  // Channel clock enables — active when channel is NOT halted
  logic mm2s_clk_en_w;
  logic s2mm_clk_en_w;
  // Gated clocks produced by ICG cells
  logic mm2s_gated_clk_w;
  logic s2mm_gated_clk_w;
  // Counters + flags
  logic [8-1:0] mm2s_stream_ctr;
  logic [8-1:0] s2mm_recv_ctr;
  logic mm2s_sg_active;
  logic s2mm_sg_active;
  // Timing: latch mm2s_fsm_beats_w at start so tlast logic starts from a FF,
  // not from the combinational SG-state → xfer_num_beats → beats-mux chain.
  logic [8-1:0] mm2s_beats_r;
  // Timing: lookahead register — true when the CURRENT stream beat is the last.
  // Precomputed one cycle early; critical path for tlast is 1 gate (AND with tvalid).
  logic mm2s_tlast_r;
  // ── Clock gate enables ──────────────────────────────────────────────
  // OR the start signals so the clock wakes up on the same cycle start fires,
  // before the FSM has had a chance to leave Idle (which would clear halted).
  // Without this, start fires into a gated clock and is permanently lost.
  assign mm2s_clk_en_w = ~mm2s_halted_w | mm2s_fsm_start_w | mm2s_sg_start_w;
  assign s2mm_clk_en_w = ~s2mm_halted_w | s2mm_fsm_start_w | s2mm_sg_start_w;
  // ── Instances ───────────────────────────────────────────────────────
  // ICG cells: gate each channel clock when that channel is halted
  ClkGateDma mm2s_icg (
    .clk_in(clk),
    .enable(mm2s_clk_en_w),
    .clk_out(mm2s_gated_clk_w)
  );
  ClkGateDma s2mm_icg (
    .clk_in(clk),
    .enable(s2mm_clk_en_w),
    .clk_out(s2mm_gated_clk_w)
  );
  AxiLiteRegs regs (
    .clk(clk),
    .rst(rst),
    .axil_aw_valid(s_axil_aw_valid),
    .axil_aw_ready(s_axil_aw_ready),
    .axil_aw_addr(s_axil_aw_addr),
    .axil_w_valid(s_axil_w_valid),
    .axil_w_ready(s_axil_w_ready),
    .axil_w_data(s_axil_w_data),
    .axil_w_strb(s_axil_w_strb),
    .axil_b_valid(s_axil_b_valid),
    .axil_b_ready(s_axil_b_ready),
    .axil_b_resp(s_axil_b_resp),
    .axil_ar_valid(s_axil_ar_valid),
    .axil_ar_ready(s_axil_ar_ready),
    .axil_ar_addr(s_axil_ar_addr),
    .axil_r_valid(s_axil_r_valid),
    .axil_r_ready(s_axil_r_ready),
    .axil_r_data(s_axil_r_data),
    .axil_r_resp(s_axil_r_resp),
    .mm2s_start(mm2s_start_w),
    .mm2s_src_addr(mm2s_src_addr_w),
    .mm2s_num_beats(mm2s_num_beats_w),
    .mm2s_done(mm2s_done_w),
    .mm2s_halted(mm2s_halted_w),
    .mm2s_idle(mm2s_idle_w),
    .s2mm_start(s2mm_start_w),
    .s2mm_dst_addr(s2mm_dst_addr_w),
    .s2mm_num_beats(s2mm_num_beats_w),
    .s2mm_done(s2mm_done_w),
    .s2mm_halted(s2mm_halted_w),
    .s2mm_idle(s2mm_idle_w),
    .mm2s_sg_start(mm2s_sg_start_w),
    .mm2s_curdesc_o(mm2s_curdesc_w),
    .mm2s_taildesc_o(mm2s_taildesc_w),
    .mm2s_sg_done(mm2s_sg_done_w),
    .s2mm_sg_start(s2mm_sg_start_w),
    .s2mm_curdesc_o(s2mm_curdesc_w),
    .s2mm_taildesc_o(s2mm_taildesc_w),
    .s2mm_sg_done(s2mm_sg_done_w),
    .mm2s_sg_active(mm2s_sg_active),
    .s2mm_sg_active(s2mm_sg_active),
    .mm2s_introut(mm2s_introut_w),
    .s2mm_introut(s2mm_introut_w)
  );
  FsmSgEngine mm2s_sg (
    .clk(mm2s_gated_clk_w),
    .rst(rst),
    .sg_start(mm2s_sg_start_w),
    .curdesc(mm2s_curdesc_w),
    .taildesc(mm2s_taildesc_w),
    .xfer_start(mm2s_sg_xfer_start_w),
    .xfer_addr(mm2s_sg_xfer_addr_w),
    .xfer_num_beats(mm2s_sg_xfer_beats_w),
    .xfer_done(mm2s_done_w),
    .sg_done(mm2s_sg_done_w),
    .sg_axi_ar_valid(m_axi_mm2s_sg_ar_valid),
    .sg_axi_ar_ready(m_axi_mm2s_sg_ar_ready),
    .sg_axi_ar_addr(m_axi_mm2s_sg_ar_addr),
    .sg_axi_ar_id(m_axi_mm2s_sg_ar_id),
    .sg_axi_ar_len(m_axi_mm2s_sg_ar_len),
    .sg_axi_ar_size(m_axi_mm2s_sg_ar_size),
    .sg_axi_ar_burst(m_axi_mm2s_sg_ar_burst),
    .sg_axi_r_valid(m_axi_mm2s_sg_r_valid),
    .sg_axi_r_ready(m_axi_mm2s_sg_r_ready),
    .sg_axi_r_data(m_axi_mm2s_sg_r_data),
    .sg_axi_r_id(m_axi_mm2s_sg_r_id),
    .sg_axi_r_resp(m_axi_mm2s_sg_r_resp),
    .sg_axi_r_last(m_axi_mm2s_sg_r_last),
    .sg_axi_aw_valid(m_axi_mm2s_sg_aw_valid),
    .sg_axi_aw_ready(m_axi_mm2s_sg_aw_ready),
    .sg_axi_aw_addr(m_axi_mm2s_sg_aw_addr),
    .sg_axi_aw_id(m_axi_mm2s_sg_aw_id),
    .sg_axi_aw_len(m_axi_mm2s_sg_aw_len),
    .sg_axi_aw_size(m_axi_mm2s_sg_aw_size),
    .sg_axi_aw_burst(m_axi_mm2s_sg_aw_burst),
    .sg_axi_w_valid(m_axi_mm2s_sg_w_valid),
    .sg_axi_w_ready(m_axi_mm2s_sg_w_ready),
    .sg_axi_w_data(m_axi_mm2s_sg_w_data),
    .sg_axi_w_strb(m_axi_mm2s_sg_w_strb),
    .sg_axi_w_last(m_axi_mm2s_sg_w_last),
    .sg_axi_b_valid(m_axi_mm2s_sg_b_valid),
    .sg_axi_b_ready(m_axi_mm2s_sg_b_ready),
    .sg_axi_b_id(m_axi_mm2s_sg_b_id),
    .sg_axi_b_resp(m_axi_mm2s_sg_b_resp)
  );
  FsmSgEngine s2mm_sg (
    .clk(s2mm_gated_clk_w),
    .rst(rst),
    .sg_start(s2mm_sg_start_w),
    .curdesc(s2mm_curdesc_w),
    .taildesc(s2mm_taildesc_w),
    .xfer_start(s2mm_sg_xfer_start_w),
    .xfer_addr(s2mm_sg_xfer_addr_w),
    .xfer_num_beats(s2mm_sg_xfer_beats_w),
    .xfer_done(s2mm_done_w),
    .sg_done(s2mm_sg_done_w),
    .sg_axi_ar_valid(m_axi_s2mm_sg_ar_valid),
    .sg_axi_ar_ready(m_axi_s2mm_sg_ar_ready),
    .sg_axi_ar_addr(m_axi_s2mm_sg_ar_addr),
    .sg_axi_ar_id(m_axi_s2mm_sg_ar_id),
    .sg_axi_ar_len(m_axi_s2mm_sg_ar_len),
    .sg_axi_ar_size(m_axi_s2mm_sg_ar_size),
    .sg_axi_ar_burst(m_axi_s2mm_sg_ar_burst),
    .sg_axi_r_valid(m_axi_s2mm_sg_r_valid),
    .sg_axi_r_ready(m_axi_s2mm_sg_r_ready),
    .sg_axi_r_data(m_axi_s2mm_sg_r_data),
    .sg_axi_r_id(m_axi_s2mm_sg_r_id),
    .sg_axi_r_resp(m_axi_s2mm_sg_r_resp),
    .sg_axi_r_last(m_axi_s2mm_sg_r_last),
    .sg_axi_aw_valid(m_axi_s2mm_sg_aw_valid),
    .sg_axi_aw_ready(m_axi_s2mm_sg_aw_ready),
    .sg_axi_aw_addr(m_axi_s2mm_sg_aw_addr),
    .sg_axi_aw_id(m_axi_s2mm_sg_aw_id),
    .sg_axi_aw_len(m_axi_s2mm_sg_aw_len),
    .sg_axi_aw_size(m_axi_s2mm_sg_aw_size),
    .sg_axi_aw_burst(m_axi_s2mm_sg_aw_burst),
    .sg_axi_w_valid(m_axi_s2mm_sg_w_valid),
    .sg_axi_w_ready(m_axi_s2mm_sg_w_ready),
    .sg_axi_w_data(m_axi_s2mm_sg_w_data),
    .sg_axi_w_strb(m_axi_s2mm_sg_w_strb),
    .sg_axi_w_last(m_axi_s2mm_sg_w_last),
    .sg_axi_b_valid(m_axi_s2mm_sg_b_valid),
    .sg_axi_b_ready(m_axi_s2mm_sg_b_ready),
    .sg_axi_b_id(m_axi_s2mm_sg_b_id),
    .sg_axi_b_resp(m_axi_s2mm_sg_b_resp)
  );
  FsmMm2s mm2s_fsm (
    .clk(mm2s_gated_clk_w),
    .rst(rst),
    .start(mm2s_fsm_start_w),
    .src_addr(mm2s_fsm_addr_w),
    .num_beats(mm2s_fsm_beats_w),
    .done(mm2s_done_w),
    .halted(mm2s_halted_w),
    .idle_out(mm2s_idle_w),
    .axi_rd_ar_valid(m_axi_mm2s_ar_valid),
    .axi_rd_ar_ready(m_axi_mm2s_ar_ready),
    .axi_rd_ar_addr(m_axi_mm2s_ar_addr),
    .axi_rd_ar_id(m_axi_mm2s_ar_id),
    .axi_rd_ar_len(m_axi_mm2s_ar_len),
    .axi_rd_ar_size(m_axi_mm2s_ar_size),
    .axi_rd_ar_burst(m_axi_mm2s_ar_burst),
    .axi_rd_r_valid(m_axi_mm2s_r_valid),
    .axi_rd_r_ready(m_axi_mm2s_r_ready),
    .axi_rd_r_data(m_axi_mm2s_r_data),
    .axi_rd_r_id(m_axi_mm2s_r_id),
    .axi_rd_r_resp(m_axi_mm2s_r_resp),
    .axi_rd_r_last(m_axi_mm2s_r_last),
    .push_valid(mm2s_push_valid_w),
    .push_ready(mm2s_push_ready_w),
    .push_data(mm2s_push_data_w)
  );
  Mm2sFifo mm2s_fifo (
    .clk(mm2s_gated_clk_w),
    .rst(rst),
    .push_valid(mm2s_push_valid_w),
    .push_ready(mm2s_push_ready_w),
    .push_data(mm2s_push_data_w),
    .pop_valid(mm2s_pop_valid_w),
    .pop_ready(mm2s_pop_ready_w),
    .pop_data(mm2s_pop_data_w)
  );
  S2mmFifo s2mm_fifo (
    .clk(s2mm_gated_clk_w),
    .rst(rst),
    .push_valid(s2mm_push_valid_w),
    .push_ready(s2mm_push_ready_w),
    .push_data(s2mm_push_data_w),
    .pop_valid(s2mm_pop_valid_w),
    .pop_ready(s2mm_pop_ready_w),
    .pop_data(s2mm_pop_data_w)
  );
  FsmS2mm s2mm_fsm (
    .clk(s2mm_gated_clk_w),
    .rst(rst),
    .start(s2mm_fsm_start_w),
    .dst_addr(s2mm_fsm_addr_w),
    .num_beats(s2mm_fsm_beats_w),
    .recv_count(s2mm_recv_ctr),
    .done(s2mm_done_w),
    .halted(s2mm_halted_w),
    .idle_out(s2mm_idle_w),
    .pop_valid(s2mm_pop_valid_w),
    .pop_ready(s2mm_pop_ready_w),
    .pop_data(s2mm_pop_data_w),
    .axi_wr_aw_valid(m_axi_s2mm_aw_valid),
    .axi_wr_aw_ready(m_axi_s2mm_aw_ready),
    .axi_wr_aw_addr(m_axi_s2mm_aw_addr),
    .axi_wr_aw_id(m_axi_s2mm_aw_id),
    .axi_wr_aw_len(m_axi_s2mm_aw_len),
    .axi_wr_aw_size(m_axi_s2mm_aw_size),
    .axi_wr_aw_burst(m_axi_s2mm_aw_burst),
    .axi_wr_w_valid(m_axi_s2mm_w_valid),
    .axi_wr_w_ready(m_axi_s2mm_w_ready),
    .axi_wr_w_data(m_axi_s2mm_w_data),
    .axi_wr_w_strb(m_axi_s2mm_w_strb),
    .axi_wr_w_last(m_axi_s2mm_w_last),
    .axi_wr_b_valid(m_axi_s2mm_b_valid),
    .axi_wr_b_ready(m_axi_s2mm_b_ready),
    .axi_wr_b_id(m_axi_s2mm_b_id),
    .axi_wr_b_resp(m_axi_s2mm_b_resp)
  );
  // ── Simple/SG mux ──────────────────────────────────────────────────
  always_comb begin
    if (mm2s_sg_active) begin
      mm2s_fsm_start_w = mm2s_sg_xfer_start_w;
      mm2s_fsm_addr_w = mm2s_sg_xfer_addr_w;
      mm2s_fsm_beats_w = mm2s_sg_xfer_beats_w;
    end else begin
      mm2s_fsm_start_w = mm2s_start_w;
      mm2s_fsm_addr_w = mm2s_src_addr_w;
      mm2s_fsm_beats_w = mm2s_num_beats_w;
    end
    if (s2mm_sg_active) begin
      s2mm_fsm_start_w = s2mm_sg_xfer_start_w;
      s2mm_fsm_addr_w = s2mm_sg_xfer_addr_w;
      s2mm_fsm_beats_w = s2mm_sg_xfer_beats_w;
    end else begin
      s2mm_fsm_start_w = s2mm_start_w;
      s2mm_fsm_addr_w = s2mm_dst_addr_w;
      s2mm_fsm_beats_w = s2mm_num_beats_w;
    end
  end
  // ── Stream wiring ───────────────────────────────────────────────────
  assign m_axis_mm2s_tdata = mm2s_pop_data_w;
  assign m_axis_mm2s_tvalid = mm2s_pop_valid_w;
  assign m_axis_mm2s_tkeep = 'hF;
  assign mm2s_pop_ready_w = m_axis_mm2s_tready;
  assign m_axis_mm2s_tlast = mm2s_pop_valid_w & mm2s_tlast_r;
  // tlast uses precomputed lookahead register — 1 gate on critical path
  assign s2mm_push_data_w = s_axis_s2mm_tdata;
  assign s2mm_push_valid_w = s_axis_s2mm_tvalid;
  assign s_axis_s2mm_tready = s2mm_push_ready_w;
  // ── Route interrupt outputs ──────────────────────────────────────────
  assign mm2s_introut = mm2s_introut_w;
  assign s2mm_introut = s2mm_introut_w;
  // ── Registered state ────────────────────────────────────────────────
  always_ff @(posedge clk) begin
    if (rst) begin
      mm2s_beats_r <= 0;
      mm2s_sg_active <= 1'b0;
      mm2s_stream_ctr <= 0;
      mm2s_tlast_r <= 1'b0;
      s2mm_recv_ctr <= 0;
      s2mm_sg_active <= 1'b0;
    end else begin
      if (mm2s_sg_start_w) begin
        mm2s_sg_active <= 1'b1;
      end else if (mm2s_sg_done_w) begin
        mm2s_sg_active <= 1'b0;
      end
      if (s2mm_sg_start_w) begin
        s2mm_sg_active <= 1'b1;
      end else if (s2mm_sg_done_w) begin
        s2mm_sg_active <= 1'b0;
      end
      if (mm2s_fsm_start_w) begin
        mm2s_stream_ctr <= 0;
        // Latch beats count — breaks SG-state combinational path from future cycles
        mm2s_beats_r <= mm2s_fsm_beats_w;
        // Lookahead: beat 0 is last iff total beats == 1
        mm2s_tlast_r <= mm2s_fsm_beats_w == 1;
      end else if (mm2s_pop_valid_w & m_axis_mm2s_tready) begin
        mm2s_stream_ctr <= 8'(mm2s_stream_ctr + 1);
        // Lookahead: next beat (stream_ctr+1) is last when stream_ctr+1 == beats_r-1
        //            i.e. stream_ctr + 2 == beats_r (no subtraction, both are FFs)
        mm2s_tlast_r <= 8'(mm2s_stream_ctr + 2) == mm2s_beats_r;
      end
      if (s2mm_fsm_start_w) begin
        s2mm_recv_ctr <= 0;
      end else if (s_axis_s2mm_tvalid & s2mm_push_ready_w) begin
        s2mm_recv_ctr <= 8'(s2mm_recv_ctr + 1);
      end
    end
  end

endmodule

