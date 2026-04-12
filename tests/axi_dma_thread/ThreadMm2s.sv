// Thread-based multi-outstanding MM2S read engine.
// Uses generate_for to unroll N identical read threads.
//
// Work assignment: thread i owns xfer indices i, i+4, i+8, ...
//   (static round-robin — no shared counter, no race)
// Each thread loops, driven by thread_complete[i] (its own completion count).
// Done when sum of all thread_complete[i] == total_xfers.
module _ThreadMm2s_threads (
  input logic clk,
  input logic rst,
  input logic active,
  input logic ar_ready,
  input logic [31:0] base_addr_r,
  input logic [7:0] burst_len_r,
  input logic push_ready,
  input logic [31:0] r_data,
  input logic [1:0] r_id,
  input logic r_valid,
  input logic [15:0] total_xfers_r,
  output logic [31:0] ar_addr,
  output logic [1:0] ar_burst,
  output logic [1:0] ar_id,
  output logic [7:0] ar_len,
  output logic [2:0] ar_size,
  output logic ar_valid,
  output logic [31:0] push_data,
  output logic push_valid,
  output logic r_ready,
  output logic [3:0] [15:0] thread_complete
);

  always_comb begin
    ar_addr = 0;
    ar_burst = 0;
    ar_id = 0;
    ar_len = 0;
    ar_size = 0;
    ar_valid = 0;
    push_data = 0;
    push_valid = 0;
    r_ready = 0;
    _ar_ch_req_0 = 1'b0;
    _ar_ch_req_1 = 1'b0;
    _ar_ch_req_2 = 1'b0;
    _ar_ch_req_3 = 1'b0;
    if (_t0_state == 1) begin
      // Control latches
      // Per-thread completion counts: thread_complete[i] = rounds completed by thread i
      // Written only by thread i → no multi-driver conflict
      // Total completions across all threads
      // Done when all xfers have completed
      // Combinational active includes start pulse — zero startup latency
      // Controller: latch config on start, clear on completion
      // Read threads — each handles xfer indices i, i+4, i+8, ...
      // Thread i waits until its next xfer index < total_xfers_r, then issues AR.
      // Next xfer for this thread: i + thread_complete[i] * 4
      // Wait condition: this xfer index must be within range
      // Serialise AR channel access
      _ar_ch_req_0 = 1;
      if (_ar_ch_grant_0) begin
        ar_valid = 1;
        ar_addr = 32'(base_addr_r + ((32'($unsigned(thread_complete[0])) << 2) + 0) * (32'($unsigned(burst_len_r)) << 2));
        ar_id = 0;
        ar_len = 8'(burst_len_r - 1);
        ar_size = 3'd2;
        ar_burst = 2'd1;
      end
    end
    if (_t0_state == 2) begin
      // Collect R beats for this thread's ID
      r_ready = r_ready | r_id == 0;
      push_valid = push_valid | (r_valid && r_id == 0);
      push_data = r_data;
    end
    if (_t1_state == 1) begin
      _ar_ch_req_1 = 1;
      if (_ar_ch_grant_1) begin
        ar_valid = 1;
        ar_addr = 32'(base_addr_r + ((32'($unsigned(thread_complete[1])) << 2) + 1) * (32'($unsigned(burst_len_r)) << 2));
        ar_id = 1;
        ar_len = 8'(burst_len_r - 1);
        ar_size = 3'd2;
        ar_burst = 2'd1;
      end
    end
    if (_t1_state == 2) begin
      r_ready = r_ready | r_id == 1;
      push_valid = push_valid | (r_valid && r_id == 1);
      push_data = r_data;
    end
    if (_t2_state == 1) begin
      _ar_ch_req_2 = 1;
      if (_ar_ch_grant_2) begin
        ar_valid = 1;
        ar_addr = 32'(base_addr_r + ((32'($unsigned(thread_complete[2])) << 2) + 2) * (32'($unsigned(burst_len_r)) << 2));
        ar_id = 2;
        ar_len = 8'(burst_len_r - 1);
        ar_size = 3'd2;
        ar_burst = 2'd1;
      end
    end
    if (_t2_state == 2) begin
      r_ready = r_ready | r_id == 2;
      push_valid = push_valid | (r_valid && r_id == 2);
      push_data = r_data;
    end
    if (_t3_state == 1) begin
      _ar_ch_req_3 = 1;
      if (_ar_ch_grant_3) begin
        ar_valid = 1;
        ar_addr = 32'(base_addr_r + ((32'($unsigned(thread_complete[3])) << 2) + 3) * (32'($unsigned(burst_len_r)) << 2));
        ar_id = 3;
        ar_len = 8'(burst_len_r - 1);
        ar_size = 3'd2;
        ar_burst = 2'd1;
      end
    end
    if (_t3_state == 2) begin
      r_ready = r_ready | r_id == 3;
      push_valid = push_valid | (r_valid && r_id == 3);
      push_data = r_data;
    end
    // Increment per-thread counter (merged into for-loop exit — no dead cycle)
  end
  logic _ar_ch_req_0;
  logic _ar_ch_grant_0;
  logic _ar_ch_req_1;
  logic _ar_ch_grant_1;
  logic _ar_ch_req_2;
  logic _ar_ch_grant_2;
  logic _ar_ch_req_3;
  logic _ar_ch_grant_3;
  assign _ar_ch_grant_0 = _ar_ch_req_0;
  assign _ar_ch_grant_1 = _ar_ch_req_1 && !_ar_ch_grant_0;
  assign _ar_ch_grant_2 = _ar_ch_req_2 && !_ar_ch_grant_0 && !_ar_ch_grant_1;
  assign _ar_ch_grant_3 = _ar_ch_req_3 && !_ar_ch_grant_0 && !_ar_ch_grant_1 && !_ar_ch_grant_2;
  logic [1:0] _t0_state = 0;
  logic [1:0] _t1_state = 0;
  logic [1:0] _t2_state = 0;
  logic [1:0] _t3_state = 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      _t0_state <= 0;
      _t1_state <= 0;
      _t2_state <= 0;
      _t3_state <= 0;
      for (int __ri0 = 0; __ri0 < 4; __ri0++) begin
        thread_complete[__ri0] <= 0;
      end
    end else begin
      if (_t0_state == 0) begin
        if (active && (thread_complete[0] << 2) + 0 < total_xfers_r) begin
          _t0_state <= 1;
        end
      end
      if (_t0_state == 1) begin
        _t0_loop_cnt <= 0;
        if (_ar_ch_grant_0 && ar_ready) begin
          _t0_state <= 2;
        end
      end
      if (_t0_state == 2) begin
        if (r_valid && r_id == 0 && push_ready) begin
          _t0_loop_cnt <= 8'(_t0_loop_cnt + 8'd1);
        end
        if (r_valid && r_id == 0 && push_ready && _t0_loop_cnt >= 8'(burst_len_r - 1)) begin
          thread_complete[0] <= 16'(thread_complete[0] + 1);
        end
        if (r_valid && r_id == 0 && push_ready && _t0_loop_cnt < 8'(burst_len_r - 1)) begin
          _t0_state <= 2;
        end
        if (r_valid && r_id == 0 && push_ready && _t0_loop_cnt >= 8'(burst_len_r - 1)) begin
          _t0_state <= 0;
        end
      end
      if (_t1_state == 0) begin
        if (active && (thread_complete[1] << 2) + 1 < total_xfers_r) begin
          _t1_state <= 1;
        end
      end
      if (_t1_state == 1) begin
        _t1_loop_cnt <= 0;
        if (_ar_ch_grant_1 && ar_ready) begin
          _t1_state <= 2;
        end
      end
      if (_t1_state == 2) begin
        if (r_valid && r_id == 1 && push_ready) begin
          _t1_loop_cnt <= 8'(_t1_loop_cnt + 8'd1);
        end
        if (r_valid && r_id == 1 && push_ready && _t1_loop_cnt >= 8'(burst_len_r - 1)) begin
          thread_complete[1] <= 16'(thread_complete[1] + 1);
        end
        if (r_valid && r_id == 1 && push_ready && _t1_loop_cnt < 8'(burst_len_r - 1)) begin
          _t1_state <= 2;
        end
        if (r_valid && r_id == 1 && push_ready && _t1_loop_cnt >= 8'(burst_len_r - 1)) begin
          _t1_state <= 0;
        end
      end
      if (_t2_state == 0) begin
        if (active && (thread_complete[2] << 2) + 2 < total_xfers_r) begin
          _t2_state <= 1;
        end
      end
      if (_t2_state == 1) begin
        _t2_loop_cnt <= 0;
        if (_ar_ch_grant_2 && ar_ready) begin
          _t2_state <= 2;
        end
      end
      if (_t2_state == 2) begin
        if (r_valid && r_id == 2 && push_ready) begin
          _t2_loop_cnt <= 8'(_t2_loop_cnt + 8'd1);
        end
        if (r_valid && r_id == 2 && push_ready && _t2_loop_cnt >= 8'(burst_len_r - 1)) begin
          thread_complete[2] <= 16'(thread_complete[2] + 1);
        end
        if (r_valid && r_id == 2 && push_ready && _t2_loop_cnt < 8'(burst_len_r - 1)) begin
          _t2_state <= 2;
        end
        if (r_valid && r_id == 2 && push_ready && _t2_loop_cnt >= 8'(burst_len_r - 1)) begin
          _t2_state <= 0;
        end
      end
      if (_t3_state == 0) begin
        if (active && (thread_complete[3] << 2) + 3 < total_xfers_r) begin
          _t3_state <= 1;
        end
      end
      if (_t3_state == 1) begin
        _t3_loop_cnt <= 0;
        if (_ar_ch_grant_3 && ar_ready) begin
          _t3_state <= 2;
        end
      end
      if (_t3_state == 2) begin
        if (r_valid && r_id == 3 && push_ready) begin
          _t3_loop_cnt <= 8'(_t3_loop_cnt + 8'd1);
        end
        if (r_valid && r_id == 3 && push_ready && _t3_loop_cnt >= 8'(burst_len_r - 1)) begin
          thread_complete[3] <= 16'(thread_complete[3] + 1);
        end
        if (r_valid && r_id == 3 && push_ready && _t3_loop_cnt < 8'(burst_len_r - 1)) begin
          _t3_state <= 2;
        end
        if (r_valid && r_id == 3 && push_ready && _t3_loop_cnt >= 8'(burst_len_r - 1)) begin
          _t3_state <= 0;
        end
      end
    end
  end
  logic [7:0] _t0_loop_cnt = 0;
  logic [7:0] _t1_loop_cnt = 0;
  logic [7:0] _t2_loop_cnt = 0;
  logic [7:0] _t3_loop_cnt = 0;

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

  logic [15:0] total_xfers_r;
  logic [31:0] base_addr_r;
  logic [7:0] burst_len_r;
  logic active_r;
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
  always_ff @(posedge clk) begin
    if (rst) begin
      active_r <= 1'b0;
      base_addr_r <= 0;
      burst_len_r <= 0;
      total_xfers_r <= 0;
    end else begin
      if (start && !active_r) begin
        total_xfers_r <= total_xfers;
        base_addr_r <= base_addr;
        burst_len_r <= burst_len;
        active_r <= 1'b1;
        thread_complete[0] <= 0;
        thread_complete[1] <= 0;
        thread_complete[2] <= 0;
        thread_complete[3] <= 0;
      end
      if (all_done) begin
        active_r <= 1'b0;
      end
    end
  end
  logic [3:0] [15:0] thread_complete;
  _ThreadMm2s_threads _threads (
    .clk(clk),
    .rst(rst),
    .active(active),
    .ar_ready(ar_ready),
    .base_addr_r(base_addr_r),
    .burst_len_r(burst_len_r),
    .push_ready(push_ready),
    .r_data(r_data),
    .r_id(r_id),
    .r_valid(r_valid),
    .total_xfers_r(total_xfers_r),
    .ar_addr(ar_addr),
    .ar_burst(ar_burst),
    .ar_id(ar_id),
    .ar_len(ar_len),
    .ar_size(ar_size),
    .ar_valid(ar_valid),
    .push_data(push_data),
    .push_valid(push_valid),
    .r_ready(r_ready),
    .thread_complete(thread_complete)
  );

endmodule

