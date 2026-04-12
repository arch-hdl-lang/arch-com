// Thread-based multi-outstanding S2MM write engine.
// Compare with FsmS2mmMulti.arch (226 lines, 4 states, manual AW/W/B).
// Uses fork/join for AW+W parallelism, resource/lock for arbitration.
module _ThreadS2mm_threads (
  input logic clk,
  input logic rst,
  input logic aw_ready,
  input logic [2-1:0] b_id,
  input logic b_valid,
  input logic [8-1:0] burst_len,
  input logic [32-1:0] pop_data,
  input logic pop_valid,
  input logic start,
  input logic w_ready,
  output logic [32-1:0] aw_addr,
  output logic [2-1:0] aw_burst,
  output logic [2-1:0] aw_id,
  output logic [8-1:0] aw_len,
  output logic [3-1:0] aw_size,
  output logic aw_valid,
  output logic b_ready,
  output logic pop_ready,
  output logic [32-1:0] w_data,
  output logic w_last,
  output logic [4-1:0] w_strb,
  output logic w_valid,
  output logic [16-1:0] aw_issued_r,
  output logic [16-1:0] b_received_r,
  output logic [32-1:0] next_addr_r
);

  always_comb begin
    aw_addr = 0;
    aw_burst = 0;
    aw_id = 0;
    aw_len = 0;
    aw_size = 0;
    aw_valid = 0;
    b_ready = 0;
    pop_ready = 0;
    w_data = 0;
    w_last = 0;
    w_strb = 0;
    w_valid = 0;
    _done_ch_req_0 = 1'b0;
    _done_ch_req_1 = 1'b0;
    _done_ch_req_2 = 1'b0;
    _done_ch_req_3 = 1'b0;
    _aw_ch_req_0 = 1'b0;
    _aw_ch_req_1 = 1'b0;
    _aw_ch_req_2 = 1'b0;
    _aw_ch_req_3 = 1'b0;
    _w_ch_req_0 = 1'b0;
    _w_ch_req_1 = 1'b0;
    _w_ch_req_2 = 1'b0;
    _w_ch_req_3 = 1'b0;
    if (_t0_state == 1) begin
      // Shared state — protected by locks
      // AW+W in parallel via fork/join
      // AW branch: exclusive access to address channel
      _aw_ch_req_0 = 1;
      if (_aw_ch_grant_0) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 0;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      // W branch: exclusive access to data channel
      _w_ch_req_0 = 1;
    end
    if (_t0_state == 2) begin
      _aw_ch_req_0 = 1;
      _w_ch_req_0 = 1;
    end
    if (_t0_state == 3) begin
      _aw_ch_req_0 = 1;
      aw_valid = 0;
      _w_ch_req_0 = 1;
    end
    if (_t0_state == 4) begin
      _w_ch_req_0 = 1;
    end
    if (_t0_state == 5) begin
      _aw_ch_req_0 = 1;
      if (_aw_ch_grant_0) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 0;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_0 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t0_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t0_state == 6) begin
      _aw_ch_req_0 = 1;
      _w_ch_req_0 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t0_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t0_state == 7) begin
      _aw_ch_req_0 = 1;
      aw_valid = 0;
      _w_ch_req_0 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t0_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t0_state == 8) begin
      _w_ch_req_0 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t0_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t0_state == 9) begin
      _aw_ch_req_0 = 1;
      if (_aw_ch_grant_0) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 0;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_0 = 1;
    end
    if (_t0_state == 10) begin
      _aw_ch_req_0 = 1;
      _w_ch_req_0 = 1;
    end
    if (_t0_state == 11) begin
      _aw_ch_req_0 = 1;
      aw_valid = 0;
      _w_ch_req_0 = 1;
    end
    if (_t0_state == 12) begin
      _w_ch_req_0 = 1;
    end
    if (_t0_state == 13) begin
      _aw_ch_req_0 = 1;
      if (_aw_ch_grant_0) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 0;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_0 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t0_state == 14) begin
      _aw_ch_req_0 = 1;
      _w_ch_req_0 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t0_state == 15) begin
      _aw_ch_req_0 = 1;
      aw_valid = 0;
      _w_ch_req_0 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t0_state == 16) begin
      _w_ch_req_0 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t0_state == 17) begin
      _aw_ch_req_0 = 1;
      if (_aw_ch_grant_0) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 0;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
    end
    if (_t0_state == 18) begin
      _aw_ch_req_0 = 1;
    end
    if (_t0_state == 19) begin
      _aw_ch_req_0 = 1;
      aw_valid = 0;
    end
    if (_t0_state == 21) begin
      // B phase: wait for write response matching this ID
      b_ready = b_ready | 1;
    end
    if (_t0_state == 23) begin
      b_ready = b_ready | 0;
    end
    if (_t0_state == 24) begin
      // Mark complete
      _done_ch_req_0 = 1;
    end
    if (_t0_state == 25) begin
      _done_ch_req_0 = 1;
    end
    if (_t1_state == 1) begin
      _aw_ch_req_1 = 1;
      if (_aw_ch_grant_1) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 1;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_1 = 1;
    end
    if (_t1_state == 2) begin
      _aw_ch_req_1 = 1;
      _w_ch_req_1 = 1;
    end
    if (_t1_state == 3) begin
      _aw_ch_req_1 = 1;
      aw_valid = 0;
      _w_ch_req_1 = 1;
    end
    if (_t1_state == 4) begin
      _w_ch_req_1 = 1;
    end
    if (_t1_state == 5) begin
      _aw_ch_req_1 = 1;
      if (_aw_ch_grant_1) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 1;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_1 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t1_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t1_state == 6) begin
      _aw_ch_req_1 = 1;
      _w_ch_req_1 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t1_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t1_state == 7) begin
      _aw_ch_req_1 = 1;
      aw_valid = 0;
      _w_ch_req_1 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t1_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t1_state == 8) begin
      _w_ch_req_1 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t1_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t1_state == 9) begin
      _aw_ch_req_1 = 1;
      if (_aw_ch_grant_1) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 1;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_1 = 1;
    end
    if (_t1_state == 10) begin
      _aw_ch_req_1 = 1;
      _w_ch_req_1 = 1;
    end
    if (_t1_state == 11) begin
      _aw_ch_req_1 = 1;
      aw_valid = 0;
      _w_ch_req_1 = 1;
    end
    if (_t1_state == 12) begin
      _w_ch_req_1 = 1;
    end
    if (_t1_state == 13) begin
      _aw_ch_req_1 = 1;
      if (_aw_ch_grant_1) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 1;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_1 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t1_state == 14) begin
      _aw_ch_req_1 = 1;
      _w_ch_req_1 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t1_state == 15) begin
      _aw_ch_req_1 = 1;
      aw_valid = 0;
      _w_ch_req_1 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t1_state == 16) begin
      _w_ch_req_1 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t1_state == 17) begin
      _aw_ch_req_1 = 1;
      if (_aw_ch_grant_1) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 1;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
    end
    if (_t1_state == 18) begin
      _aw_ch_req_1 = 1;
    end
    if (_t1_state == 19) begin
      _aw_ch_req_1 = 1;
      aw_valid = 0;
    end
    if (_t1_state == 21) begin
      b_ready = b_ready | 1;
    end
    if (_t1_state == 23) begin
      b_ready = b_ready | 0;
    end
    if (_t1_state == 24) begin
      _done_ch_req_1 = 1;
    end
    if (_t1_state == 25) begin
      _done_ch_req_1 = 1;
    end
    if (_t2_state == 1) begin
      _aw_ch_req_2 = 1;
      if (_aw_ch_grant_2) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 2;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_2 = 1;
    end
    if (_t2_state == 2) begin
      _aw_ch_req_2 = 1;
      _w_ch_req_2 = 1;
    end
    if (_t2_state == 3) begin
      _aw_ch_req_2 = 1;
      aw_valid = 0;
      _w_ch_req_2 = 1;
    end
    if (_t2_state == 4) begin
      _w_ch_req_2 = 1;
    end
    if (_t2_state == 5) begin
      _aw_ch_req_2 = 1;
      if (_aw_ch_grant_2) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 2;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_2 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t2_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t2_state == 6) begin
      _aw_ch_req_2 = 1;
      _w_ch_req_2 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t2_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t2_state == 7) begin
      _aw_ch_req_2 = 1;
      aw_valid = 0;
      _w_ch_req_2 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t2_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t2_state == 8) begin
      _w_ch_req_2 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t2_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t2_state == 9) begin
      _aw_ch_req_2 = 1;
      if (_aw_ch_grant_2) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 2;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_2 = 1;
    end
    if (_t2_state == 10) begin
      _aw_ch_req_2 = 1;
      _w_ch_req_2 = 1;
    end
    if (_t2_state == 11) begin
      _aw_ch_req_2 = 1;
      aw_valid = 0;
      _w_ch_req_2 = 1;
    end
    if (_t2_state == 12) begin
      _w_ch_req_2 = 1;
    end
    if (_t2_state == 13) begin
      _aw_ch_req_2 = 1;
      if (_aw_ch_grant_2) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 2;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_2 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t2_state == 14) begin
      _aw_ch_req_2 = 1;
      _w_ch_req_2 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t2_state == 15) begin
      _aw_ch_req_2 = 1;
      aw_valid = 0;
      _w_ch_req_2 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t2_state == 16) begin
      _w_ch_req_2 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t2_state == 17) begin
      _aw_ch_req_2 = 1;
      if (_aw_ch_grant_2) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 2;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
    end
    if (_t2_state == 18) begin
      _aw_ch_req_2 = 1;
    end
    if (_t2_state == 19) begin
      _aw_ch_req_2 = 1;
      aw_valid = 0;
    end
    if (_t2_state == 21) begin
      b_ready = b_ready | 1;
    end
    if (_t2_state == 23) begin
      b_ready = b_ready | 0;
    end
    if (_t2_state == 24) begin
      _done_ch_req_2 = 1;
    end
    if (_t2_state == 25) begin
      _done_ch_req_2 = 1;
    end
    if (_t3_state == 1) begin
      _aw_ch_req_3 = 1;
      if (_aw_ch_grant_3) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 3;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_3 = 1;
    end
    if (_t3_state == 2) begin
      _aw_ch_req_3 = 1;
      _w_ch_req_3 = 1;
    end
    if (_t3_state == 3) begin
      _aw_ch_req_3 = 1;
      aw_valid = 0;
      _w_ch_req_3 = 1;
    end
    if (_t3_state == 4) begin
      _w_ch_req_3 = 1;
    end
    if (_t3_state == 5) begin
      _aw_ch_req_3 = 1;
      if (_aw_ch_grant_3) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 3;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_3 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t3_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t3_state == 6) begin
      _aw_ch_req_3 = 1;
      _w_ch_req_3 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t3_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t3_state == 7) begin
      _aw_ch_req_3 = 1;
      aw_valid = 0;
      _w_ch_req_3 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t3_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t3_state == 8) begin
      _w_ch_req_3 = 1;
      w_valid = 1;
      w_data = pop_data;
      w_strb = 4'd15;
      w_last = _t3_loop_cnt == 8'(burst_len - 1);
      pop_ready = 1;
    end
    if (_t3_state == 9) begin
      _aw_ch_req_3 = 1;
      if (_aw_ch_grant_3) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 3;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_3 = 1;
    end
    if (_t3_state == 10) begin
      _aw_ch_req_3 = 1;
      _w_ch_req_3 = 1;
    end
    if (_t3_state == 11) begin
      _aw_ch_req_3 = 1;
      aw_valid = 0;
      _w_ch_req_3 = 1;
    end
    if (_t3_state == 12) begin
      _w_ch_req_3 = 1;
    end
    if (_t3_state == 13) begin
      _aw_ch_req_3 = 1;
      if (_aw_ch_grant_3) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 3;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      _w_ch_req_3 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t3_state == 14) begin
      _aw_ch_req_3 = 1;
      _w_ch_req_3 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t3_state == 15) begin
      _aw_ch_req_3 = 1;
      aw_valid = 0;
      _w_ch_req_3 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t3_state == 16) begin
      _w_ch_req_3 = 1;
      w_valid = 0;
      pop_ready = 0;
    end
    if (_t3_state == 17) begin
      _aw_ch_req_3 = 1;
      if (_aw_ch_grant_3) begin
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 3;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
    end
    if (_t3_state == 18) begin
      _aw_ch_req_3 = 1;
    end
    if (_t3_state == 19) begin
      _aw_ch_req_3 = 1;
      aw_valid = 0;
    end
    if (_t3_state == 21) begin
      b_ready = b_ready | 1;
    end
    if (_t3_state == 23) begin
      b_ready = b_ready | 0;
    end
    if (_t3_state == 24) begin
      _done_ch_req_3 = 1;
    end
    if (_t3_state == 25) begin
      _done_ch_req_3 = 1;
    end
  end
  logic _done_ch_req_0;
  logic _done_ch_grant_0;
  logic _done_ch_req_1;
  logic _done_ch_grant_1;
  logic _done_ch_req_2;
  logic _done_ch_grant_2;
  logic _done_ch_req_3;
  logic _done_ch_grant_3;
  assign _done_ch_grant_0 = _done_ch_req_0;
  assign _done_ch_grant_1 = _done_ch_req_1 && !_done_ch_grant_0;
  assign _done_ch_grant_2 = _done_ch_req_2 && !_done_ch_grant_0 && !_done_ch_grant_1;
  assign _done_ch_grant_3 = _done_ch_req_3 && !_done_ch_grant_0 && !_done_ch_grant_1 && !_done_ch_grant_2;
  logic _aw_ch_req_0;
  logic _aw_ch_grant_0;
  logic _aw_ch_req_1;
  logic _aw_ch_grant_1;
  logic _aw_ch_req_2;
  logic _aw_ch_grant_2;
  logic _aw_ch_req_3;
  logic _aw_ch_grant_3;
  assign _aw_ch_grant_0 = _aw_ch_req_0;
  assign _aw_ch_grant_1 = _aw_ch_req_1 && !_aw_ch_grant_0;
  assign _aw_ch_grant_2 = _aw_ch_req_2 && !_aw_ch_grant_0 && !_aw_ch_grant_1;
  assign _aw_ch_grant_3 = _aw_ch_req_3 && !_aw_ch_grant_0 && !_aw_ch_grant_1 && !_aw_ch_grant_2;
  logic _w_ch_req_0;
  logic _w_ch_grant_0;
  logic _w_ch_req_1;
  logic _w_ch_grant_1;
  logic _w_ch_req_2;
  logic _w_ch_grant_2;
  logic _w_ch_req_3;
  logic _w_ch_grant_3;
  assign _w_ch_grant_0 = _w_ch_req_0;
  assign _w_ch_grant_1 = _w_ch_req_1 && !_w_ch_grant_0;
  assign _w_ch_grant_2 = _w_ch_req_2 && !_w_ch_grant_0 && !_w_ch_grant_1;
  assign _w_ch_grant_3 = _w_ch_req_3 && !_w_ch_grant_0 && !_w_ch_grant_1 && !_w_ch_grant_2;
  logic [5-1:0] _t0_state = 0;
  logic [5-1:0] _t1_state = 0;
  logic [5-1:0] _t2_state = 0;
  logic [5-1:0] _t3_state = 0;
  always_ff @(posedge clk) begin
    if ((!rst)) begin
      _t0_state <= 0;
      _t1_state <= 0;
      _t2_state <= 0;
      _t3_state <= 0;
      aw_issued_r <= 0;
      b_received_r <= 0;
      next_addr_r <= 0;
    end else begin
      if (_t0_state == 0) begin
        if (start) begin
          _t0_state <= 1;
        end
      end
      if (_t0_state == 1) begin
        if (_aw_ch_grant_0) begin
          if (_aw_ch_grant_0) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_w_ch_grant_0) begin
          if (_w_ch_grant_0) begin
            _t0_loop_cnt <= 0;
          end
        end
        if (_aw_ch_grant_0 && _w_ch_grant_0) begin
          _t0_state <= 6;
        end
        if (_w_ch_grant_0 && !_aw_ch_grant_0) begin
          _t0_state <= 5;
        end
        if (_aw_ch_grant_0 && !_w_ch_grant_0) begin
          _t0_state <= 2;
        end
      end
      if (_t0_state == 2) begin
        if (_w_ch_grant_0) begin
          if (_w_ch_grant_0) begin
            _t0_loop_cnt <= 0;
          end
        end
        if (aw_ready && _w_ch_grant_0) begin
          _t0_state <= 7;
        end
        if (_w_ch_grant_0 && !aw_ready) begin
          _t0_state <= 6;
        end
        if (aw_ready && !_w_ch_grant_0) begin
          _t0_state <= 3;
        end
      end
      if (_t0_state == 3) begin
        if (_w_ch_grant_0) begin
          if (_w_ch_grant_0) begin
            _t0_loop_cnt <= 0;
          end
        end
        if (_w_ch_grant_0) begin
          _t0_state <= 8;
        end
        if (1'b1 && !_w_ch_grant_0) begin
          _t0_state <= 4;
        end
      end
      if (_t0_state == 4) begin
        if (_w_ch_grant_0) begin
          if (_w_ch_grant_0) begin
            _t0_loop_cnt <= 0;
          end
        end
        if (_w_ch_grant_0) begin
          _t0_state <= 8;
        end
      end
      if (_t0_state == 5) begin
        if (_aw_ch_grant_0) begin
          if (_aw_ch_grant_0) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_aw_ch_grant_0) begin
          _t0_state <= 10;
        end
        if (1'b1 && !_aw_ch_grant_0) begin
          _t0_state <= 9;
        end
      end
      if (_t0_state == 6) begin
        if (aw_ready) begin
          _t0_state <= 11;
        end
        if (1'b1 && !aw_ready) begin
          _t0_state <= 10;
        end
      end
      if (_t0_state == 7) begin
        if (1'b1) begin
          _t0_state <= 12;
        end
      end
      if (_t0_state == 8) begin
        if (1'b1) begin
          _t0_state <= 12;
        end
      end
      if (_t0_state == 9) begin
        if (_aw_ch_grant_0) begin
          if (_aw_ch_grant_0) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (w_ready && pop_valid) begin
          _t0_loop_cnt <= 8'(_t0_loop_cnt + 8'd1);
        end
        if (_aw_ch_grant_0) begin
          _t0_state <= 14;
        end
        if (1'b1 && !_aw_ch_grant_0) begin
          _t0_state <= 13;
        end
      end
      if (_t0_state == 10) begin
        if (w_ready && pop_valid) begin
          _t0_loop_cnt <= 8'(_t0_loop_cnt + 8'd1);
        end
        if (aw_ready) begin
          _t0_state <= 15;
        end
        if (1'b1 && !aw_ready) begin
          _t0_state <= 14;
        end
      end
      if (_t0_state == 11) begin
        if (w_ready && pop_valid) begin
          _t0_loop_cnt <= 8'(_t0_loop_cnt + 8'd1);
        end
        if (1'b1) begin
          _t0_state <= 16;
        end
      end
      if (_t0_state == 12) begin
        if (w_ready && pop_valid) begin
          _t0_loop_cnt <= 8'(_t0_loop_cnt + 8'd1);
        end
        if (1'b1) begin
          _t0_state <= 16;
        end
      end
      if (_t0_state == 13) begin
        if (_aw_ch_grant_0) begin
          if (_aw_ch_grant_0) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_aw_ch_grant_0) begin
          _t0_state <= 18;
        end
        if (1'b1 && !_aw_ch_grant_0) begin
          _t0_state <= 17;
        end
      end
      if (_t0_state == 14) begin
        if (aw_ready) begin
          _t0_state <= 19;
        end
        if (1'b1 && !aw_ready) begin
          _t0_state <= 18;
        end
      end
      if (_t0_state == 15) begin
        if (1'b1) begin
          _t0_state <= 20;
        end
      end
      if (_t0_state == 16) begin
        if (1'b1) begin
          _t0_state <= 20;
        end
      end
      if (_t0_state == 17) begin
        if (_aw_ch_grant_0) begin
          if (_aw_ch_grant_0) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_aw_ch_grant_0) begin
          _t0_state <= 18;
        end
      end
      if (_t0_state == 18) begin
        if (aw_ready) begin
          _t0_state <= 19;
        end
      end
      if (_t0_state == 19) begin
        if (1'b1) begin
          _t0_state <= 20;
        end
      end
      if (_t0_state == 20) begin
        _t0_state <= 21;
      end
      if (_t0_state == 21) begin
        _t0_state <= 22;
      end
      if (_t0_state == 22) begin
        if (b_valid && b_id == 0) begin
          _t0_state <= 23;
        end
      end
      if (_t0_state == 23) begin
        _t0_state <= 24;
      end
      if (_t0_state == 24) begin
        if (_done_ch_grant_0) begin
          b_received_r <= 16'(b_received_r + 1);
        end
        if (_done_ch_grant_0) begin
          _t0_cnt <= 32'(1 - 32'd1);
        end
        if (_done_ch_grant_0) begin
          _t0_state <= 25;
        end
      end
      if (_t0_state == 25) begin
        _t0_cnt <= 32'(_t0_cnt - 32'd1);
        if (_t0_cnt == 0) begin
          _t0_state <= 0;
        end
      end
      if (_t1_state == 0) begin
        if (start) begin
          _t1_state <= 1;
        end
      end
      if (_t1_state == 1) begin
        if (_aw_ch_grant_1) begin
          if (_aw_ch_grant_1) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_w_ch_grant_1) begin
          if (_w_ch_grant_1) begin
            _t1_loop_cnt <= 0;
          end
        end
        if (_aw_ch_grant_1 && _w_ch_grant_1) begin
          _t1_state <= 6;
        end
        if (_w_ch_grant_1 && !_aw_ch_grant_1) begin
          _t1_state <= 5;
        end
        if (_aw_ch_grant_1 && !_w_ch_grant_1) begin
          _t1_state <= 2;
        end
      end
      if (_t1_state == 2) begin
        if (_w_ch_grant_1) begin
          if (_w_ch_grant_1) begin
            _t1_loop_cnt <= 0;
          end
        end
        if (aw_ready && _w_ch_grant_1) begin
          _t1_state <= 7;
        end
        if (_w_ch_grant_1 && !aw_ready) begin
          _t1_state <= 6;
        end
        if (aw_ready && !_w_ch_grant_1) begin
          _t1_state <= 3;
        end
      end
      if (_t1_state == 3) begin
        if (_w_ch_grant_1) begin
          if (_w_ch_grant_1) begin
            _t1_loop_cnt <= 0;
          end
        end
        if (_w_ch_grant_1) begin
          _t1_state <= 8;
        end
        if (1'b1 && !_w_ch_grant_1) begin
          _t1_state <= 4;
        end
      end
      if (_t1_state == 4) begin
        if (_w_ch_grant_1) begin
          if (_w_ch_grant_1) begin
            _t1_loop_cnt <= 0;
          end
        end
        if (_w_ch_grant_1) begin
          _t1_state <= 8;
        end
      end
      if (_t1_state == 5) begin
        if (_aw_ch_grant_1) begin
          if (_aw_ch_grant_1) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_aw_ch_grant_1) begin
          _t1_state <= 10;
        end
        if (1'b1 && !_aw_ch_grant_1) begin
          _t1_state <= 9;
        end
      end
      if (_t1_state == 6) begin
        if (aw_ready) begin
          _t1_state <= 11;
        end
        if (1'b1 && !aw_ready) begin
          _t1_state <= 10;
        end
      end
      if (_t1_state == 7) begin
        if (1'b1) begin
          _t1_state <= 12;
        end
      end
      if (_t1_state == 8) begin
        if (1'b1) begin
          _t1_state <= 12;
        end
      end
      if (_t1_state == 9) begin
        if (_aw_ch_grant_1) begin
          if (_aw_ch_grant_1) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (w_ready && pop_valid) begin
          _t1_loop_cnt <= 8'(_t1_loop_cnt + 8'd1);
        end
        if (_aw_ch_grant_1) begin
          _t1_state <= 14;
        end
        if (1'b1 && !_aw_ch_grant_1) begin
          _t1_state <= 13;
        end
      end
      if (_t1_state == 10) begin
        if (w_ready && pop_valid) begin
          _t1_loop_cnt <= 8'(_t1_loop_cnt + 8'd1);
        end
        if (aw_ready) begin
          _t1_state <= 15;
        end
        if (1'b1 && !aw_ready) begin
          _t1_state <= 14;
        end
      end
      if (_t1_state == 11) begin
        if (w_ready && pop_valid) begin
          _t1_loop_cnt <= 8'(_t1_loop_cnt + 8'd1);
        end
        if (1'b1) begin
          _t1_state <= 16;
        end
      end
      if (_t1_state == 12) begin
        if (w_ready && pop_valid) begin
          _t1_loop_cnt <= 8'(_t1_loop_cnt + 8'd1);
        end
        if (1'b1) begin
          _t1_state <= 16;
        end
      end
      if (_t1_state == 13) begin
        if (_aw_ch_grant_1) begin
          if (_aw_ch_grant_1) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_aw_ch_grant_1) begin
          _t1_state <= 18;
        end
        if (1'b1 && !_aw_ch_grant_1) begin
          _t1_state <= 17;
        end
      end
      if (_t1_state == 14) begin
        if (aw_ready) begin
          _t1_state <= 19;
        end
        if (1'b1 && !aw_ready) begin
          _t1_state <= 18;
        end
      end
      if (_t1_state == 15) begin
        if (1'b1) begin
          _t1_state <= 20;
        end
      end
      if (_t1_state == 16) begin
        if (1'b1) begin
          _t1_state <= 20;
        end
      end
      if (_t1_state == 17) begin
        if (_aw_ch_grant_1) begin
          if (_aw_ch_grant_1) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_aw_ch_grant_1) begin
          _t1_state <= 18;
        end
      end
      if (_t1_state == 18) begin
        if (aw_ready) begin
          _t1_state <= 19;
        end
      end
      if (_t1_state == 19) begin
        if (1'b1) begin
          _t1_state <= 20;
        end
      end
      if (_t1_state == 20) begin
        _t1_state <= 21;
      end
      if (_t1_state == 21) begin
        _t1_state <= 22;
      end
      if (_t1_state == 22) begin
        if (b_valid && b_id == 1) begin
          _t1_state <= 23;
        end
      end
      if (_t1_state == 23) begin
        _t1_state <= 24;
      end
      if (_t1_state == 24) begin
        if (_done_ch_grant_1) begin
          b_received_r <= 16'(b_received_r + 1);
        end
        if (_done_ch_grant_1) begin
          _t1_cnt <= 32'(1 - 32'd1);
        end
        if (_done_ch_grant_1) begin
          _t1_state <= 25;
        end
      end
      if (_t1_state == 25) begin
        _t1_cnt <= 32'(_t1_cnt - 32'd1);
        if (_t1_cnt == 0) begin
          _t1_state <= 0;
        end
      end
      if (_t2_state == 0) begin
        if (start) begin
          _t2_state <= 1;
        end
      end
      if (_t2_state == 1) begin
        if (_aw_ch_grant_2) begin
          if (_aw_ch_grant_2) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_w_ch_grant_2) begin
          if (_w_ch_grant_2) begin
            _t2_loop_cnt <= 0;
          end
        end
        if (_aw_ch_grant_2 && _w_ch_grant_2) begin
          _t2_state <= 6;
        end
        if (_w_ch_grant_2 && !_aw_ch_grant_2) begin
          _t2_state <= 5;
        end
        if (_aw_ch_grant_2 && !_w_ch_grant_2) begin
          _t2_state <= 2;
        end
      end
      if (_t2_state == 2) begin
        if (_w_ch_grant_2) begin
          if (_w_ch_grant_2) begin
            _t2_loop_cnt <= 0;
          end
        end
        if (aw_ready && _w_ch_grant_2) begin
          _t2_state <= 7;
        end
        if (_w_ch_grant_2 && !aw_ready) begin
          _t2_state <= 6;
        end
        if (aw_ready && !_w_ch_grant_2) begin
          _t2_state <= 3;
        end
      end
      if (_t2_state == 3) begin
        if (_w_ch_grant_2) begin
          if (_w_ch_grant_2) begin
            _t2_loop_cnt <= 0;
          end
        end
        if (_w_ch_grant_2) begin
          _t2_state <= 8;
        end
        if (1'b1 && !_w_ch_grant_2) begin
          _t2_state <= 4;
        end
      end
      if (_t2_state == 4) begin
        if (_w_ch_grant_2) begin
          if (_w_ch_grant_2) begin
            _t2_loop_cnt <= 0;
          end
        end
        if (_w_ch_grant_2) begin
          _t2_state <= 8;
        end
      end
      if (_t2_state == 5) begin
        if (_aw_ch_grant_2) begin
          if (_aw_ch_grant_2) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_aw_ch_grant_2) begin
          _t2_state <= 10;
        end
        if (1'b1 && !_aw_ch_grant_2) begin
          _t2_state <= 9;
        end
      end
      if (_t2_state == 6) begin
        if (aw_ready) begin
          _t2_state <= 11;
        end
        if (1'b1 && !aw_ready) begin
          _t2_state <= 10;
        end
      end
      if (_t2_state == 7) begin
        if (1'b1) begin
          _t2_state <= 12;
        end
      end
      if (_t2_state == 8) begin
        if (1'b1) begin
          _t2_state <= 12;
        end
      end
      if (_t2_state == 9) begin
        if (_aw_ch_grant_2) begin
          if (_aw_ch_grant_2) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (w_ready && pop_valid) begin
          _t2_loop_cnt <= 8'(_t2_loop_cnt + 8'd1);
        end
        if (_aw_ch_grant_2) begin
          _t2_state <= 14;
        end
        if (1'b1 && !_aw_ch_grant_2) begin
          _t2_state <= 13;
        end
      end
      if (_t2_state == 10) begin
        if (w_ready && pop_valid) begin
          _t2_loop_cnt <= 8'(_t2_loop_cnt + 8'd1);
        end
        if (aw_ready) begin
          _t2_state <= 15;
        end
        if (1'b1 && !aw_ready) begin
          _t2_state <= 14;
        end
      end
      if (_t2_state == 11) begin
        if (w_ready && pop_valid) begin
          _t2_loop_cnt <= 8'(_t2_loop_cnt + 8'd1);
        end
        if (1'b1) begin
          _t2_state <= 16;
        end
      end
      if (_t2_state == 12) begin
        if (w_ready && pop_valid) begin
          _t2_loop_cnt <= 8'(_t2_loop_cnt + 8'd1);
        end
        if (1'b1) begin
          _t2_state <= 16;
        end
      end
      if (_t2_state == 13) begin
        if (_aw_ch_grant_2) begin
          if (_aw_ch_grant_2) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_aw_ch_grant_2) begin
          _t2_state <= 18;
        end
        if (1'b1 && !_aw_ch_grant_2) begin
          _t2_state <= 17;
        end
      end
      if (_t2_state == 14) begin
        if (aw_ready) begin
          _t2_state <= 19;
        end
        if (1'b1 && !aw_ready) begin
          _t2_state <= 18;
        end
      end
      if (_t2_state == 15) begin
        if (1'b1) begin
          _t2_state <= 20;
        end
      end
      if (_t2_state == 16) begin
        if (1'b1) begin
          _t2_state <= 20;
        end
      end
      if (_t2_state == 17) begin
        if (_aw_ch_grant_2) begin
          if (_aw_ch_grant_2) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_aw_ch_grant_2) begin
          _t2_state <= 18;
        end
      end
      if (_t2_state == 18) begin
        if (aw_ready) begin
          _t2_state <= 19;
        end
      end
      if (_t2_state == 19) begin
        if (1'b1) begin
          _t2_state <= 20;
        end
      end
      if (_t2_state == 20) begin
        _t2_state <= 21;
      end
      if (_t2_state == 21) begin
        _t2_state <= 22;
      end
      if (_t2_state == 22) begin
        if (b_valid && b_id == 2) begin
          _t2_state <= 23;
        end
      end
      if (_t2_state == 23) begin
        _t2_state <= 24;
      end
      if (_t2_state == 24) begin
        if (_done_ch_grant_2) begin
          b_received_r <= 16'(b_received_r + 1);
        end
        if (_done_ch_grant_2) begin
          _t2_cnt <= 32'(1 - 32'd1);
        end
        if (_done_ch_grant_2) begin
          _t2_state <= 25;
        end
      end
      if (_t2_state == 25) begin
        _t2_cnt <= 32'(_t2_cnt - 32'd1);
        if (_t2_cnt == 0) begin
          _t2_state <= 0;
        end
      end
      if (_t3_state == 0) begin
        if (start) begin
          _t3_state <= 1;
        end
      end
      if (_t3_state == 1) begin
        if (_aw_ch_grant_3) begin
          if (_aw_ch_grant_3) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_w_ch_grant_3) begin
          if (_w_ch_grant_3) begin
            _t3_loop_cnt <= 0;
          end
        end
        if (_aw_ch_grant_3 && _w_ch_grant_3) begin
          _t3_state <= 6;
        end
        if (_w_ch_grant_3 && !_aw_ch_grant_3) begin
          _t3_state <= 5;
        end
        if (_aw_ch_grant_3 && !_w_ch_grant_3) begin
          _t3_state <= 2;
        end
      end
      if (_t3_state == 2) begin
        if (_w_ch_grant_3) begin
          if (_w_ch_grant_3) begin
            _t3_loop_cnt <= 0;
          end
        end
        if (aw_ready && _w_ch_grant_3) begin
          _t3_state <= 7;
        end
        if (_w_ch_grant_3 && !aw_ready) begin
          _t3_state <= 6;
        end
        if (aw_ready && !_w_ch_grant_3) begin
          _t3_state <= 3;
        end
      end
      if (_t3_state == 3) begin
        if (_w_ch_grant_3) begin
          if (_w_ch_grant_3) begin
            _t3_loop_cnt <= 0;
          end
        end
        if (_w_ch_grant_3) begin
          _t3_state <= 8;
        end
        if (1'b1 && !_w_ch_grant_3) begin
          _t3_state <= 4;
        end
      end
      if (_t3_state == 4) begin
        if (_w_ch_grant_3) begin
          if (_w_ch_grant_3) begin
            _t3_loop_cnt <= 0;
          end
        end
        if (_w_ch_grant_3) begin
          _t3_state <= 8;
        end
      end
      if (_t3_state == 5) begin
        if (_aw_ch_grant_3) begin
          if (_aw_ch_grant_3) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_aw_ch_grant_3) begin
          _t3_state <= 10;
        end
        if (1'b1 && !_aw_ch_grant_3) begin
          _t3_state <= 9;
        end
      end
      if (_t3_state == 6) begin
        if (aw_ready) begin
          _t3_state <= 11;
        end
        if (1'b1 && !aw_ready) begin
          _t3_state <= 10;
        end
      end
      if (_t3_state == 7) begin
        if (1'b1) begin
          _t3_state <= 12;
        end
      end
      if (_t3_state == 8) begin
        if (1'b1) begin
          _t3_state <= 12;
        end
      end
      if (_t3_state == 9) begin
        if (_aw_ch_grant_3) begin
          if (_aw_ch_grant_3) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (w_ready && pop_valid) begin
          _t3_loop_cnt <= 8'(_t3_loop_cnt + 8'd1);
        end
        if (_aw_ch_grant_3) begin
          _t3_state <= 14;
        end
        if (1'b1 && !_aw_ch_grant_3) begin
          _t3_state <= 13;
        end
      end
      if (_t3_state == 10) begin
        if (w_ready && pop_valid) begin
          _t3_loop_cnt <= 8'(_t3_loop_cnt + 8'd1);
        end
        if (aw_ready) begin
          _t3_state <= 15;
        end
        if (1'b1 && !aw_ready) begin
          _t3_state <= 14;
        end
      end
      if (_t3_state == 11) begin
        if (w_ready && pop_valid) begin
          _t3_loop_cnt <= 8'(_t3_loop_cnt + 8'd1);
        end
        if (1'b1) begin
          _t3_state <= 16;
        end
      end
      if (_t3_state == 12) begin
        if (w_ready && pop_valid) begin
          _t3_loop_cnt <= 8'(_t3_loop_cnt + 8'd1);
        end
        if (1'b1) begin
          _t3_state <= 16;
        end
      end
      if (_t3_state == 13) begin
        if (_aw_ch_grant_3) begin
          if (_aw_ch_grant_3) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_aw_ch_grant_3) begin
          _t3_state <= 18;
        end
        if (1'b1 && !_aw_ch_grant_3) begin
          _t3_state <= 17;
        end
      end
      if (_t3_state == 14) begin
        if (aw_ready) begin
          _t3_state <= 19;
        end
        if (1'b1 && !aw_ready) begin
          _t3_state <= 18;
        end
      end
      if (_t3_state == 15) begin
        if (1'b1) begin
          _t3_state <= 20;
        end
      end
      if (_t3_state == 16) begin
        if (1'b1) begin
          _t3_state <= 20;
        end
      end
      if (_t3_state == 17) begin
        if (_aw_ch_grant_3) begin
          if (_aw_ch_grant_3) begin
            aw_issued_r <= 16'(aw_issued_r + 1);
            next_addr_r <= 32'(next_addr_r + (32'($unsigned(burst_len)) << 2));
          end
        end
        if (_aw_ch_grant_3) begin
          _t3_state <= 18;
        end
      end
      if (_t3_state == 18) begin
        if (aw_ready) begin
          _t3_state <= 19;
        end
      end
      if (_t3_state == 19) begin
        if (1'b1) begin
          _t3_state <= 20;
        end
      end
      if (_t3_state == 20) begin
        _t3_state <= 21;
      end
      if (_t3_state == 21) begin
        _t3_state <= 22;
      end
      if (_t3_state == 22) begin
        if (b_valid && b_id == 3) begin
          _t3_state <= 23;
        end
      end
      if (_t3_state == 23) begin
        _t3_state <= 24;
      end
      if (_t3_state == 24) begin
        if (_done_ch_grant_3) begin
          b_received_r <= 16'(b_received_r + 1);
        end
        if (_done_ch_grant_3) begin
          _t3_cnt <= 32'(1 - 32'd1);
        end
        if (_done_ch_grant_3) begin
          _t3_state <= 25;
        end
      end
      if (_t3_state == 25) begin
        _t3_cnt <= 32'(_t3_cnt - 32'd1);
        if (_t3_cnt == 0) begin
          _t3_state <= 0;
        end
      end
    end
  end
  logic [32-1:0] _t0_cnt = 0;
  logic [8-1:0] _t0_loop_cnt = 0;
  logic [32-1:0] _t1_cnt = 0;
  logic [8-1:0] _t1_loop_cnt = 0;
  logic [32-1:0] _t2_cnt = 0;
  logic [8-1:0] _t2_loop_cnt = 0;
  logic [32-1:0] _t3_cnt = 0;
  logic [8-1:0] _t3_loop_cnt = 0;

endmodule

module ThreadS2mm #(
  parameter int NUM_OUTSTANDING = 4,
  parameter int ID_W = 2
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
  output logic aw_valid,
  input logic aw_ready,
  output logic [32-1:0] aw_addr,
  output logic [2-1:0] aw_id,
  output logic [8-1:0] aw_len,
  output logic [3-1:0] aw_size,
  output logic [2-1:0] aw_burst,
  output logic w_valid,
  input logic w_ready,
  output logic [32-1:0] w_data,
  output logic [4-1:0] w_strb,
  output logic w_last,
  input logic b_valid,
  output logic b_ready,
  input logic [2-1:0] b_id,
  input logic pop_valid,
  output logic pop_ready,
  input logic [32-1:0] pop_data
);

  assign halted = 1'b0;
  assign idle_out = aw_issued_r == 0 && b_received_r == 0;
  assign done = b_received_r == total_xfers && b_received_r != 0;
  logic [16-1:0] aw_issued_r;
  logic [16-1:0] b_received_r;
  logic [32-1:0] next_addr_r;
  _ThreadS2mm_threads _threads (
    .clk(clk),
    .rst(rst),
    .aw_ready(aw_ready),
    .b_id(b_id),
    .b_valid(b_valid),
    .burst_len(burst_len),
    .pop_data(pop_data),
    .pop_valid(pop_valid),
    .start(start),
    .w_ready(w_ready),
    .aw_addr(aw_addr),
    .aw_burst(aw_burst),
    .aw_id(aw_id),
    .aw_len(aw_len),
    .aw_size(aw_size),
    .aw_valid(aw_valid),
    .b_ready(b_ready),
    .pop_ready(pop_ready),
    .w_data(w_data),
    .w_last(w_last),
    .w_strb(w_strb),
    .w_valid(w_valid),
    .aw_issued_r(aw_issued_r),
    .b_received_r(b_received_r),
    .next_addr_r(next_addr_r)
  );

endmodule

