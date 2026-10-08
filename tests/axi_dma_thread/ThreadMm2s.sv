//! ---
//! spec_md: doc/axi_dma_case_study.md
//! tags: [dma, axi4, mm2s, multi_outstanding, thread]
//! refs:
//!   - "AXI4 spec ARM IHI 0022, §A3 Single Interface Requirements"
//!   - "Xilinx PG021 (LogiCORE IP AXI DMA) — MM2S read path"
//! ---
//!
//! Multi-outstanding AXI4 MM2S (read) engine — issues up to NUM_OUTSTANDING
//! parallel AR transactions and collects R beats per AXI ID, pushing data to
//! a downstream FIFO. The thread-based decomposition replaces what would be
//! a 4-state hand-written FSM in the FsmMm2sMulti.arch baseline.
/// Multi-outstanding AXI4 MM2S read engine.
///
/// Architecture: a single `ArIssuer` thread owns the AR channel and a
/// `generate_for` block spawns NUM_OUTSTANDING `RCollect_i` threads, one
/// per AXI ID. The single-issuer + per-ID-collector split is the central
/// design choice — see the inner doc on `ArIssuer` and on the `RCollect_i`
/// generate for the rationale.
///
/// Work assignment: transfer `k` is issued with `ar_id = k % NUM_OUTSTANDING`
/// and collected by `RCollect_{k % NUM_OUTSTANDING}`. The engine is "done"
/// when `Σ thread_complete[i] == total_xfers`.
module _ThreadMm2s_threads #(
  parameter int NUM_OUTSTANDING = 4,
  localparam [0:0] _t0_S0_wait_until = 0,
  localparam [0:0] _t0_S1_wait_until = 1,
  localparam [0:0] _t1_S0_wait_until = 0,
  localparam [0:0] _t1_S1_dispatch = 1,
  localparam [0:0] _t2_S0_wait_until = 0,
  localparam [0:0] _t2_S1_dispatch = 1,
  localparam [0:0] _t3_S0_wait_until = 0,
  localparam [0:0] _t3_S1_dispatch = 1,
  localparam [0:0] _t4_S0_wait_until = 0,
  localparam [0:0] _t4_S1_dispatch = 1
) (
  input logic clk,
  input logic rst,
  input logic active,
  input logic active_r,
  input logic ar_ready,
  input logic [31:0] base_addr,
  input logic [7:0] burst_len,
  input logic push_ready,
  input logic [1:0] r_id,
  input logic r_valid,
  input logic start,
  input logic [15:0] total_xfers,
  output logic [31:0] ar_addr,
  output logic [1:0] ar_burst,
  output logic [1:0] ar_id,
  output logic [7:0] ar_len,
  output logic [2:0] ar_size,
  output logic ar_valid,
  output logic push_valid,
  output logic r_ready,
  output logic [7:0] burst_len_r,
  output logic [31:0] next_ar_addr_r,
  output logic [3:0] [15:0] thread_complete,
  output logic [15:0] total_xfers_r,
  output logic [15:0] xfer_ctr_r
);

  logic [0:0] _t0_state;
  logic [0:0] _t1_state;
  logic [0:0] _t2_state;
  logic [0:0] _t3_state;
  logic [0:0] _t4_state;
  logic [7:0] _t1_loop_cnt_0;
  logic [7:0] _t2_loop_cnt_0;
  logic [7:0] _t3_loop_cnt_0;
  logic [7:0] _t4_loop_cnt_0;
  always_comb begin
    ar_addr = 0;
    ar_burst = 0;
    ar_id = 0;
    ar_len = 0;
    ar_size = 0;
    ar_valid = 0;
    push_valid = 0;
    r_ready = 0;
    // Control latches — owned by ArIssuer (reset via default when)
    // Run flag — owned by the run-control seq block below (single driver: a
    // reg must not be written by both a thread and a seq block).
    // AR issuer state: xfer_ctr_r counts issued ARs; next_ar_addr_r is current address.
    // Per-thread completion counts — each owned exclusively by RCollect_i
    // Run control — sets active on a start while idle (the same condition
    // as the threads' `default when` soft-reset arm), clears it when all
    // responses are received. The two are exclusive: all_done needs active_r.
    // ── AR issuer ─────────────────────────────────────────────────────────────
    //! Single thread drives all AR outputs — no resource lock, no 4-way
    //! mux. Address is maintained in `next_ar_addr_r` (increment-only, no
    //! multiply); ID rotates via `xfer_ctr_r[1:0]` so transfer `k` lands
    //! on AXI ID `k % 4`. The `default when start and not active_r` clause
    //! is the soft-reset arm — kicks off a new run synchronously without
    //! a separate idle state.
    ar_valid = 1'b0;
    ar_addr = 32'd0;
    ar_id = 2'd0;
    ar_len = 8'd0;
    ar_size = 3'd0;
    ar_burst = 2'd0;
    // ── R collectors ─────────────────────────────────────────────────────────
    //! Per-ID R-channel collector. Only drives 1-bit `r_ready` and
    //! `push_valid` (both `shared(or)` — collector `i` asserts only
    //! when `r_id == i`), so the OR-reduction in the merged comb block
    //! is a narrow mux. `push_data = r_data` unconditionally (all
    //! collectors source the same wire), which is why `push_data` is
    //! NOT `shared(or)` — see the comb block above for the
    //! unconditional drive.
    //!
    //! Backpressure: each collector waits for its `(thread_complete[i]
    //! << 2) + i < xfer_ctr_r` window to open before consuming a beat.
    //! This couples to the issuer's round-robin ID rotation.
    r_ready = 1'b0;
    push_valid = 1'b0;
    r_ready = 1'b0;
    push_valid = 1'b0;
    r_ready = 1'b0;
    push_valid = 1'b0;
    r_ready = 1'b0;
    push_valid = 1'b0;
    if (_t0_state == _t0_S0_wait_until && active && xfer_ctr_r < total_xfers_r) begin
      ar_valid = 1;
      ar_addr = next_ar_addr_r;
      ar_id = xfer_ctr_r[1:0];
      ar_len = 8'(burst_len_r - 1);
      ar_size = 3'd2;
      ar_burst = 2'd1;
    end
    if (_t0_state == _t0_S1_wait_until) begin
      ar_valid = 1;
      ar_addr = next_ar_addr_r;
      ar_id = xfer_ctr_r[1:0];
      ar_len = 8'(burst_len_r - 1);
      ar_size = 3'd2;
      ar_burst = 2'd1;
    end
    if (_t1_state == _t1_S1_dispatch) begin
      r_ready = r_ready | 1;
      push_valid = push_valid | (r_valid && r_id == 0);
    end
    if (_t2_state == _t2_S1_dispatch) begin
      r_ready = r_ready | 1;
      push_valid = push_valid | (r_valid && r_id == 1);
    end
    if (_t3_state == _t3_S1_dispatch) begin
      r_ready = r_ready | 1;
      push_valid = push_valid | (r_valid && r_id == 2);
    end
    if (_t4_state == _t4_S1_dispatch) begin
      r_ready = r_ready | 1;
      push_valid = push_valid | (r_valid && r_id == 3);
    end
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      _t0_state <= 0;
      _t1_loop_cnt_0 <= 0;
      _t1_state <= 0;
      _t2_loop_cnt_0 <= 0;
      _t2_state <= 0;
      _t3_loop_cnt_0 <= 0;
      _t3_state <= 0;
      _t4_loop_cnt_0 <= 0;
      _t4_state <= 0;
      burst_len_r <= 0;
      next_ar_addr_r <= 0;
      for (int __ri0 = 0; __ri0 < 4; __ri0++) begin
        thread_complete[__ri0] <= 0;
      end
      total_xfers_r <= 0;
      xfer_ctr_r <= 0;
    end else begin
      if (start && !active_r) begin
        total_xfers_r <= total_xfers;
        burst_len_r <= burst_len;
        xfer_ctr_r <= 0;
        next_ar_addr_r <= base_addr;
        _t0_state <= _t0_S0_wait_until;
      end else begin
        if (_t0_state == _t0_S0_wait_until) begin
          if (active && xfer_ctr_r < total_xfers_r) begin
            _t0_state <= _t0_S1_wait_until;
          end
          if (active && xfer_ctr_r < total_xfers_r) begin
            if (ar_ready) begin
              xfer_ctr_r <= 16'(xfer_ctr_r + 1);
            end
            if (ar_ready) begin
              next_ar_addr_r <= 32'(next_ar_addr_r + (32'($unsigned(burst_len_r)) << 2));
            end
            if (ar_ready) begin
              _t0_state <= _t0_S0_wait_until;
            end
          end
        end
        if (_t0_state == _t0_S1_wait_until) begin
          if (ar_ready) begin
            xfer_ctr_r <= 16'(xfer_ctr_r + 1);
          end
          if (ar_ready) begin
            next_ar_addr_r <= 32'(next_ar_addr_r + (32'($unsigned(burst_len_r)) << 2));
          end
          if (ar_ready) begin
            _t0_state <= _t0_S0_wait_until;
          end
        end
      end
      if (start && !active_r) begin
        thread_complete[0] <= 0;
        _t1_state <= _t1_S0_wait_until;
      end else begin
        if (_t1_state == _t1_S0_wait_until) begin
          _t1_loop_cnt_0 <= 0;
          if (active && (thread_complete[0] << 2) + 0 < xfer_ctr_r) begin
            _t1_state <= _t1_S1_dispatch;
          end
        end
        if (_t1_state == _t1_S1_dispatch) begin
          if (r_valid && r_id == 0 && push_ready) begin
            _t1_loop_cnt_0 <= 8'(_t1_loop_cnt_0 + 8'd1);
          end
          if (r_valid && r_id == 0 && push_ready && _t1_loop_cnt_0 >= 8'(burst_len_r - 1)) begin
            thread_complete[0] <= 16'(thread_complete[0] + 1);
          end
          if (r_valid && r_id == 0 && push_ready && _t1_loop_cnt_0 < 8'(burst_len_r - 1)) begin
            _t1_state <= _t1_S1_dispatch;
          end
          if (r_valid && r_id == 0 && push_ready && _t1_loop_cnt_0 >= 8'(burst_len_r - 1)) begin
            _t1_state <= _t1_S0_wait_until;
          end
        end
      end
      if (start && !active_r) begin
        thread_complete[1] <= 0;
        _t2_state <= _t2_S0_wait_until;
      end else begin
        if (_t2_state == _t2_S0_wait_until) begin
          _t2_loop_cnt_0 <= 0;
          if (active && (thread_complete[1] << 2) + 1 < xfer_ctr_r) begin
            _t2_state <= _t2_S1_dispatch;
          end
        end
        if (_t2_state == _t2_S1_dispatch) begin
          if (r_valid && r_id == 1 && push_ready) begin
            _t2_loop_cnt_0 <= 8'(_t2_loop_cnt_0 + 8'd1);
          end
          if (r_valid && r_id == 1 && push_ready && _t2_loop_cnt_0 >= 8'(burst_len_r - 1)) begin
            thread_complete[1] <= 16'(thread_complete[1] + 1);
          end
          if (r_valid && r_id == 1 && push_ready && _t2_loop_cnt_0 < 8'(burst_len_r - 1)) begin
            _t2_state <= _t2_S1_dispatch;
          end
          if (r_valid && r_id == 1 && push_ready && _t2_loop_cnt_0 >= 8'(burst_len_r - 1)) begin
            _t2_state <= _t2_S0_wait_until;
          end
        end
      end
      if (start && !active_r) begin
        thread_complete[2] <= 0;
        _t3_state <= _t3_S0_wait_until;
      end else begin
        if (_t3_state == _t3_S0_wait_until) begin
          _t3_loop_cnt_0 <= 0;
          if (active && (thread_complete[2] << 2) + 2 < xfer_ctr_r) begin
            _t3_state <= _t3_S1_dispatch;
          end
        end
        if (_t3_state == _t3_S1_dispatch) begin
          if (r_valid && r_id == 2 && push_ready) begin
            _t3_loop_cnt_0 <= 8'(_t3_loop_cnt_0 + 8'd1);
          end
          if (r_valid && r_id == 2 && push_ready && _t3_loop_cnt_0 >= 8'(burst_len_r - 1)) begin
            thread_complete[2] <= 16'(thread_complete[2] + 1);
          end
          if (r_valid && r_id == 2 && push_ready && _t3_loop_cnt_0 < 8'(burst_len_r - 1)) begin
            _t3_state <= _t3_S1_dispatch;
          end
          if (r_valid && r_id == 2 && push_ready && _t3_loop_cnt_0 >= 8'(burst_len_r - 1)) begin
            _t3_state <= _t3_S0_wait_until;
          end
        end
      end
      if (start && !active_r) begin
        thread_complete[3] <= 0;
        _t4_state <= _t4_S0_wait_until;
      end else begin
        if (_t4_state == _t4_S0_wait_until) begin
          _t4_loop_cnt_0 <= 0;
          if (active && (thread_complete[3] << 2) + 3 < xfer_ctr_r) begin
            _t4_state <= _t4_S1_dispatch;
          end
        end
        if (_t4_state == _t4_S1_dispatch) begin
          if (r_valid && r_id == 3 && push_ready) begin
            _t4_loop_cnt_0 <= 8'(_t4_loop_cnt_0 + 8'd1);
          end
          if (r_valid && r_id == 3 && push_ready && _t4_loop_cnt_0 >= 8'(burst_len_r - 1)) begin
            thread_complete[3] <= 16'(thread_complete[3] + 1);
          end
          if (r_valid && r_id == 3 && push_ready && _t4_loop_cnt_0 < 8'(burst_len_r - 1)) begin
            _t4_state <= _t4_S1_dispatch;
          end
          if (r_valid && r_id == 3 && push_ready && _t4_loop_cnt_0 >= 8'(burst_len_r - 1)) begin
            _t4_state <= _t4_S0_wait_until;
          end
        end
      end
    end
  end

endmodule
module ThreadMm2s #(
  parameter int NUM_OUTSTANDING = 4
) (
  input logic clk,
  input logic rst,
  input logic start,
  input logic [15:0] total_xfers,
  input logic [31:0] base_addr,
  input logic [7:0] burst_len,
  output logic done,
  output logic halted,
  output logic idle_out,
  output logic ar_valid,
  input logic ar_ready,
  output logic [31:0] ar_addr,
  output logic [1:0] ar_id,
  output logic [7:0] ar_len,
  output logic [2:0] ar_size,
  output logic [1:0] ar_burst,
  input logic r_valid,
  output logic r_ready,
  input logic [31:0] r_data,
  input logic [1:0] r_id,
  input logic r_last,
  output logic push_valid,
  input logic push_ready,
  output logic [31:0] push_data
);

  logic [15:0] total_complete;
  logic all_done;
  logic active;
  logic [15:0] total_xfers_r;
  logic [7:0] burst_len_r;
  logic active_r;
  logic [15:0] xfer_ctr_r;
  logic [31:0] next_ar_addr_r;
  logic [3:0] [15:0] thread_complete;
  assign total_complete = ((($bits(thread_complete[0]) > $bits(thread_complete[1]) ? $bits(thread_complete[0]) : $bits(thread_complete[1])) > $bits(thread_complete[2]) ? ($bits(thread_complete[0]) > $bits(thread_complete[1]) ? $bits(thread_complete[0]) : $bits(thread_complete[1])) : $bits(thread_complete[2])) > $bits(thread_complete[3]) ? (($bits(thread_complete[0]) > $bits(thread_complete[1]) ? $bits(thread_complete[0]) : $bits(thread_complete[1])) > $bits(thread_complete[2]) ? ($bits(thread_complete[0]) > $bits(thread_complete[1]) ? $bits(thread_complete[0]) : $bits(thread_complete[1])) : $bits(thread_complete[2])) : $bits(thread_complete[3]))'(((($bits(thread_complete[0]) > $bits(thread_complete[1]) ? $bits(thread_complete[0]) : $bits(thread_complete[1])) > $bits(thread_complete[2]) ? ($bits(thread_complete[0]) > $bits(thread_complete[1]) ? $bits(thread_complete[0]) : $bits(thread_complete[1])) : $bits(thread_complete[2]))'((($bits(thread_complete[0]) > $bits(thread_complete[1]) ? $bits(thread_complete[0]) : $bits(thread_complete[1]))'(thread_complete[0] + thread_complete[1])) + thread_complete[2])) + thread_complete[3]);
  assign all_done = active_r && total_xfers_r != 0 && total_complete == total_xfers_r;
  assign active = active_r || start && !active_r;
  assign halted = 1'b0;
  assign idle_out = !active;
  assign done = all_done;
  assign push_data = r_data;
  always_ff @(posedge clk) begin
    if (rst) begin
      active_r <= 1'b0;
    end else begin
      if (start && !active_r) begin
        active_r <= 1'b1;
      end else if (all_done) begin
        active_r <= 1'b0;
      end
    end
  end
  _ThreadMm2s_threads #(.NUM_OUTSTANDING(NUM_OUTSTANDING)) _threads (
    .clk(clk),
    .rst(rst),
    .active(active),
    .active_r(active_r),
    .ar_ready(ar_ready),
    .base_addr(base_addr),
    .burst_len(burst_len),
    .push_ready(push_ready),
    .r_id(r_id),
    .r_valid(r_valid),
    .start(start),
    .total_xfers(total_xfers),
    .ar_addr(ar_addr),
    .ar_burst(ar_burst),
    .ar_id(ar_id),
    .ar_len(ar_len),
    .ar_size(ar_size),
    .ar_valid(ar_valid),
    .push_valid(push_valid),
    .r_ready(r_ready),
    .burst_len_r(burst_len_r),
    .next_ar_addr_r(next_ar_addr_r),
    .thread_complete(thread_complete),
    .total_xfers_r(total_xfers_r),
    .xfer_ctr_r(xfer_ctr_r)
  );

endmodule
