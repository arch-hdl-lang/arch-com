// Thread-based multi-outstanding MM2S read engine.
//
// Architecture: single ArIssuer thread + 4 RCollect threads.
//   - ArIssuer: single state machine drives all AR outputs — no 4-way mux needed.
//     Address maintained in next_ar_addr_r (increment-only, no multiply).
//   - RCollect_i: each thread collects R beats for its assigned ID.
//     Only drives 1-bit r_ready/push_valid — narrow mux overhead.
//   - push_data = r_data unconditionally (all threads push same source).
//
// Work assignment: xfer k → AR ID k%4, handled by RCollect_{k%4}.
// Done when sum of thread_complete[i] == total_xfers.
module _ThreadMm2s_threads (
  input logic clk,
  input logic rst,
  input logic active,
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
  output logic active_r,
  output logic [7:0] burst_len_r,
  output logic [31:0] next_ar_addr_r,
  output logic [3:0] [15:0] thread_complete,
  output logic [15:0] total_xfers_r,
  output logic [15:0] xfer_ctr_r
);

  always_comb begin
    ar_addr = 0;
    ar_burst = 0;
    ar_id = 0;
    ar_len = 0;
    ar_size = 0;
    ar_valid = 0;
    push_valid = 0;
    r_ready = 0;
    if (_t0_state == 1) begin
      // Control latches — owned by ArIssuer (reset via default when)
      // AR issuer state: xfer_ctr_r counts issued ARs; next_ar_addr_r is current address.
      // Per-thread completion counts — each owned exclusively by RCollect_i
      // Completion handler — clears active when all responses received
      // ── AR issuer ─────────────────────────────────────────────────────────────
      // Single thread drives all AR outputs — no resource lock, no 4-way mux.
      ar_valid = 1;
      ar_addr = next_ar_addr_r;
      ar_id = xfer_ctr_r[1:0];
      ar_len = 8'(burst_len_r - 1);
      ar_size = 3'd2;
      ar_burst = 2'd1;
    end
    if (_t1_state == 1) begin
      // ── R collectors ─────────────────────────────────────────────────────────
      r_ready = r_ready | 1;
      push_valid = push_valid | (r_valid && r_id == 0);
    end
    if (_t2_state == 1) begin
      r_ready = r_ready | 1;
      push_valid = push_valid | (r_valid && r_id == 1);
    end
    if (_t3_state == 1) begin
      r_ready = r_ready | 1;
      push_valid = push_valid | (r_valid && r_id == 2);
    end
    if (_t4_state == 1) begin
      r_ready = r_ready | 1;
      push_valid = push_valid | (r_valid && r_id == 3);
    end
  end
  logic [0:0] _t0_state = 0;
  logic [0:0] _t1_state = 0;
  logic [0:0] _t2_state = 0;
  logic [0:0] _t3_state = 0;
  logic [0:0] _t4_state = 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      _t0_state <= 0;
      _t1_state <= 0;
      _t2_state <= 0;
      _t3_state <= 0;
      _t4_state <= 0;
      active_r <= 1'b0;
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
        active_r <= 1'b1;
        xfer_ctr_r <= 0;
        next_ar_addr_r <= base_addr;
        _t0_state <= 0;
      end else begin
        if (_t0_state == 0) begin
          if (active && xfer_ctr_r < total_xfers_r) begin
            _t0_state <= 1;
          end
        end
        if (_t0_state == 1) begin
          if (ar_ready) begin
            xfer_ctr_r <= 16'(xfer_ctr_r + 1);
          end
          if (ar_ready) begin
            next_ar_addr_r <= 32'(next_ar_addr_r + (32'($unsigned(burst_len_r)) << 2));
          end
          if (ar_ready) begin
            _t0_state <= 0;
          end
        end
      end
      if (start && !active_r) begin
        thread_complete[0] <= 0;
        _t1_state <= 0;
      end else begin
        if (_t1_state == 0) begin
          _t1_loop_cnt <= 0;
          if (active && (thread_complete[0] << 2) + 0 < xfer_ctr_r) begin
            _t1_state <= 1;
          end
        end
        if (_t1_state == 1) begin
          if (r_valid && r_id == 0 && push_ready) begin
            _t1_loop_cnt <= 8'(_t1_loop_cnt + 8'd1);
          end
          if (r_valid && r_id == 0 && push_ready && _t1_loop_cnt >= 8'(burst_len_r - 1)) begin
            thread_complete[0] <= 16'(thread_complete[0] + 1);
          end
          if (r_valid && r_id == 0 && push_ready && _t1_loop_cnt < 8'(burst_len_r - 1)) begin
            _t1_state <= 1;
          end
          if (r_valid && r_id == 0 && push_ready && _t1_loop_cnt >= 8'(burst_len_r - 1)) begin
            _t1_state <= 0;
          end
        end
      end
      if (start && !active_r) begin
        thread_complete[1] <= 0;
        _t2_state <= 0;
      end else begin
        if (_t2_state == 0) begin
          _t2_loop_cnt <= 0;
          if (active && (thread_complete[1] << 2) + 1 < xfer_ctr_r) begin
            _t2_state <= 1;
          end
        end
        if (_t2_state == 1) begin
          if (r_valid && r_id == 1 && push_ready) begin
            _t2_loop_cnt <= 8'(_t2_loop_cnt + 8'd1);
          end
          if (r_valid && r_id == 1 && push_ready && _t2_loop_cnt >= 8'(burst_len_r - 1)) begin
            thread_complete[1] <= 16'(thread_complete[1] + 1);
          end
          if (r_valid && r_id == 1 && push_ready && _t2_loop_cnt < 8'(burst_len_r - 1)) begin
            _t2_state <= 1;
          end
          if (r_valid && r_id == 1 && push_ready && _t2_loop_cnt >= 8'(burst_len_r - 1)) begin
            _t2_state <= 0;
          end
        end
      end
      if (start && !active_r) begin
        thread_complete[2] <= 0;
        _t3_state <= 0;
      end else begin
        if (_t3_state == 0) begin
          _t3_loop_cnt <= 0;
          if (active && (thread_complete[2] << 2) + 2 < xfer_ctr_r) begin
            _t3_state <= 1;
          end
        end
        if (_t3_state == 1) begin
          if (r_valid && r_id == 2 && push_ready) begin
            _t3_loop_cnt <= 8'(_t3_loop_cnt + 8'd1);
          end
          if (r_valid && r_id == 2 && push_ready && _t3_loop_cnt >= 8'(burst_len_r - 1)) begin
            thread_complete[2] <= 16'(thread_complete[2] + 1);
          end
          if (r_valid && r_id == 2 && push_ready && _t3_loop_cnt < 8'(burst_len_r - 1)) begin
            _t3_state <= 1;
          end
          if (r_valid && r_id == 2 && push_ready && _t3_loop_cnt >= 8'(burst_len_r - 1)) begin
            _t3_state <= 0;
          end
        end
      end
      if (start && !active_r) begin
        thread_complete[3] <= 0;
        _t4_state <= 0;
      end else begin
        if (_t4_state == 0) begin
          _t4_loop_cnt <= 0;
          if (active && (thread_complete[3] << 2) + 3 < xfer_ctr_r) begin
            _t4_state <= 1;
          end
        end
        if (_t4_state == 1) begin
          if (r_valid && r_id == 3 && push_ready) begin
            _t4_loop_cnt <= 8'(_t4_loop_cnt + 8'd1);
          end
          if (r_valid && r_id == 3 && push_ready && _t4_loop_cnt >= 8'(burst_len_r - 1)) begin
            thread_complete[3] <= 16'(thread_complete[3] + 1);
          end
          if (r_valid && r_id == 3 && push_ready && _t4_loop_cnt < 8'(burst_len_r - 1)) begin
            _t4_state <= 1;
          end
          if (r_valid && r_id == 3 && push_ready && _t4_loop_cnt >= 8'(burst_len_r - 1)) begin
            _t4_state <= 0;
          end
        end
      end
    end
  end
  logic [7:0] _t1_loop_cnt = 0;
  logic [7:0] _t2_loop_cnt = 0;
  logic [7:0] _t3_loop_cnt = 0;
  logic [7:0] _t4_loop_cnt = 0;

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

  logic [16:0] tc01;
  assign tc01 = thread_complete[0] + thread_complete[1];
  logic [16:0] tc23;
  assign tc23 = thread_complete[2] + thread_complete[3];
  logic [17:0] total_complete;
  assign total_complete = tc01 + tc23;
  logic all_done;
  assign all_done = active_r && total_xfers_r != 0 && total_complete == 18'($unsigned(total_xfers_r));
  logic active;
  assign active = active_r || start && !active_r;
  assign halted = 1'b0;
  assign idle_out = !active;
  assign done = all_done;
  assign push_data = r_data;
  always_ff @(posedge clk) begin
    if (all_done) begin
      active_r <= 1'b0;
    end
  end
  logic active_r;
  logic [7:0] burst_len_r;
  logic [31:0] next_ar_addr_r;
  logic [3:0] [15:0] thread_complete;
  logic [15:0] total_xfers_r;
  logic [15:0] xfer_ctr_r;
  _ThreadMm2s_threads _threads (
    .clk(clk),
    .rst(rst),
    .active(active),
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
    .active_r(active_r),
    .burst_len_r(burst_len_r),
    .next_ar_addr_r(next_ar_addr_r),
    .thread_complete(thread_complete),
    .total_xfers_r(total_xfers_r),
    .xfer_ctr_r(xfer_ctr_r)
  );

endmodule

