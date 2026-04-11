// Thread-based multi-outstanding MM2S read engine.
// Each thread index = AXI ID. Thread i issues burst i.
// No shared counters — thread index determines address and ID.
module _ThreadMm2s_threads (
  input logic clk,
  input logic rst,
  input logic ar_ready,
  input logic [32-1:0] base_addr,
  input logic [8-1:0] burst_len,
  input logic done,
  input logic push_ready,
  input logic [32-1:0] r_data,
  input logic [2-1:0] r_id,
  input logic r_valid,
  input logic start,
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
  output logic active_r,
  output logic [32-1:0] base_addr_r,
  output logic [8-1:0] burst_len_r,
  output logic done_0,
  output logic done_1,
  output logic done_2,
  output logic done_3,
  output logic [16-1:0] total_xfers_r
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
    _ar_ch_req_4 = 1'b0;
    if (_t1_state == 1) begin
      // Control latches (Controller-only)
      // Per-thread done flags (shared: Controller clears, threads set)
      // Controller: latches start, waits for completion
      // Read thread 0
      _ar_ch_req_1 = 1;
    end
    if (_t1_state == 2) begin
      _ar_ch_req_1 = 1;
      ar_valid = 1;
      ar_addr = base_addr_r;
      ar_id = 0;
      ar_len = 8'(burst_len_r - 1);
      ar_size = 3'd2;
      ar_burst = 2'd1;
    end
    if (_t1_state == 3) begin
      _ar_ch_req_1 = 1;
      ar_valid = 0;
    end
    if (_t1_state == 5) begin
      r_ready = r_id == 0;
    end
    if (_t1_state == 6) begin
      // Push to FIFO — hold until accepted
      push_valid = 1;
      push_data = r_data;
    end
    if (_t1_state == 8) begin
      r_ready = 0;
    end
    if (_t2_state == 1) begin
      // Read thread 1
      _ar_ch_req_2 = 1;
    end
    if (_t2_state == 2) begin
      _ar_ch_req_2 = 1;
      ar_valid = 1;
      ar_addr = 32'(base_addr_r + (32'($unsigned(burst_len_r)) << 2));
      ar_id = 1;
      ar_len = 8'(burst_len_r - 1);
      ar_size = 3'd2;
      ar_burst = 2'd1;
    end
    if (_t2_state == 3) begin
      _ar_ch_req_2 = 1;
      ar_valid = 0;
    end
    if (_t2_state == 5) begin
      r_ready = r_id == 1;
    end
    if (_t2_state == 6) begin
      push_valid = 1;
      push_data = r_data;
    end
    if (_t2_state == 8) begin
      r_ready = 0;
    end
    if (_t3_state == 1) begin
      // Read thread 2
      _ar_ch_req_3 = 1;
    end
    if (_t3_state == 2) begin
      _ar_ch_req_3 = 1;
      ar_valid = 1;
      ar_addr = 32'(base_addr_r + 2 * (32'($unsigned(burst_len_r)) << 2));
      ar_id = 2;
      ar_len = 8'(burst_len_r - 1);
      ar_size = 3'd2;
      ar_burst = 2'd1;
    end
    if (_t3_state == 3) begin
      _ar_ch_req_3 = 1;
      ar_valid = 0;
    end
    if (_t3_state == 5) begin
      r_ready = r_id == 2;
    end
    if (_t3_state == 6) begin
      push_valid = 1;
      push_data = r_data;
    end
    if (_t3_state == 8) begin
      r_ready = 0;
    end
    if (_t4_state == 1) begin
      // Read thread 3
      _ar_ch_req_4 = 1;
    end
    if (_t4_state == 2) begin
      _ar_ch_req_4 = 1;
      ar_valid = 1;
      ar_addr = 32'(base_addr_r + 3 * (32'($unsigned(burst_len_r)) << 2));
      ar_id = 3;
      ar_len = 8'(burst_len_r - 1);
      ar_size = 3'd2;
      ar_burst = 2'd1;
    end
    if (_t4_state == 3) begin
      _ar_ch_req_4 = 1;
      ar_valid = 0;
    end
    if (_t4_state == 5) begin
      r_ready = r_id == 3;
    end
    if (_t4_state == 6) begin
      push_valid = 1;
      push_data = r_data;
    end
    if (_t4_state == 8) begin
      r_ready = 0;
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
  logic _ar_ch_req_4;
  logic _ar_ch_grant_4;
  assign _ar_ch_grant_0 = _ar_ch_req_0;
  assign _ar_ch_grant_1 = _ar_ch_req_1 && !_ar_ch_grant_0;
  assign _ar_ch_grant_2 = _ar_ch_req_2 && !_ar_ch_grant_0 && !_ar_ch_grant_1;
  assign _ar_ch_grant_3 = _ar_ch_req_3 && !_ar_ch_grant_0 && !_ar_ch_grant_1 && !_ar_ch_grant_2;
  assign _ar_ch_grant_4 = _ar_ch_req_4 && !_ar_ch_grant_0 && !_ar_ch_grant_1 && !_ar_ch_grant_2 && !_ar_ch_grant_3;
  logic [2-1:0] _t0_state = 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      _t0_state <= 0;
      active_r <= 1'b0;
      base_addr_r <= 0;
      burst_len_r <= 0;
      done_0 <= 1'b0;
      done_1 <= 1'b0;
      done_2 <= 1'b0;
      done_3 <= 1'b0;
      total_xfers_r <= 0;
    end else begin
      if (_t0_state == 0) begin
        if (start) begin
          _t0_state <= 1;
        end
      end
      if (_t0_state == 1) begin
        total_xfers_r <= total_xfers;
        base_addr_r <= base_addr;
        burst_len_r <= burst_len;
        active_r <= 1'b1;
        done_0 <= 1'b0;
        done_1 <= 1'b0;
        done_2 <= 1'b0;
        done_3 <= 1'b0;
        if (done) begin
          _t0_cnt <= 32'(1 - 32'd1);
        end
        if (done) begin
          _t0_state <= 2;
        end
      end
      if (_t0_state == 2) begin
        active_r <= 1'b0;
        _t0_cnt <= 32'(_t0_cnt - 32'd1);
        if (_t0_cnt == 0) begin
          _t0_state <= 0;
        end
      end
    end
  end
  logic [4-1:0] _t1_state = 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      _t1_state <= 0;
      done_0 <= 1'b0;
    end else begin
      if (_t1_state == 0) begin
        if (active_r && !done_0 && total_xfers_r > 0) begin
          _t1_state <= 1;
        end
      end
      if (_t1_state == 1) begin
        if (_ar_ch_grant_1) begin
          _t1_state <= 2;
        end
      end
      if (_t1_state == 2) begin
        if (ar_ready) begin
          _t1_state <= 3;
        end
      end
      if (_t1_state == 3) begin
        _t1_state <= 4;
      end
      if (_t1_state == 4) begin
        _t1_loop_cnt <= 0;
        _t1_state <= 5;
      end
      if (_t1_state == 5) begin
        if (r_valid && r_id == 0) begin
          _t1_state <= 6;
        end
      end
      if (_t1_state == 6) begin
        if (push_ready) begin
          _t1_state <= 7;
        end
      end
      if (_t1_state == 7) begin
        _t1_loop_cnt <= 32'(_t1_loop_cnt + 32'd1);
        _t1_cnt <= 32'(1 - 32'd1);
        if (_t1_loop_cnt < burst_len_r - 1) begin
          _t1_state <= 5;
        end
        if (_t1_loop_cnt >= burst_len_r - 1) begin
          _t1_state <= 8;
        end
      end
      if (_t1_state == 8) begin
        done_0 <= 1'b1;
        _t1_cnt <= 32'(_t1_cnt - 32'd1);
        if (_t1_cnt == 0) begin
          _t1_state <= 0;
        end
      end
    end
  end
  logic [4-1:0] _t2_state = 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      _t2_state <= 0;
      done_1 <= 1'b0;
    end else begin
      if (_t2_state == 0) begin
        if (active_r && !done_1 && total_xfers_r > 1) begin
          _t2_state <= 1;
        end
      end
      if (_t2_state == 1) begin
        if (_ar_ch_grant_2) begin
          _t2_state <= 2;
        end
      end
      if (_t2_state == 2) begin
        if (ar_ready) begin
          _t2_state <= 3;
        end
      end
      if (_t2_state == 3) begin
        _t2_state <= 4;
      end
      if (_t2_state == 4) begin
        _t2_loop_cnt <= 0;
        _t2_state <= 5;
      end
      if (_t2_state == 5) begin
        if (r_valid && r_id == 1) begin
          _t2_state <= 6;
        end
      end
      if (_t2_state == 6) begin
        if (push_ready) begin
          _t2_state <= 7;
        end
      end
      if (_t2_state == 7) begin
        _t2_loop_cnt <= 32'(_t2_loop_cnt + 32'd1);
        _t2_cnt <= 32'(1 - 32'd1);
        if (_t2_loop_cnt < burst_len_r - 1) begin
          _t2_state <= 5;
        end
        if (_t2_loop_cnt >= burst_len_r - 1) begin
          _t2_state <= 8;
        end
      end
      if (_t2_state == 8) begin
        done_1 <= 1'b1;
        _t2_cnt <= 32'(_t2_cnt - 32'd1);
        if (_t2_cnt == 0) begin
          _t2_state <= 0;
        end
      end
    end
  end
  logic [4-1:0] _t3_state = 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      _t3_state <= 0;
      done_2 <= 1'b0;
    end else begin
      if (_t3_state == 0) begin
        if (active_r && !done_2 && total_xfers_r > 2) begin
          _t3_state <= 1;
        end
      end
      if (_t3_state == 1) begin
        if (_ar_ch_grant_3) begin
          _t3_state <= 2;
        end
      end
      if (_t3_state == 2) begin
        if (ar_ready) begin
          _t3_state <= 3;
        end
      end
      if (_t3_state == 3) begin
        _t3_state <= 4;
      end
      if (_t3_state == 4) begin
        _t3_loop_cnt <= 0;
        _t3_state <= 5;
      end
      if (_t3_state == 5) begin
        if (r_valid && r_id == 2) begin
          _t3_state <= 6;
        end
      end
      if (_t3_state == 6) begin
        if (push_ready) begin
          _t3_state <= 7;
        end
      end
      if (_t3_state == 7) begin
        _t3_loop_cnt <= 32'(_t3_loop_cnt + 32'd1);
        _t3_cnt <= 32'(1 - 32'd1);
        if (_t3_loop_cnt < burst_len_r - 1) begin
          _t3_state <= 5;
        end
        if (_t3_loop_cnt >= burst_len_r - 1) begin
          _t3_state <= 8;
        end
      end
      if (_t3_state == 8) begin
        done_2 <= 1'b1;
        _t3_cnt <= 32'(_t3_cnt - 32'd1);
        if (_t3_cnt == 0) begin
          _t3_state <= 0;
        end
      end
    end
  end
  logic [4-1:0] _t4_state = 0;
  always_ff @(posedge clk) begin
    if (rst) begin
      _t4_state <= 0;
      done_3 <= 1'b0;
    end else begin
      if (_t4_state == 0) begin
        if (active_r && !done_3 && total_xfers_r > 3) begin
          _t4_state <= 1;
        end
      end
      if (_t4_state == 1) begin
        if (_ar_ch_grant_4) begin
          _t4_state <= 2;
        end
      end
      if (_t4_state == 2) begin
        if (ar_ready) begin
          _t4_state <= 3;
        end
      end
      if (_t4_state == 3) begin
        _t4_state <= 4;
      end
      if (_t4_state == 4) begin
        _t4_loop_cnt <= 0;
        _t4_state <= 5;
      end
      if (_t4_state == 5) begin
        if (r_valid && r_id == 3) begin
          _t4_state <= 6;
        end
      end
      if (_t4_state == 6) begin
        if (push_ready) begin
          _t4_state <= 7;
        end
      end
      if (_t4_state == 7) begin
        _t4_loop_cnt <= 32'(_t4_loop_cnt + 32'd1);
        _t4_cnt <= 32'(1 - 32'd1);
        if (_t4_loop_cnt < burst_len_r - 1) begin
          _t4_state <= 5;
        end
        if (_t4_loop_cnt >= burst_len_r - 1) begin
          _t4_state <= 8;
        end
      end
      if (_t4_state == 8) begin
        done_3 <= 1'b1;
        _t4_cnt <= 32'(_t4_cnt - 32'd1);
        if (_t4_cnt == 0) begin
          _t4_state <= 0;
        end
      end
    end
  end
  logic [32-1:0] _t0_cnt = 0;
  logic [32-1:0] _t1_cnt = 0;
  logic [32-1:0] _t1_loop_cnt = 0;
  logic [32-1:0] _t2_cnt = 0;
  logic [32-1:0] _t2_loop_cnt = 0;
  logic [32-1:0] _t3_cnt = 0;
  logic [32-1:0] _t3_loop_cnt = 0;
  logic [32-1:0] _t4_cnt = 0;
  logic [32-1:0] _t4_loop_cnt = 0;

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

  assign halted = 1'b0;
  assign idle_out = !active_r;
  assign done = active_r && total_xfers_r != 0 && (done_0 || total_xfers_r < 1) && (done_1 || total_xfers_r < 2) && (done_2 || total_xfers_r < 3) && (done_3 || total_xfers_r < 4);
  logic active_r;
  logic [32-1:0] base_addr_r;
  logic [8-1:0] burst_len_r;
  logic done_0;
  logic done_1;
  logic done_2;
  logic done_3;
  logic [16-1:0] total_xfers_r;
  _ThreadMm2s_threads _threads (
    .clk(clk),
    .rst(rst),
    .ar_ready(ar_ready),
    .base_addr(base_addr),
    .burst_len(burst_len),
    .done(done),
    .push_ready(push_ready),
    .r_data(r_data),
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
    .push_data(push_data),
    .push_valid(push_valid),
    .r_ready(r_ready),
    .active_r(active_r),
    .base_addr_r(base_addr_r),
    .burst_len_r(burst_len_r),
    .done_0(done_0),
    .done_1(done_1),
    .done_2(done_2),
    .done_3(done_3),
    .total_xfers_r(total_xfers_r)
  );

endmodule

