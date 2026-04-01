// PG021-compatible AXI DMA — top-level integration (Simple DMA mode).
// Two independent channels: MM2S (mem→stream) and S2MM (stream→mem).
module AxiDmaTop (
  input logic clk,
  input logic rst,
  input logic [8-1:0] s_awaddr,
  input logic s_awvalid,
  output logic s_awready,
  input logic [32-1:0] s_wdata,
  input logic [4-1:0] s_wstrb,
  input logic s_wvalid,
  output logic s_wready,
  output logic [2-1:0] s_bresp,
  output logic s_bvalid,
  input logic s_bready,
  input logic [8-1:0] s_araddr,
  input logic s_arvalid,
  output logic s_arready,
  output logic [32-1:0] s_rdata,
  output logic [2-1:0] s_rresp,
  output logic s_rvalid,
  input logic s_rready,
  output logic mm2s_ar_valid,
  input logic mm2s_ar_ready,
  output logic [32-1:0] mm2s_ar_addr,
  output logic [8-1:0] mm2s_ar_len,
  output logic [3-1:0] mm2s_ar_size,
  output logic [2-1:0] mm2s_ar_burst,
  input logic mm2s_r_valid,
  output logic mm2s_r_ready,
  input logic [32-1:0] mm2s_r_data,
  input logic mm2s_r_last,
  output logic s2mm_aw_valid,
  input logic s2mm_aw_ready,
  output logic [32-1:0] s2mm_aw_addr,
  output logic [8-1:0] s2mm_aw_len,
  output logic [3-1:0] s2mm_aw_size,
  output logic [2-1:0] s2mm_aw_burst,
  output logic s2mm_w_valid,
  input logic s2mm_w_ready,
  output logic [32-1:0] s2mm_w_data,
  output logic [4-1:0] s2mm_w_strb,
  output logic s2mm_w_last,
  input logic s2mm_b_valid,
  output logic s2mm_b_ready,
  output logic m_axis_tvalid,
  input logic m_axis_tready,
  output logic [32-1:0] m_axis_tdata,
  output logic m_axis_tlast,
  output logic [4-1:0] m_axis_tkeep,
  input logic s_axis_tvalid,
  output logic s_axis_tready,
  input logic [32-1:0] s_axis_tdata,
  input logic s_axis_tlast,
  output logic mm2s_introut,
  output logic s2mm_introut
);

  // ── AXI4-Lite slave (register access) ───────────────────────────────
  // ── AXI4 Read Master (MM2S memory reads) ────────────────────────────
  // ── AXI4 Write Master (S2MM memory writes) ──────────────────────────
  // ── AXI4-Stream Master (MM2S output) ────────────────────────────────
  // ── AXI4-Stream Slave (S2MM input) ──────────────────────────────────
  // ── Interrupts ──────────────────────────────────────────────────────
  // ── Internal wires ──────────────────────────────────────────────────
  // Register block ↔ FSMs
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
  logic mm2s_introut_w;
  logic s2mm_introut_w;
  // Register block AXI-Lite outputs
  logic s_awready_w;
  logic s_wready_w;
  logic [2-1:0] s_bresp_w;
  logic s_bvalid_w;
  logic s_arready_w;
  logic [32-1:0] s_rdata_w;
  logic [2-1:0] s_rresp_w;
  logic s_rvalid_w;
  // MM2S FIFO wires
  logic mm2s_push_valid_w;
  logic mm2s_push_ready_w;
  logic [32-1:0] mm2s_push_data_w;
  logic mm2s_pop_valid_w;
  logic mm2s_pop_ready_w;
  logic [32-1:0] mm2s_pop_data_w;
  // S2MM FIFO wires
  logic s2mm_push_valid_w;
  logic s2mm_push_ready_w;
  logic [32-1:0] s2mm_push_data_w;
  logic s2mm_pop_valid_w;
  logic s2mm_pop_ready_w;
  logic [32-1:0] s2mm_pop_data_w;
  // MM2S AXI4 read wires
  logic mm2s_ar_valid_w;
  logic [32-1:0] mm2s_ar_addr_w;
  logic [8-1:0] mm2s_ar_len_w;
  logic [3-1:0] mm2s_ar_size_w;
  logic [2-1:0] mm2s_ar_burst_w;
  logic mm2s_r_ready_w;
  // S2MM AXI4 write wires
  logic s2mm_aw_valid_w;
  logic [32-1:0] s2mm_aw_addr_w;
  logic [8-1:0] s2mm_aw_len_w;
  logic [3-1:0] s2mm_aw_size_w;
  logic [2-1:0] s2mm_aw_burst_w;
  logic s2mm_w_valid_w;
  logic [32-1:0] s2mm_w_data_w;
  logic [4-1:0] s2mm_w_strb_w;
  logic s2mm_w_last_w;
  logic s2mm_b_ready_w;
  // Stream beat counters
  logic [8-1:0] mm2s_stream_ctr;
  logic [8-1:0] s2mm_recv_ctr;
  // ── Instances ───────────────────────────────────────────────────────
  AxiLiteRegs regs (
    .clk(clk),
    .rst(rst),
    .awaddr_i(s_awaddr),
    .awvalid_i(s_awvalid),
    .awready_o(s_awready_w),
    .wdata_i(s_wdata),
    .wstrb_i(s_wstrb),
    .wvalid_i(s_wvalid),
    .wready_o(s_wready_w),
    .bresp_o(s_bresp_w),
    .bvalid_o(s_bvalid_w),
    .bready_i(s_bready),
    .araddr_i(s_araddr),
    .arvalid_i(s_arvalid),
    .arready_o(s_arready_w),
    .rdata_o(s_rdata_w),
    .rresp_o(s_rresp_w),
    .rvalid_o(s_rvalid_w),
    .rready_i(s_rready),
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
    .mm2s_introut(mm2s_introut_w),
    .s2mm_introut(s2mm_introut_w)
  );
  FsmMm2s mm2s_fsm (
    .clk(clk),
    .rst(rst),
    .start(mm2s_start_w),
    .src_addr(mm2s_src_addr_w),
    .num_beats(mm2s_num_beats_w),
    .done(mm2s_done_w),
    .halted(mm2s_halted_w),
    .idle_out(mm2s_idle_w),
    .ar_valid(mm2s_ar_valid_w),
    .ar_ready(mm2s_ar_ready),
    .ar_addr(mm2s_ar_addr_w),
    .ar_len(mm2s_ar_len_w),
    .ar_size(mm2s_ar_size_w),
    .ar_burst(mm2s_ar_burst_w),
    .r_valid(mm2s_r_valid),
    .r_ready(mm2s_r_ready_w),
    .r_data(mm2s_r_data),
    .r_last(mm2s_r_last),
    .push_valid(mm2s_push_valid_w),
    .push_ready(mm2s_push_ready_w),
    .push_data(mm2s_push_data_w)
  );
  Mm2sFifo mm2s_fifo (
    .clk(clk),
    .rst(rst),
    .push_valid(mm2s_push_valid_w),
    .push_ready(mm2s_push_ready_w),
    .push_data(mm2s_push_data_w),
    .pop_valid(mm2s_pop_valid_w),
    .pop_ready(mm2s_pop_ready_w),
    .pop_data(mm2s_pop_data_w)
  );
  S2mmFifo s2mm_fifo (
    .clk(clk),
    .rst(rst),
    .push_valid(s2mm_push_valid_w),
    .push_ready(s2mm_push_ready_w),
    .push_data(s2mm_push_data_w),
    .pop_valid(s2mm_pop_valid_w),
    .pop_ready(s2mm_pop_ready_w),
    .pop_data(s2mm_pop_data_w)
  );
  FsmS2mm s2mm_fsm (
    .clk(clk),
    .rst(rst),
    .start(s2mm_start_w),
    .dst_addr(s2mm_dst_addr_w),
    .num_beats(s2mm_num_beats_w),
    .recv_count(s2mm_recv_ctr),
    .done(s2mm_done_w),
    .halted(s2mm_halted_w),
    .idle_out(s2mm_idle_w),
    .pop_valid(s2mm_pop_valid_w),
    .pop_ready(s2mm_pop_ready_w),
    .pop_data(s2mm_pop_data_w),
    .aw_valid(s2mm_aw_valid_w),
    .aw_ready(s2mm_aw_ready),
    .aw_addr(s2mm_aw_addr_w),
    .aw_len(s2mm_aw_len_w),
    .aw_size(s2mm_aw_size_w),
    .aw_burst(s2mm_aw_burst_w),
    .w_valid(s2mm_w_valid_w),
    .w_ready(s2mm_w_ready),
    .w_data(s2mm_w_data_w),
    .w_strb(s2mm_w_strb_w),
    .w_last(s2mm_w_last_w),
    .b_valid(s2mm_b_valid),
    .b_ready(s2mm_b_ready_w)
  );
  // ── Stream wiring + beat counters ───────────────────────────────────
  // MM2S: FIFO pop → AXI4-Stream output
  assign m_axis_tdata = mm2s_pop_data_w;
  assign m_axis_tvalid = mm2s_pop_valid_w;
  assign m_axis_tkeep = 'hF;
  assign mm2s_pop_ready_w = m_axis_tready;
  assign m_axis_tlast = mm2s_pop_valid_w & mm2s_stream_ctr == 8'(mm2s_num_beats_w - 1);
  // S2MM: AXI4-Stream input → FIFO push
  assign s2mm_push_data_w = s_axis_tdata;
  assign s2mm_push_valid_w = s_axis_tvalid;
  assign s_axis_tready = s2mm_push_ready_w;
  // Route register block AXI-Lite outputs to top ports
  assign s_awready = s_awready_w;
  assign s_wready = s_wready_w;
  assign s_bresp = s_bresp_w;
  assign s_bvalid = s_bvalid_w;
  assign s_arready = s_arready_w;
  assign s_rdata = s_rdata_w;
  assign s_rresp = s_rresp_w;
  assign s_rvalid = s_rvalid_w;
  // Route AXI4 read master (MM2S)
  assign mm2s_ar_valid = mm2s_ar_valid_w;
  assign mm2s_ar_addr = mm2s_ar_addr_w;
  assign mm2s_ar_len = mm2s_ar_len_w;
  assign mm2s_ar_size = mm2s_ar_size_w;
  assign mm2s_ar_burst = mm2s_ar_burst_w;
  assign mm2s_r_ready = mm2s_r_ready_w;
  // Route AXI4 write master (S2MM)
  assign s2mm_aw_valid = s2mm_aw_valid_w;
  assign s2mm_aw_addr = s2mm_aw_addr_w;
  assign s2mm_aw_len = s2mm_aw_len_w;
  assign s2mm_aw_size = s2mm_aw_size_w;
  assign s2mm_aw_burst = s2mm_aw_burst_w;
  assign s2mm_w_valid = s2mm_w_valid_w;
  assign s2mm_w_data = s2mm_w_data_w;
  assign s2mm_w_strb = s2mm_w_strb_w;
  assign s2mm_w_last = s2mm_w_last_w;
  assign s2mm_b_ready = s2mm_b_ready_w;
  // Route interrupts
  assign mm2s_introut = mm2s_introut_w;
  assign s2mm_introut = s2mm_introut_w;
  // ── Beat counters (registered) ──────────────────────────────────────
  always_ff @(posedge clk) begin
    if (rst) begin
      mm2s_stream_ctr <= 0;
      s2mm_recv_ctr <= 0;
    end else begin
      // MM2S stream output beat counter — counts pops for tlast generation
      if (mm2s_start_w) begin
        mm2s_stream_ctr <= 0;
      end else if (mm2s_pop_valid_w & m_axis_tready) begin
        mm2s_stream_ctr <= 8'(mm2s_stream_ctr + 1);
      end
      // S2MM stream input beat counter — counts received beats
      if (s2mm_start_w) begin
        s2mm_recv_ctr <= 0;
      end else if (s_axis_tvalid & s2mm_push_ready_w) begin
        s2mm_recv_ctr <= 8'(s2mm_recv_ctr + 1);
      end
    end
  end

endmodule

