// Multi-outstanding AXI DMA — top-level integration.
// Supports Simple DMA mode and Scatter-Gather mode.
// Data-path FSMs issue multiple outstanding AXI transactions.
module AxiDmaTop #(
  parameter int NUM_OUTSTANDING = 4
) (
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
  output logic [2-1:0] m_axi_mm2s_ar_id,
  output logic [8-1:0] m_axi_mm2s_ar_len,
  output logic [3-1:0] m_axi_mm2s_ar_size,
  output logic [2-1:0] m_axi_mm2s_ar_burst,
  input logic m_axi_mm2s_r_valid,
  output logic m_axi_mm2s_r_ready,
  input logic [32-1:0] m_axi_mm2s_r_data,
  input logic [2-1:0] m_axi_mm2s_r_id,
  input logic [2-1:0] m_axi_mm2s_r_resp,
  input logic m_axi_mm2s_r_last,
  output logic m_axi_mm2s_aw_valid,
  input logic m_axi_mm2s_aw_ready,
  output logic [32-1:0] m_axi_mm2s_aw_addr,
  output logic [2-1:0] m_axi_mm2s_aw_id,
  output logic [8-1:0] m_axi_mm2s_aw_len,
  output logic [3-1:0] m_axi_mm2s_aw_size,
  output logic [2-1:0] m_axi_mm2s_aw_burst,
  output logic m_axi_mm2s_w_valid,
  input logic m_axi_mm2s_w_ready,
  output logic [32-1:0] m_axi_mm2s_w_data,
  output logic [4-1:0] m_axi_mm2s_w_strb,
  output logic m_axi_mm2s_w_last,
  input logic m_axi_mm2s_b_valid,
  output logic m_axi_mm2s_b_ready,
  input logic [2-1:0] m_axi_mm2s_b_id,
  input logic [2-1:0] m_axi_mm2s_b_resp,
  output logic m_axi_s2mm_ar_valid,
  input logic m_axi_s2mm_ar_ready,
  output logic [32-1:0] m_axi_s2mm_ar_addr,
  output logic [2-1:0] m_axi_s2mm_ar_id,
  output logic [8-1:0] m_axi_s2mm_ar_len,
  output logic [3-1:0] m_axi_s2mm_ar_size,
  output logic [2-1:0] m_axi_s2mm_ar_burst,
  input logic m_axi_s2mm_r_valid,
  output logic m_axi_s2mm_r_ready,
  input logic [32-1:0] m_axi_s2mm_r_data,
  input logic [2-1:0] m_axi_s2mm_r_id,
  input logic [2-1:0] m_axi_s2mm_r_resp,
  input logic m_axi_s2mm_r_last,
  output logic m_axi_s2mm_aw_valid,
  input logic m_axi_s2mm_aw_ready,
  output logic [32-1:0] m_axi_s2mm_aw_addr,
  output logic [2-1:0] m_axi_s2mm_aw_id,
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
  input logic [2-1:0] m_axi_s2mm_b_id,
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

  // AXI4-Lite slave (register access)
  // AXI4 masters
  // AXI4-Stream
  // Interrupts
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
  // SG ↔ data FSM
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
  // Muxed FSM control
  logic mm2s_fsm_start_w;
  logic [32-1:0] mm2s_fsm_addr_w;
  logic [8-1:0] mm2s_fsm_beats_w;
  logic s2mm_fsm_start_w;
  logic [32-1:0] s2mm_fsm_addr_w;
  logic [8-1:0] s2mm_fsm_beats_w;
  // Clock gate
  logic mm2s_clk_en_w;
  logic s2mm_clk_en_w;
  logic mm2s_gated_clk_w;
  logic s2mm_gated_clk_w;
  // Counters
  logic [8-1:0] mm2s_stream_ctr;
  logic [8-1:0] s2mm_recv_ctr;
  logic mm2s_sg_active;
  logic s2mm_sg_active;
  logic [8-1:0] mm2s_beats_r;
  logic mm2s_tlast_r;
  // ── Clock gate ──────────────────────────────────────────────────────
  assign mm2s_clk_en_w = ~mm2s_halted_w | mm2s_fsm_start_w | mm2s_sg_start_w;
  assign s2mm_clk_en_w = ~s2mm_halted_w | s2mm_fsm_start_w | s2mm_sg_start_w;
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
  // ── Register block ──────────────────────────────────────────────────
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
  // ── SG engines ──────────────────────────────────────────────────────
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
  // ── MM2S multi-outstanding read FSM ─────────────────────────────────
  ThreadMm2s mm2s_fsm (
    .clk(mm2s_gated_clk_w),
    .rst(rst),
    .start(mm2s_fsm_start_w),
    .total_xfers(1),
    .base_addr(mm2s_fsm_addr_w),
    .burst_len(mm2s_fsm_beats_w),
    .done(mm2s_done_w),
    .halted(mm2s_halted_w),
    .idle_out(mm2s_idle_w),
    .ar_valid(m_axi_mm2s_ar_valid),
    .ar_ready(m_axi_mm2s_ar_ready),
    .ar_addr(m_axi_mm2s_ar_addr),
    .ar_id(m_axi_mm2s_ar_id),
    .ar_len(m_axi_mm2s_ar_len),
    .ar_size(m_axi_mm2s_ar_size),
    .ar_burst(m_axi_mm2s_ar_burst),
    .r_valid(m_axi_mm2s_r_valid),
    .r_ready(m_axi_mm2s_r_ready),
    .r_data(m_axi_mm2s_r_data),
    .r_id(m_axi_mm2s_r_id),
    .r_last(m_axi_mm2s_r_last),
    .push_valid(mm2s_push_valid_w),
    .push_ready(mm2s_push_ready_w),
    .push_data(mm2s_push_data_w)
  );
  // ── MM2S FIFO ───────────────────────────────────────────────────────
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
  // ── S2MM multi-outstanding write FSM ────────────────────────────────
  ThreadS2mm s2mm_fsm (
    .clk(s2mm_gated_clk_w),
    .rst(rst),
    .start(s2mm_fsm_start_w),
    .total_xfers(1),
    .base_addr(s2mm_fsm_addr_w),
    .burst_len(s2mm_fsm_beats_w),
    .done(s2mm_done_w),
    .halted(s2mm_halted_w),
    .idle_out(s2mm_idle_w),
    .aw_valid(m_axi_s2mm_aw_valid),
    .aw_ready(m_axi_s2mm_aw_ready),
    .aw_addr(m_axi_s2mm_aw_addr),
    .aw_id(m_axi_s2mm_aw_id),
    .aw_len(m_axi_s2mm_aw_len),
    .aw_size(m_axi_s2mm_aw_size),
    .aw_burst(m_axi_s2mm_aw_burst),
    .w_valid(m_axi_s2mm_w_valid),
    .w_ready(m_axi_s2mm_w_ready),
    .w_data(m_axi_s2mm_w_data),
    .w_strb(m_axi_s2mm_w_strb),
    .w_last(m_axi_s2mm_w_last),
    .b_valid(m_axi_s2mm_b_valid),
    .b_ready(m_axi_s2mm_b_ready),
    .b_id(m_axi_s2mm_b_id),
    .pop_valid(s2mm_pop_valid_w),
    .pop_ready(s2mm_pop_ready_w),
    .pop_data(s2mm_pop_data_w)
  );
  // ── S2MM FIFO ───────────────────────────────────────────────────────
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
  assign m_axis_mm2s_tlast = mm2s_pop_valid_w && mm2s_tlast_r;
  // tlast via lookahead register — set when current beat is the last
  assign s2mm_push_data_w = s_axis_s2mm_tdata;
  assign s2mm_push_valid_w = s_axis_s2mm_tvalid;
  assign s_axis_s2mm_tready = s2mm_push_ready_w;
  // ── Tie-off unused channels ──────────────────────────────────────────
  // MM2S is read-only: tie off write channels
  assign m_axi_mm2s_aw_valid = 1'b0;
  assign m_axi_mm2s_aw_addr = 0;
  assign m_axi_mm2s_aw_id = 0;
  assign m_axi_mm2s_aw_len = 0;
  assign m_axi_mm2s_aw_size = 0;
  assign m_axi_mm2s_aw_burst = 0;
  assign m_axi_mm2s_w_valid = 1'b0;
  assign m_axi_mm2s_w_data = 0;
  assign m_axi_mm2s_w_strb = 0;
  assign m_axi_mm2s_w_last = 1'b0;
  assign m_axi_mm2s_b_ready = 1'b0;
  // S2MM is write-only: tie off read channels
  assign m_axi_s2mm_ar_valid = 1'b0;
  assign m_axi_s2mm_ar_addr = 0;
  assign m_axi_s2mm_ar_id = 0;
  assign m_axi_s2mm_ar_len = 0;
  assign m_axi_s2mm_ar_size = 0;
  assign m_axi_s2mm_ar_burst = 0;
  assign m_axi_s2mm_r_ready = 1'b0;
  // ── Interrupts ──────────────────────────────────────────────────────
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
        mm2s_beats_r <= mm2s_fsm_beats_w;
        // tlast for single-beat transfer
        mm2s_tlast_r <= mm2s_fsm_beats_w == 1;
      end else if (mm2s_pop_valid_w && m_axis_mm2s_tready) begin
        mm2s_stream_ctr <= 8'(mm2s_stream_ctr + 1);
        // tlast lookahead: set when current beat IS the last
        mm2s_tlast_r <= 8'(mm2s_stream_ctr + 1) == mm2s_beats_r;
      end
      if (s2mm_fsm_start_w) begin
        s2mm_recv_ctr <= 0;
      end else if (s_axis_s2mm_tvalid && s2mm_push_ready_w) begin
        s2mm_recv_ctr <= 8'(s2mm_recv_ctr + 1);
      end
    end
  end

endmodule

