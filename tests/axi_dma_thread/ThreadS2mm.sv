// Thread-based multi-outstanding S2MM write engine.
// Compare with FsmS2mmMulti.arch (226 lines, 4 states, manual AW/W/B).
// Uses fork/join for AW+W parallelism, resource/lock for arbitration.
// Shared state — protected by locks
module _ThreadS2mm_WriteReq_0 (
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
  output logic [32-1:0] next_addr_r,
  output logic _aw_ch_req,
  input logic _aw_ch_grant,
  output logic _w_ch_req,
  input logic _w_ch_grant,
  output logic _done_ch_req,
  input logic _done_ch_grant
);

  typedef enum logic [4:0] {
    S0 = 5'd0,
    S1 = 5'd1,
    S2 = 5'd2,
    S3 = 5'd3,
    S4 = 5'd4,
    S5 = 5'd5,
    S6 = 5'd6,
    S7 = 5'd7,
    S8 = 5'd8,
    S9 = 5'd9,
    S10 = 5'd10,
    S11 = 5'd11,
    S12 = 5'd12,
    S13 = 5'd13,
    S14 = 5'd14,
    S15 = 5'd15,
    S16 = 5'd16,
    S17 = 5'd17,
    S18 = 5'd18,
    S19 = 5'd19,
    S20 = 5'd20,
    S21 = 5'd21,
    S22 = 5'd22,
    S23 = 5'd23,
    S24 = 5'd24,
    S25 = 5'd25,
    S26 = 5'd26,
    S27 = 5'd27,
    S28 = 5'd28
  } _ThreadS2mm_WriteReq_0_state_t;
  
  _ThreadS2mm_WriteReq_0_state_t state_r, state_next;
  
  logic [32-1:0] _cnt;
  logic [32-1:0] _loop_cnt;
  
  always_ff @(posedge clk) begin
    if ((!rst)) begin
      state_r <= S0;
      _cnt <= 0;
      _loop_cnt <= 0;
      aw_issued_r <= 0;
      b_received_r <= 0;
      next_addr_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S2: begin
          // AW+W in parallel via fork/join
          if (aw_ready) begin
            // AW branch: exclusive access to address channel
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S5: begin
          // W branch: exclusive access to data channel
          _loop_cnt <= 0;
        end
        S6: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
          _loop_cnt <= 0;
        end
        S7: begin
          _loop_cnt <= 0;
        end
        S8: begin
          _loop_cnt <= 0;
        end
        S10: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S13: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S14: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
          _loop_cnt <= _loop_cnt + 1;
        end
        S15: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S16: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S18: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S22: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S27: begin
          if (_done_ch_grant) begin
            _cnt <= 1 - 1;
          end
        end
        S28: begin
          if (_cnt == 0) begin
            // B phase: wait for write response matching this ID
            // Mark complete
            b_received_r <= b_received_r + 1;
          end
          _cnt <= _cnt - 1;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S0: begin
        if (start) state_next = S1;
      end
      S1: begin
        if (_aw_ch_grant && _w_ch_grant) state_next = S6;
        else if (_w_ch_grant && !_aw_ch_grant) state_next = S5;
        else if (_aw_ch_grant && !_w_ch_grant) state_next = S2;
      end
      S2: begin
        if (aw_ready && _w_ch_grant) state_next = S7;
        else if (_w_ch_grant && !aw_ready) state_next = S6;
        else if (aw_ready && !_w_ch_grant) state_next = S3;
      end
      S3: begin
        if (_w_ch_grant) state_next = S8;
        else if (1'b1 && !_w_ch_grant) state_next = S4;
      end
      S4: begin
        if (_w_ch_grant) state_next = S8;
      end
      S5: begin
        if (_aw_ch_grant) state_next = S10;
        else if (1'b1 && !_aw_ch_grant) state_next = S9;
      end
      S6: begin
        if (aw_ready) state_next = S11;
        else if (1'b1 && !aw_ready) state_next = S10;
      end
      S7: begin
        state_next = S12;
      end
      S8: begin
        state_next = S12;
      end
      S9: begin
        if (_aw_ch_grant && w_ready && pop_valid) state_next = S14;
        else if (w_ready && pop_valid && !_aw_ch_grant) state_next = S13;
        else if (_aw_ch_grant && !(w_ready && pop_valid)) state_next = S10;
      end
      S10: begin
        if (aw_ready && w_ready && pop_valid) state_next = S15;
        else if (w_ready && pop_valid && !aw_ready) state_next = S14;
        else if (aw_ready && !(w_ready && pop_valid)) state_next = S11;
      end
      S11: begin
        if (w_ready && pop_valid) state_next = S16;
        else if (1'b1 && !(w_ready && pop_valid)) state_next = S12;
      end
      S12: begin
        if (w_ready && pop_valid) state_next = S16;
      end
      S13: begin
        if (_aw_ch_grant) state_next = S18;
        else if (1'b1 && !_aw_ch_grant) state_next = S17;
      end
      S14: begin
        if (aw_ready) state_next = S19;
        else if (1'b1 && !aw_ready) state_next = S18;
      end
      S15: begin
        state_next = S20;
      end
      S16: begin
        state_next = S20;
      end
      S17: begin
        if (_aw_ch_grant) state_next = S22;
        else if (1'b1 && !_aw_ch_grant) state_next = S21;
      end
      S18: begin
        if (aw_ready) state_next = S23;
        else if (1'b1 && !aw_ready) state_next = S22;
      end
      S19: begin
        state_next = S24;
      end
      S20: begin
        state_next = S24;
      end
      S21: begin
        if (_aw_ch_grant) state_next = S22;
      end
      S22: begin
        if (aw_ready) state_next = S23;
      end
      S23: begin
        state_next = S24;
      end
      S24: begin
        state_next = S25;
      end
      S25: begin
        if (b_valid && b_id == 0) state_next = S26;
      end
      S26: begin
        state_next = S27;
      end
      S27: begin
        if (_done_ch_grant) state_next = S28;
      end
      S28: begin
        if (_cnt == 0) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
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
    _aw_ch_req = 0;
    _w_ch_req = 0;
    _done_ch_req = 0;
    case (state_r)
      S0: begin
      end
      S1: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
      end
      S2: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 0;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
      end
      S3: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
      end
      S4: begin
        _w_ch_req = 1;
      end
      S5: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
      end
      S6: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 0;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
      end
      S7: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
      end
      S8: begin
        _w_ch_req = 1;
      end
      S9: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S10: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 0;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S11: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S12: begin
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S13: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
      end
      S14: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 0;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
      end
      S15: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
      end
      S16: begin
        _w_ch_req = 1;
      end
      S17: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S18: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 0;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S19: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S20: begin
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S21: begin
        _aw_ch_req = 1;
      end
      S22: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 0;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      S23: begin
        _aw_ch_req = 1;
        aw_valid = 0;
      end
      S24: begin
      end
      S25: begin
        b_ready = 1;
      end
      S26: begin
        b_ready = 0;
      end
      S27: begin
        _done_ch_req = 1;
      end
      S28: begin
        _done_ch_req = 1;
      end
      default: ;
    endcase
  end

endmodule

module _ThreadS2mm_WriteReq_1 (
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
  output logic [32-1:0] next_addr_r,
  output logic _aw_ch_req,
  input logic _aw_ch_grant,
  output logic _w_ch_req,
  input logic _w_ch_grant,
  output logic _done_ch_req,
  input logic _done_ch_grant
);

  typedef enum logic [4:0] {
    S0 = 5'd0,
    S1 = 5'd1,
    S2 = 5'd2,
    S3 = 5'd3,
    S4 = 5'd4,
    S5 = 5'd5,
    S6 = 5'd6,
    S7 = 5'd7,
    S8 = 5'd8,
    S9 = 5'd9,
    S10 = 5'd10,
    S11 = 5'd11,
    S12 = 5'd12,
    S13 = 5'd13,
    S14 = 5'd14,
    S15 = 5'd15,
    S16 = 5'd16,
    S17 = 5'd17,
    S18 = 5'd18,
    S19 = 5'd19,
    S20 = 5'd20,
    S21 = 5'd21,
    S22 = 5'd22,
    S23 = 5'd23,
    S24 = 5'd24,
    S25 = 5'd25,
    S26 = 5'd26,
    S27 = 5'd27,
    S28 = 5'd28
  } _ThreadS2mm_WriteReq_1_state_t;
  
  _ThreadS2mm_WriteReq_1_state_t state_r, state_next;
  
  logic [32-1:0] _cnt;
  logic [32-1:0] _loop_cnt;
  
  always_ff @(posedge clk) begin
    if ((!rst)) begin
      state_r <= S0;
      _cnt <= 0;
      _loop_cnt <= 0;
      aw_issued_r <= 0;
      b_received_r <= 0;
      next_addr_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S2: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S5: begin
          _loop_cnt <= 0;
        end
        S6: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
          _loop_cnt <= 0;
        end
        S7: begin
          _loop_cnt <= 0;
        end
        S8: begin
          _loop_cnt <= 0;
        end
        S10: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S13: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S14: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
          _loop_cnt <= _loop_cnt + 1;
        end
        S15: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S16: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S18: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S22: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S27: begin
          if (_done_ch_grant) begin
            _cnt <= 1 - 1;
          end
        end
        S28: begin
          if (_cnt == 0) begin
            b_received_r <= b_received_r + 1;
          end
          _cnt <= _cnt - 1;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S0: begin
        if (start) state_next = S1;
      end
      S1: begin
        if (_aw_ch_grant && _w_ch_grant) state_next = S6;
        else if (_w_ch_grant && !_aw_ch_grant) state_next = S5;
        else if (_aw_ch_grant && !_w_ch_grant) state_next = S2;
      end
      S2: begin
        if (aw_ready && _w_ch_grant) state_next = S7;
        else if (_w_ch_grant && !aw_ready) state_next = S6;
        else if (aw_ready && !_w_ch_grant) state_next = S3;
      end
      S3: begin
        if (_w_ch_grant) state_next = S8;
        else if (1'b1 && !_w_ch_grant) state_next = S4;
      end
      S4: begin
        if (_w_ch_grant) state_next = S8;
      end
      S5: begin
        if (_aw_ch_grant) state_next = S10;
        else if (1'b1 && !_aw_ch_grant) state_next = S9;
      end
      S6: begin
        if (aw_ready) state_next = S11;
        else if (1'b1 && !aw_ready) state_next = S10;
      end
      S7: begin
        state_next = S12;
      end
      S8: begin
        state_next = S12;
      end
      S9: begin
        if (_aw_ch_grant && w_ready && pop_valid) state_next = S14;
        else if (w_ready && pop_valid && !_aw_ch_grant) state_next = S13;
        else if (_aw_ch_grant && !(w_ready && pop_valid)) state_next = S10;
      end
      S10: begin
        if (aw_ready && w_ready && pop_valid) state_next = S15;
        else if (w_ready && pop_valid && !aw_ready) state_next = S14;
        else if (aw_ready && !(w_ready && pop_valid)) state_next = S11;
      end
      S11: begin
        if (w_ready && pop_valid) state_next = S16;
        else if (1'b1 && !(w_ready && pop_valid)) state_next = S12;
      end
      S12: begin
        if (w_ready && pop_valid) state_next = S16;
      end
      S13: begin
        if (_aw_ch_grant) state_next = S18;
        else if (1'b1 && !_aw_ch_grant) state_next = S17;
      end
      S14: begin
        if (aw_ready) state_next = S19;
        else if (1'b1 && !aw_ready) state_next = S18;
      end
      S15: begin
        state_next = S20;
      end
      S16: begin
        state_next = S20;
      end
      S17: begin
        if (_aw_ch_grant) state_next = S22;
        else if (1'b1 && !_aw_ch_grant) state_next = S21;
      end
      S18: begin
        if (aw_ready) state_next = S23;
        else if (1'b1 && !aw_ready) state_next = S22;
      end
      S19: begin
        state_next = S24;
      end
      S20: begin
        state_next = S24;
      end
      S21: begin
        if (_aw_ch_grant) state_next = S22;
      end
      S22: begin
        if (aw_ready) state_next = S23;
      end
      S23: begin
        state_next = S24;
      end
      S24: begin
        state_next = S25;
      end
      S25: begin
        if (b_valid && b_id == 1) state_next = S26;
      end
      S26: begin
        state_next = S27;
      end
      S27: begin
        if (_done_ch_grant) state_next = S28;
      end
      S28: begin
        if (_cnt == 0) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
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
    _aw_ch_req = 0;
    _w_ch_req = 0;
    _done_ch_req = 0;
    case (state_r)
      S0: begin
      end
      S1: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
      end
      S2: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 1;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
      end
      S3: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
      end
      S4: begin
        _w_ch_req = 1;
      end
      S5: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
      end
      S6: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 1;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
      end
      S7: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
      end
      S8: begin
        _w_ch_req = 1;
      end
      S9: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S10: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 1;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S11: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S12: begin
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S13: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
      end
      S14: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 1;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
      end
      S15: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
      end
      S16: begin
        _w_ch_req = 1;
      end
      S17: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S18: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 1;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S19: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S20: begin
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S21: begin
        _aw_ch_req = 1;
      end
      S22: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 1;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      S23: begin
        _aw_ch_req = 1;
        aw_valid = 0;
      end
      S24: begin
      end
      S25: begin
        b_ready = 1;
      end
      S26: begin
        b_ready = 0;
      end
      S27: begin
        _done_ch_req = 1;
      end
      S28: begin
        _done_ch_req = 1;
      end
      default: ;
    endcase
  end

endmodule

module _ThreadS2mm_WriteReq_2 (
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
  output logic [32-1:0] next_addr_r,
  output logic _w_ch_req,
  input logic _w_ch_grant,
  output logic _done_ch_req,
  input logic _done_ch_grant,
  output logic _aw_ch_req,
  input logic _aw_ch_grant
);

  typedef enum logic [4:0] {
    S0 = 5'd0,
    S1 = 5'd1,
    S2 = 5'd2,
    S3 = 5'd3,
    S4 = 5'd4,
    S5 = 5'd5,
    S6 = 5'd6,
    S7 = 5'd7,
    S8 = 5'd8,
    S9 = 5'd9,
    S10 = 5'd10,
    S11 = 5'd11,
    S12 = 5'd12,
    S13 = 5'd13,
    S14 = 5'd14,
    S15 = 5'd15,
    S16 = 5'd16,
    S17 = 5'd17,
    S18 = 5'd18,
    S19 = 5'd19,
    S20 = 5'd20,
    S21 = 5'd21,
    S22 = 5'd22,
    S23 = 5'd23,
    S24 = 5'd24,
    S25 = 5'd25,
    S26 = 5'd26,
    S27 = 5'd27,
    S28 = 5'd28
  } _ThreadS2mm_WriteReq_2_state_t;
  
  _ThreadS2mm_WriteReq_2_state_t state_r, state_next;
  
  logic [32-1:0] _cnt;
  logic [32-1:0] _loop_cnt;
  
  always_ff @(posedge clk) begin
    if ((!rst)) begin
      state_r <= S0;
      _cnt <= 0;
      _loop_cnt <= 0;
      aw_issued_r <= 0;
      b_received_r <= 0;
      next_addr_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S2: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S5: begin
          _loop_cnt <= 0;
        end
        S6: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
          _loop_cnt <= 0;
        end
        S7: begin
          _loop_cnt <= 0;
        end
        S8: begin
          _loop_cnt <= 0;
        end
        S10: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S13: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S14: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
          _loop_cnt <= _loop_cnt + 1;
        end
        S15: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S16: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S18: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S22: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S27: begin
          if (_done_ch_grant) begin
            _cnt <= 1 - 1;
          end
        end
        S28: begin
          if (_cnt == 0) begin
            b_received_r <= b_received_r + 1;
          end
          _cnt <= _cnt - 1;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S0: begin
        if (start) state_next = S1;
      end
      S1: begin
        if (_aw_ch_grant && _w_ch_grant) state_next = S6;
        else if (_w_ch_grant && !_aw_ch_grant) state_next = S5;
        else if (_aw_ch_grant && !_w_ch_grant) state_next = S2;
      end
      S2: begin
        if (aw_ready && _w_ch_grant) state_next = S7;
        else if (_w_ch_grant && !aw_ready) state_next = S6;
        else if (aw_ready && !_w_ch_grant) state_next = S3;
      end
      S3: begin
        if (_w_ch_grant) state_next = S8;
        else if (1'b1 && !_w_ch_grant) state_next = S4;
      end
      S4: begin
        if (_w_ch_grant) state_next = S8;
      end
      S5: begin
        if (_aw_ch_grant) state_next = S10;
        else if (1'b1 && !_aw_ch_grant) state_next = S9;
      end
      S6: begin
        if (aw_ready) state_next = S11;
        else if (1'b1 && !aw_ready) state_next = S10;
      end
      S7: begin
        state_next = S12;
      end
      S8: begin
        state_next = S12;
      end
      S9: begin
        if (_aw_ch_grant && w_ready && pop_valid) state_next = S14;
        else if (w_ready && pop_valid && !_aw_ch_grant) state_next = S13;
        else if (_aw_ch_grant && !(w_ready && pop_valid)) state_next = S10;
      end
      S10: begin
        if (aw_ready && w_ready && pop_valid) state_next = S15;
        else if (w_ready && pop_valid && !aw_ready) state_next = S14;
        else if (aw_ready && !(w_ready && pop_valid)) state_next = S11;
      end
      S11: begin
        if (w_ready && pop_valid) state_next = S16;
        else if (1'b1 && !(w_ready && pop_valid)) state_next = S12;
      end
      S12: begin
        if (w_ready && pop_valid) state_next = S16;
      end
      S13: begin
        if (_aw_ch_grant) state_next = S18;
        else if (1'b1 && !_aw_ch_grant) state_next = S17;
      end
      S14: begin
        if (aw_ready) state_next = S19;
        else if (1'b1 && !aw_ready) state_next = S18;
      end
      S15: begin
        state_next = S20;
      end
      S16: begin
        state_next = S20;
      end
      S17: begin
        if (_aw_ch_grant) state_next = S22;
        else if (1'b1 && !_aw_ch_grant) state_next = S21;
      end
      S18: begin
        if (aw_ready) state_next = S23;
        else if (1'b1 && !aw_ready) state_next = S22;
      end
      S19: begin
        state_next = S24;
      end
      S20: begin
        state_next = S24;
      end
      S21: begin
        if (_aw_ch_grant) state_next = S22;
      end
      S22: begin
        if (aw_ready) state_next = S23;
      end
      S23: begin
        state_next = S24;
      end
      S24: begin
        state_next = S25;
      end
      S25: begin
        if (b_valid && b_id == 2) state_next = S26;
      end
      S26: begin
        state_next = S27;
      end
      S27: begin
        if (_done_ch_grant) state_next = S28;
      end
      S28: begin
        if (_cnt == 0) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
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
    _w_ch_req = 0;
    _done_ch_req = 0;
    _aw_ch_req = 0;
    case (state_r)
      S0: begin
      end
      S1: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
      end
      S2: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 2;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
      end
      S3: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
      end
      S4: begin
        _w_ch_req = 1;
      end
      S5: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
      end
      S6: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 2;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
      end
      S7: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
      end
      S8: begin
        _w_ch_req = 1;
      end
      S9: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S10: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 2;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S11: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S12: begin
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S13: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
      end
      S14: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 2;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
      end
      S15: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
      end
      S16: begin
        _w_ch_req = 1;
      end
      S17: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S18: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 2;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S19: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S20: begin
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S21: begin
        _aw_ch_req = 1;
      end
      S22: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 2;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      S23: begin
        _aw_ch_req = 1;
        aw_valid = 0;
      end
      S24: begin
      end
      S25: begin
        b_ready = 1;
      end
      S26: begin
        b_ready = 0;
      end
      S27: begin
        _done_ch_req = 1;
      end
      S28: begin
        _done_ch_req = 1;
      end
      default: ;
    endcase
  end

endmodule

module _ThreadS2mm_WriteReq_3 (
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
  output logic [32-1:0] next_addr_r,
  output logic _w_ch_req,
  input logic _w_ch_grant,
  output logic _done_ch_req,
  input logic _done_ch_grant,
  output logic _aw_ch_req,
  input logic _aw_ch_grant
);

  typedef enum logic [4:0] {
    S0 = 5'd0,
    S1 = 5'd1,
    S2 = 5'd2,
    S3 = 5'd3,
    S4 = 5'd4,
    S5 = 5'd5,
    S6 = 5'd6,
    S7 = 5'd7,
    S8 = 5'd8,
    S9 = 5'd9,
    S10 = 5'd10,
    S11 = 5'd11,
    S12 = 5'd12,
    S13 = 5'd13,
    S14 = 5'd14,
    S15 = 5'd15,
    S16 = 5'd16,
    S17 = 5'd17,
    S18 = 5'd18,
    S19 = 5'd19,
    S20 = 5'd20,
    S21 = 5'd21,
    S22 = 5'd22,
    S23 = 5'd23,
    S24 = 5'd24,
    S25 = 5'd25,
    S26 = 5'd26,
    S27 = 5'd27,
    S28 = 5'd28
  } _ThreadS2mm_WriteReq_3_state_t;
  
  _ThreadS2mm_WriteReq_3_state_t state_r, state_next;
  
  logic [32-1:0] _cnt;
  logic [32-1:0] _loop_cnt;
  
  always_ff @(posedge clk) begin
    if ((!rst)) begin
      state_r <= S0;
      _cnt <= 0;
      _loop_cnt <= 0;
      aw_issued_r <= 0;
      b_received_r <= 0;
      next_addr_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S2: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S5: begin
          _loop_cnt <= 0;
        end
        S6: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
          _loop_cnt <= 0;
        end
        S7: begin
          _loop_cnt <= 0;
        end
        S8: begin
          _loop_cnt <= 0;
        end
        S10: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S13: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S14: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
          _loop_cnt <= _loop_cnt + 1;
        end
        S15: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S16: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S18: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S22: begin
          if (aw_ready) begin
            aw_issued_r <= aw_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S27: begin
          if (_done_ch_grant) begin
            _cnt <= 1 - 1;
          end
        end
        S28: begin
          if (_cnt == 0) begin
            b_received_r <= b_received_r + 1;
          end
          _cnt <= _cnt - 1;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S0: begin
        if (start) state_next = S1;
      end
      S1: begin
        if (_aw_ch_grant && _w_ch_grant) state_next = S6;
        else if (_w_ch_grant && !_aw_ch_grant) state_next = S5;
        else if (_aw_ch_grant && !_w_ch_grant) state_next = S2;
      end
      S2: begin
        if (aw_ready && _w_ch_grant) state_next = S7;
        else if (_w_ch_grant && !aw_ready) state_next = S6;
        else if (aw_ready && !_w_ch_grant) state_next = S3;
      end
      S3: begin
        if (_w_ch_grant) state_next = S8;
        else if (1'b1 && !_w_ch_grant) state_next = S4;
      end
      S4: begin
        if (_w_ch_grant) state_next = S8;
      end
      S5: begin
        if (_aw_ch_grant) state_next = S10;
        else if (1'b1 && !_aw_ch_grant) state_next = S9;
      end
      S6: begin
        if (aw_ready) state_next = S11;
        else if (1'b1 && !aw_ready) state_next = S10;
      end
      S7: begin
        state_next = S12;
      end
      S8: begin
        state_next = S12;
      end
      S9: begin
        if (_aw_ch_grant && w_ready && pop_valid) state_next = S14;
        else if (w_ready && pop_valid && !_aw_ch_grant) state_next = S13;
        else if (_aw_ch_grant && !(w_ready && pop_valid)) state_next = S10;
      end
      S10: begin
        if (aw_ready && w_ready && pop_valid) state_next = S15;
        else if (w_ready && pop_valid && !aw_ready) state_next = S14;
        else if (aw_ready && !(w_ready && pop_valid)) state_next = S11;
      end
      S11: begin
        if (w_ready && pop_valid) state_next = S16;
        else if (1'b1 && !(w_ready && pop_valid)) state_next = S12;
      end
      S12: begin
        if (w_ready && pop_valid) state_next = S16;
      end
      S13: begin
        if (_aw_ch_grant) state_next = S18;
        else if (1'b1 && !_aw_ch_grant) state_next = S17;
      end
      S14: begin
        if (aw_ready) state_next = S19;
        else if (1'b1 && !aw_ready) state_next = S18;
      end
      S15: begin
        state_next = S20;
      end
      S16: begin
        state_next = S20;
      end
      S17: begin
        if (_aw_ch_grant) state_next = S22;
        else if (1'b1 && !_aw_ch_grant) state_next = S21;
      end
      S18: begin
        if (aw_ready) state_next = S23;
        else if (1'b1 && !aw_ready) state_next = S22;
      end
      S19: begin
        state_next = S24;
      end
      S20: begin
        state_next = S24;
      end
      S21: begin
        if (_aw_ch_grant) state_next = S22;
      end
      S22: begin
        if (aw_ready) state_next = S23;
      end
      S23: begin
        state_next = S24;
      end
      S24: begin
        state_next = S25;
      end
      S25: begin
        if (b_valid && b_id == 3) state_next = S26;
      end
      S26: begin
        state_next = S27;
      end
      S27: begin
        if (_done_ch_grant) state_next = S28;
      end
      S28: begin
        if (_cnt == 0) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
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
    _w_ch_req = 0;
    _done_ch_req = 0;
    _aw_ch_req = 0;
    case (state_r)
      S0: begin
      end
      S1: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
      end
      S2: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 3;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
      end
      S3: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
      end
      S4: begin
        _w_ch_req = 1;
      end
      S5: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
      end
      S6: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 3;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
      end
      S7: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
      end
      S8: begin
        _w_ch_req = 1;
      end
      S9: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S10: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 3;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S11: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S12: begin
        _w_ch_req = 1;
        w_valid = 1;
        w_data = pop_data;
        w_strb = 4'd15;
        w_last = _loop_cnt == 8'(burst_len - 1);
        pop_ready = 1;
      end
      S13: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
      end
      S14: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 3;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
      end
      S15: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
      end
      S16: begin
        _w_ch_req = 1;
      end
      S17: begin
        _aw_ch_req = 1;
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S18: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 3;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S19: begin
        _aw_ch_req = 1;
        aw_valid = 0;
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S20: begin
        _w_ch_req = 1;
        w_valid = 0;
        pop_ready = 0;
      end
      S21: begin
        _aw_ch_req = 1;
      end
      S22: begin
        _aw_ch_req = 1;
        aw_valid = 1;
        aw_addr = next_addr_r;
        aw_id = 3;
        aw_len = 8'(burst_len - 1);
        aw_size = 3'd2;
        aw_burst = 2'd1;
      end
      S23: begin
        _aw_ch_req = 1;
        aw_valid = 0;
      end
      S24: begin
      end
      S25: begin
        b_ready = 1;
      end
      S26: begin
        b_ready = 0;
      end
      S27: begin
        _done_ch_req = 1;
      end
      S28: begin
        _done_ch_req = 1;
      end
      default: ;
    endcase
  end

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

  logic [16-1:0] b_received_r__t3;
  logic [16-1:0] b_received_r__t2;
  logic [16-1:0] b_received_r__t1;
  logic [16-1:0] b_received_r__t0;
  logic [16-1:0] aw_issued_r__t3;
  logic [16-1:0] aw_issued_r__t2;
  logic [16-1:0] aw_issued_r__t1;
  logic [16-1:0] aw_issued_r__t0;
  logic [8-1:0] aw_len__t3;
  logic [8-1:0] aw_len__t2;
  logic [8-1:0] aw_len__t1;
  logic [8-1:0] aw_len__t0;
  logic [3-1:0] aw_size__t3;
  logic [3-1:0] aw_size__t2;
  logic [3-1:0] aw_size__t1;
  logic [3-1:0] aw_size__t0;
  logic w_valid__t3;
  logic w_valid__t2;
  logic w_valid__t1;
  logic w_valid__t0;
  logic [4-1:0] w_strb__t3;
  logic [4-1:0] w_strb__t2;
  logic [4-1:0] w_strb__t1;
  logic [4-1:0] w_strb__t0;
  logic w_last__t3;
  logic w_last__t2;
  logic w_last__t1;
  logic w_last__t0;
  logic pop_ready__t3;
  logic pop_ready__t2;
  logic pop_ready__t1;
  logic pop_ready__t0;
  logic [2-1:0] aw_id__t3;
  logic [2-1:0] aw_id__t2;
  logic [2-1:0] aw_id__t1;
  logic [2-1:0] aw_id__t0;
  logic [32-1:0] w_data__t3;
  logic [32-1:0] w_data__t2;
  logic [32-1:0] w_data__t1;
  logic [32-1:0] w_data__t0;
  logic [32-1:0] aw_addr__t3;
  logic [32-1:0] aw_addr__t2;
  logic [32-1:0] aw_addr__t1;
  logic [32-1:0] aw_addr__t0;
  logic [2-1:0] aw_burst__t3;
  logic [2-1:0] aw_burst__t2;
  logic [2-1:0] aw_burst__t1;
  logic [2-1:0] aw_burst__t0;
  logic [32-1:0] next_addr_r__t3;
  logic [32-1:0] next_addr_r__t2;
  logic [32-1:0] next_addr_r__t1;
  logic [32-1:0] next_addr_r__t0;
  logic aw_valid__t3;
  logic aw_valid__t2;
  logic aw_valid__t1;
  logic aw_valid__t0;
  logic b_ready__t3;
  logic b_ready__t2;
  logic b_ready__t1;
  logic b_ready__t0;
  logic _done_ch_req_3;
  logic _done_ch_req_2;
  logic _done_ch_req_1;
  logic _done_ch_req_0;
  logic _aw_ch_req_3;
  logic _aw_ch_req_2;
  logic _aw_ch_req_1;
  logic _aw_ch_req_0;
  logic _w_ch_req_3;
  logic _w_ch_req_2;
  logic _w_ch_req_1;
  logic _w_ch_req_0;
  logic [16-1:0] aw_issued_r;
  logic [16-1:0] b_received_r;
  logic [32-1:0] next_addr_r;
  assign halted = 1'b0;
  assign idle_out = aw_issued_r == 0 && b_received_r == 0;
  assign done = b_received_r == total_xfers && b_received_r != 0;
  _ThreadS2mm_WriteReq_0 _WriteReq_0 (
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
    .aw_addr(aw_addr__t0),
    .aw_burst(aw_burst__t0),
    .aw_id(aw_id__t0),
    .aw_len(aw_len__t0),
    .aw_size(aw_size__t0),
    .aw_valid(aw_valid__t0),
    .b_ready(b_ready__t0),
    .pop_ready(pop_ready__t0),
    .w_data(w_data__t0),
    .w_last(w_last__t0),
    .w_strb(w_strb__t0),
    .w_valid(w_valid__t0),
    .aw_issued_r(aw_issued_r__t0),
    .b_received_r(b_received_r__t0),
    .next_addr_r(next_addr_r__t0),
    ._aw_ch_req(_aw_ch_req_0),
    ._aw_ch_grant(_aw_ch_grant_0),
    ._w_ch_req(_w_ch_req_0),
    ._w_ch_grant(_w_ch_grant_0),
    ._done_ch_req(_done_ch_req_0),
    ._done_ch_grant(_done_ch_grant_0)
  );
  _ThreadS2mm_WriteReq_1 _WriteReq_1 (
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
    .aw_addr(aw_addr__t1),
    .aw_burst(aw_burst__t1),
    .aw_id(aw_id__t1),
    .aw_len(aw_len__t1),
    .aw_size(aw_size__t1),
    .aw_valid(aw_valid__t1),
    .b_ready(b_ready__t1),
    .pop_ready(pop_ready__t1),
    .w_data(w_data__t1),
    .w_last(w_last__t1),
    .w_strb(w_strb__t1),
    .w_valid(w_valid__t1),
    .aw_issued_r(aw_issued_r__t1),
    .b_received_r(b_received_r__t1),
    .next_addr_r(next_addr_r__t1),
    ._aw_ch_req(_aw_ch_req_1),
    ._aw_ch_grant(_aw_ch_grant_1),
    ._w_ch_req(_w_ch_req_1),
    ._w_ch_grant(_w_ch_grant_1),
    ._done_ch_req(_done_ch_req_1),
    ._done_ch_grant(_done_ch_grant_1)
  );
  _ThreadS2mm_WriteReq_2 _WriteReq_2 (
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
    .aw_addr(aw_addr__t2),
    .aw_burst(aw_burst__t2),
    .aw_id(aw_id__t2),
    .aw_len(aw_len__t2),
    .aw_size(aw_size__t2),
    .aw_valid(aw_valid__t2),
    .b_ready(b_ready__t2),
    .pop_ready(pop_ready__t2),
    .w_data(w_data__t2),
    .w_last(w_last__t2),
    .w_strb(w_strb__t2),
    .w_valid(w_valid__t2),
    .aw_issued_r(aw_issued_r__t2),
    .b_received_r(b_received_r__t2),
    .next_addr_r(next_addr_r__t2),
    ._w_ch_req(_w_ch_req_2),
    ._w_ch_grant(_w_ch_grant_2),
    ._done_ch_req(_done_ch_req_2),
    ._done_ch_grant(_done_ch_grant_2),
    ._aw_ch_req(_aw_ch_req_2),
    ._aw_ch_grant(_aw_ch_grant_2)
  );
  _ThreadS2mm_WriteReq_3 _WriteReq_3 (
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
    .aw_addr(aw_addr__t3),
    .aw_burst(aw_burst__t3),
    .aw_id(aw_id__t3),
    .aw_len(aw_len__t3),
    .aw_size(aw_size__t3),
    .aw_valid(aw_valid__t3),
    .b_ready(b_ready__t3),
    .pop_ready(pop_ready__t3),
    .w_data(w_data__t3),
    .w_last(w_last__t3),
    .w_strb(w_strb__t3),
    .w_valid(w_valid__t3),
    .aw_issued_r(aw_issued_r__t3),
    .b_received_r(b_received_r__t3),
    .next_addr_r(next_addr_r__t3),
    ._w_ch_req(_w_ch_req_3),
    ._w_ch_grant(_w_ch_grant_3),
    ._done_ch_req(_done_ch_req_3),
    ._done_ch_grant(_done_ch_grant_3),
    ._aw_ch_req(_aw_ch_req_3),
    ._aw_ch_grant(_aw_ch_grant_3)
  );
  logic _aw_ch_grant_0;
  logic _aw_ch_grant_1;
  logic _aw_ch_grant_2;
  logic _aw_ch_grant_3;
  logic [2-1:0] _aw_ch_last_grant = 3;
  assign _aw_ch_grant_0 = _aw_ch_req_0;
  assign _aw_ch_grant_1 = _aw_ch_req_1 && !_aw_ch_grant_0;
  assign _aw_ch_grant_2 = _aw_ch_req_2 && !_aw_ch_grant_0 && !_aw_ch_grant_1;
  assign _aw_ch_grant_3 = _aw_ch_req_3 && !_aw_ch_grant_0 && !_aw_ch_grant_1 && !_aw_ch_grant_2;
  always_ff @(posedge clk) begin
    if (_aw_ch_grant_0) begin
      _aw_ch_last_grant <= 0;
    end
    if (_aw_ch_grant_1) begin
      _aw_ch_last_grant <= 1;
    end
    if (_aw_ch_grant_2) begin
      _aw_ch_last_grant <= 2;
    end
    if (_aw_ch_grant_3) begin
      _aw_ch_last_grant <= 3;
    end
  end
  logic _w_ch_grant_0;
  logic _w_ch_grant_1;
  logic _w_ch_grant_2;
  logic _w_ch_grant_3;
  logic [2-1:0] _w_ch_last_grant = 3;
  assign _w_ch_grant_0 = _w_ch_req_0;
  assign _w_ch_grant_1 = _w_ch_req_1 && !_w_ch_grant_0;
  assign _w_ch_grant_2 = _w_ch_req_2 && !_w_ch_grant_0 && !_w_ch_grant_1;
  assign _w_ch_grant_3 = _w_ch_req_3 && !_w_ch_grant_0 && !_w_ch_grant_1 && !_w_ch_grant_2;
  always_ff @(posedge clk) begin
    if (_w_ch_grant_0) begin
      _w_ch_last_grant <= 0;
    end
    if (_w_ch_grant_1) begin
      _w_ch_last_grant <= 1;
    end
    if (_w_ch_grant_2) begin
      _w_ch_last_grant <= 2;
    end
    if (_w_ch_grant_3) begin
      _w_ch_last_grant <= 3;
    end
  end
  logic _done_ch_grant_0;
  logic _done_ch_grant_1;
  logic _done_ch_grant_2;
  logic _done_ch_grant_3;
  assign _done_ch_grant_0 = _done_ch_req_0;
  assign _done_ch_grant_1 = _done_ch_req_1 && !_done_ch_grant_0;
  assign _done_ch_grant_2 = _done_ch_req_2 && !_done_ch_grant_0 && !_done_ch_grant_1;
  assign _done_ch_grant_3 = _done_ch_req_3 && !_done_ch_grant_0 && !_done_ch_grant_1 && !_done_ch_grant_2;
  assign b_ready = b_ready__t0 | b_ready__t1 | b_ready__t2 | b_ready__t3;
  assign aw_valid = aw_valid__t0 | aw_valid__t1 | aw_valid__t2 | aw_valid__t3;
  assign next_addr_r = next_addr_r__t0 | next_addr_r__t1 | next_addr_r__t2 | next_addr_r__t3;
  assign aw_burst = aw_burst__t0 | aw_burst__t1 | aw_burst__t2 | aw_burst__t3;
  assign aw_addr = aw_addr__t0 | aw_addr__t1 | aw_addr__t2 | aw_addr__t3;
  assign w_data = w_data__t0 | w_data__t1 | w_data__t2 | w_data__t3;
  assign aw_id = aw_id__t0 | aw_id__t1 | aw_id__t2 | aw_id__t3;
  assign pop_ready = pop_ready__t0 | pop_ready__t1 | pop_ready__t2 | pop_ready__t3;
  assign w_last = w_last__t0 | w_last__t1 | w_last__t2 | w_last__t3;
  assign w_strb = w_strb__t0 | w_strb__t1 | w_strb__t2 | w_strb__t3;
  assign w_valid = w_valid__t0 | w_valid__t1 | w_valid__t2 | w_valid__t3;
  assign aw_size = aw_size__t0 | aw_size__t1 | aw_size__t2 | aw_size__t3;
  assign aw_len = aw_len__t0 | aw_len__t1 | aw_len__t2 | aw_len__t3;
  assign aw_issued_r = aw_issued_r__t0 | aw_issued_r__t1 | aw_issued_r__t2 | aw_issued_r__t3;
  assign b_received_r = b_received_r__t0 | b_received_r__t1 | b_received_r__t2 | b_received_r__t3;

endmodule

