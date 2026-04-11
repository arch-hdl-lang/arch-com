// Thread-based multi-outstanding MM2S read engine.
// Uses generate_for to unroll N identical read threads.
module _ThreadMm2s_threads (
  input logic clk,
  input logic rst,
  input logic active,
  input logic ar_ready,
  input logic [32-1:0] base_addr_r,
  input logic [8-1:0] burst_len_r,
  input logic push_ready,
  input logic [32-1:0] r_data,
  input logic [2-1:0] r_id,
  input logic r_valid,
  input logic [16-1:0] total_xfers,
  output logic [32-1:0] ar_addr,
  output logic [2-1:0] ar_burst,
  output logic [2-1:0] ar_id,
  output logic [8-1:0] ar_len,
  output logic [3-1:0] ar_size,
  output logic ar_valid,
  output logic [32-1:0] push_data,
  output logic push_valid,
  output logic r_ready,
  output logic done_flags [4-1:0]
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
      // Per-thread done flags (Vec indexed by thread)
      // Combinational done/status
      // Combinational active includes start pulse — zero startup latency
      // Controller: plain seq block, no thread needed
      // Read threads — one per outstanding transaction
      _ar_ch_req_0 = 1;
      if (_ar_ch_grant_0) begin
        ar_valid = 1;
        ar_addr = 32'(base_addr_r + 0 * (32'($unsigned(burst_len_r)) << 2));
        ar_id = 0;
        ar_len = 8'(burst_len_r - 1);
        ar_size = 3'd2;
        ar_burst = 2'd1;
      end
    end
    if (_t0_state == 2) begin
      r_ready = r_ready | r_id == 0;
      push_valid = push_valid | (r_valid && r_id == 0);
      push_data = r_data;
    end
    if (_t1_state == 1) begin
      _ar_ch_req_1 = 1;
      if (_ar_ch_grant_1) begin
        ar_valid = 1;
        ar_addr = 32'(base_addr_r + 1 * (32'($unsigned(burst_len_r)) << 2));
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
        ar_addr = 32'(base_addr_r + 2 * (32'($unsigned(burst_len_r)) << 2));
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
        ar_addr = 32'(base_addr_r + 3 * (32'($unsigned(burst_len_r)) << 2));
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
  logic [2-1:0] _t0_state = 0;
  logic [2-1:0] _t1_state = 0;
  logic [2-1:0] _t2_state = 0;
  logic [2-1:0] _t3_state = 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      _t0_state <= 0;
      _t1_state <= 0;
      _t2_state <= 0;
      _t3_state <= 0;
      for (int __ri0 = 0; __ri0 < 4; __ri0++) begin
        done_flags[__ri0] <= 0;
      end
    end else begin
      if (_t0_state == 0) begin
        if (active && !done_flags[0] && total_xfers > 0) begin
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
          _t0_loop_cnt <= 32'(_t0_loop_cnt + 32'd1);
        end
        if (r_valid && r_id == 0 && push_ready && _t0_loop_cnt >= burst_len_r - 1) begin
          done_flags[0] <= 1'b1;
        end
        if (r_valid && r_id == 0 && push_ready && _t0_loop_cnt < burst_len_r - 1) begin
          _t0_state <= 2;
        end
        if (r_valid && r_id == 0 && push_ready && _t0_loop_cnt >= burst_len_r - 1) begin
          _t0_state <= 0;
        end
      end
      if (_t1_state == 0) begin
        if (active && !done_flags[1] && total_xfers > 1) begin
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
          _t1_loop_cnt <= 32'(_t1_loop_cnt + 32'd1);
        end
        if (r_valid && r_id == 1 && push_ready && _t1_loop_cnt >= burst_len_r - 1) begin
          done_flags[1] <= 1'b1;
        end
        if (r_valid && r_id == 1 && push_ready && _t1_loop_cnt < burst_len_r - 1) begin
          _t1_state <= 2;
        end
        if (r_valid && r_id == 1 && push_ready && _t1_loop_cnt >= burst_len_r - 1) begin
          _t1_state <= 0;
        end
      end
      if (_t2_state == 0) begin
        if (active && !done_flags[2] && total_xfers > 2) begin
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
          _t2_loop_cnt <= 32'(_t2_loop_cnt + 32'd1);
        end
        if (r_valid && r_id == 2 && push_ready && _t2_loop_cnt >= burst_len_r - 1) begin
          done_flags[2] <= 1'b1;
        end
        if (r_valid && r_id == 2 && push_ready && _t2_loop_cnt < burst_len_r - 1) begin
          _t2_state <= 2;
        end
        if (r_valid && r_id == 2 && push_ready && _t2_loop_cnt >= burst_len_r - 1) begin
          _t2_state <= 0;
        end
      end
      if (_t3_state == 0) begin
        if (active && !done_flags[3] && total_xfers > 3) begin
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
          _t3_loop_cnt <= 32'(_t3_loop_cnt + 32'd1);
        end
        if (r_valid && r_id == 3 && push_ready && _t3_loop_cnt >= burst_len_r - 1) begin
          done_flags[3] <= 1'b1;
        end
        if (r_valid && r_id == 3 && push_ready && _t3_loop_cnt < burst_len_r - 1) begin
          _t3_state <= 2;
        end
        if (r_valid && r_id == 3 && push_ready && _t3_loop_cnt >= burst_len_r - 1) begin
          _t3_state <= 0;
        end
      end
    end
  end
  logic [32-1:0] _t0_loop_cnt = 0;
  logic [32-1:0] _t1_loop_cnt = 0;
  logic [32-1:0] _t2_loop_cnt = 0;
  logic [32-1:0] _t3_loop_cnt = 0;

endmodule

module ThreadMm2s #(
  parameter int NUM_OUTSTANDING = 4
) (
  input logic clk,
  input logic rst,
  input logic start,
  input logic [16-1:0] total_xfers,
  input logic [32-1:0] base_addr,
  input logic [8-1:0] burst_len,
  output logic done,
  output logic halted,
  output logic idle_out,
  output logic ar_valid,
  input logic ar_ready,
  output logic [32-1:0] ar_addr,
  output logic [2-1:0] ar_id,
  output logic [8-1:0] ar_len,
  output logic [3-1:0] ar_size,
  output logic [2-1:0] ar_burst,
  input logic r_valid,
  output logic r_ready,
  input logic [32-1:0] r_data,
  input logic [2-1:0] r_id,
  input logic r_last,
  output logic push_valid,
  input logic push_ready,
  output logic [32-1:0] push_data
);

  logic [16-1:0] total_xfers_r;
  logic [32-1:0] base_addr_r;
  logic [8-1:0] burst_len_r;
  logic active_r;
  logic all_done;
  assign all_done = active_r && total_xfers_r != 0 && (done_flags[0] || total_xfers_r < 1) && (done_flags[1] || total_xfers_r < 2) && (done_flags[2] || total_xfers_r < 3) && (done_flags[3] || total_xfers_r < 4);
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
        done_flags[0] <= 1'b0;
        done_flags[1] <= 1'b0;
        done_flags[2] <= 1'b0;
        done_flags[3] <= 1'b0;
      end
      if (all_done) begin
        active_r <= 1'b0;
      end
    end
  end
  logic done_flags [4-1:0];
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
    .total_xfers(total_xfers),
    .ar_addr(ar_addr),
    .ar_burst(ar_burst),
    .ar_id(ar_id),
    .ar_len(ar_len),
    .ar_size(ar_size),
    .ar_valid(ar_valid),
    .push_data(push_data),
    .push_valid(push_valid),
    .r_ready(r_ready),
    .done_flags(done_flags)
  );

endmodule

