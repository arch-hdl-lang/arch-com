// Thread-based multi-outstanding MM2S read engine.
// Compare with FsmMm2sMulti.arch (165 lines, 3 states, manual tracking).
// Shared state — all written only inside locks (mutual exclusion)
// done/idle are combinational from shared counters
module _ThreadMm2s_ReadReq_0 (
  input logic clk,
  input logic rst,
  input logic ar_ready,
  input logic [32-1:0] base_addr,
  input logic [8-1:0] burst_len,
  input logic push_ready,
  input logic [32-1:0] r_data,
  input logic [2-1:0] r_id,
  input logic r_valid,
  input logic start,
  output logic [32-1:0] ar_addr,
  output logic [2-1:0] ar_burst,
  output logic [2-1:0] ar_id,
  output logic [8-1:0] ar_len,
  output logic [3-1:0] ar_size,
  output logic ar_valid,
  output logic [32-1:0] push_data,
  output logic push_valid,
  output logic r_ready,
  output logic [32-1:0] next_addr_r,
  output logic [16-1:0] xfers_complete_r,
  output logic [16-1:0] xfers_issued_r,
  output logic _done_ch_req,
  input logic _done_ch_grant,
  output logic _push_ch_req,
  input logic _push_ch_grant,
  output logic _ar_ch_req,
  input logic _ar_ch_grant
);

  typedef enum logic [3:0] {
    S0 = 4'd0,
    S1 = 4'd1,
    S2 = 4'd2,
    S3 = 4'd3,
    S4 = 4'd4,
    S5 = 4'd5,
    S6 = 4'd6,
    S7 = 4'd7,
    S8 = 4'd8,
    S9 = 4'd9,
    S10 = 4'd10,
    S11 = 4'd11,
    S12 = 4'd12
  } _ThreadMm2s_ReadReq_0_state_t;
  
  _ThreadMm2s_ReadReq_0_state_t state_r, state_next;
  
  logic [32-1:0] _cnt;
  logic [32-1:0] _loop_cnt;
  
  always_ff @(posedge clk) begin
    if ((!rst)) begin
      state_r <= S0;
      _cnt <= 0;
      _loop_cnt <= 0;
      next_addr_r <= 0;
      xfers_complete_r <= 0;
      xfers_issued_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S2: begin
          if (ar_ready) begin
            // Wait for start pulse — first thread to see it initializes counters
            // First thread resets counters (xfers_issued == 0 means fresh start)
            if (xfers_issued_r == 0) begin
              next_addr_r <= base_addr;
            end
            // Claim next xfer
            xfers_issued_r <= xfers_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S4: begin
          // Collect R beats
          _loop_cnt <= 0;
        end
        S9: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S11: begin
          if (_done_ch_grant) begin
            _cnt <= 1 - 1;
          end
        end
        S12: begin
          if (_cnt == 0) begin
            // Mark complete
            xfers_complete_r <= xfers_complete_r + 1;
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
        if (_ar_ch_grant) state_next = S2;
      end
      S2: begin
        if (ar_ready) state_next = S3;
      end
      S3: begin
        state_next = S4;
      end
      S4: begin
        state_next = S5;
      end
      S5: begin
        if (r_valid && r_id == 0) state_next = S6;
      end
      S6: begin
        if (_push_ch_grant) state_next = S7;
      end
      S7: begin
        if (push_ready) state_next = S8;
      end
      S8: begin
        state_next = S9;
      end
      S9: begin
        if (_loop_cnt < burst_len - 1) state_next = S5;
        else if (_loop_cnt >= burst_len - 1) state_next = S10;
      end
      S10: begin
        state_next = S11;
      end
      S11: begin
        if (_done_ch_grant) state_next = S12;
      end
      S12: begin
        if (_cnt == 0) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
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
    _done_ch_req = 0;
    _push_ch_req = 0;
    _ar_ch_req = 0;
    case (state_r)
      S0: begin
      end
      S1: begin
        _ar_ch_req = 1;
      end
      S2: begin
        _ar_ch_req = 1;
        ar_valid = 1;
        ar_addr = next_addr_r;
        ar_id = 0;
        ar_len = 8'(burst_len - 1);
        ar_size = 3'd2;
        ar_burst = 2'd1;
      end
      S3: begin
        _ar_ch_req = 1;
        ar_valid = 0;
      end
      S4: begin
      end
      S5: begin
        r_ready = 1;
      end
      S6: begin
        _push_ch_req = 1;
      end
      S7: begin
        _push_ch_req = 1;
        push_valid = 1;
        push_data = r_data;
      end
      S8: begin
        _push_ch_req = 1;
        push_valid = 0;
      end
      S9: begin
      end
      S10: begin
        r_ready = 0;
      end
      S11: begin
        _done_ch_req = 1;
      end
      S12: begin
        _done_ch_req = 1;
      end
      default: ;
    endcase
  end

endmodule

module _ThreadMm2s_ReadReq_1 (
  input logic clk,
  input logic rst,
  input logic ar_ready,
  input logic [32-1:0] base_addr,
  input logic [8-1:0] burst_len,
  input logic push_ready,
  input logic [32-1:0] r_data,
  input logic [2-1:0] r_id,
  input logic r_valid,
  input logic start,
  output logic [32-1:0] ar_addr,
  output logic [2-1:0] ar_burst,
  output logic [2-1:0] ar_id,
  output logic [8-1:0] ar_len,
  output logic [3-1:0] ar_size,
  output logic ar_valid,
  output logic [32-1:0] push_data,
  output logic push_valid,
  output logic r_ready,
  output logic [32-1:0] next_addr_r,
  output logic [16-1:0] xfers_complete_r,
  output logic [16-1:0] xfers_issued_r,
  output logic _ar_ch_req,
  input logic _ar_ch_grant,
  output logic _push_ch_req,
  input logic _push_ch_grant,
  output logic _done_ch_req,
  input logic _done_ch_grant
);

  typedef enum logic [3:0] {
    S0 = 4'd0,
    S1 = 4'd1,
    S2 = 4'd2,
    S3 = 4'd3,
    S4 = 4'd4,
    S5 = 4'd5,
    S6 = 4'd6,
    S7 = 4'd7,
    S8 = 4'd8,
    S9 = 4'd9,
    S10 = 4'd10,
    S11 = 4'd11,
    S12 = 4'd12
  } _ThreadMm2s_ReadReq_1_state_t;
  
  _ThreadMm2s_ReadReq_1_state_t state_r, state_next;
  
  logic [32-1:0] _cnt;
  logic [32-1:0] _loop_cnt;
  
  always_ff @(posedge clk) begin
    if ((!rst)) begin
      state_r <= S0;
      _cnt <= 0;
      _loop_cnt <= 0;
      next_addr_r <= 0;
      xfers_complete_r <= 0;
      xfers_issued_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S2: begin
          if (ar_ready) begin
            if (xfers_issued_r == 0) begin
              next_addr_r <= base_addr;
            end
            xfers_issued_r <= xfers_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S4: begin
          _loop_cnt <= 0;
        end
        S9: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S11: begin
          if (_done_ch_grant) begin
            _cnt <= 1 - 1;
          end
        end
        S12: begin
          if (_cnt == 0) begin
            xfers_complete_r <= xfers_complete_r + 1;
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
        if (_ar_ch_grant) state_next = S2;
      end
      S2: begin
        if (ar_ready) state_next = S3;
      end
      S3: begin
        state_next = S4;
      end
      S4: begin
        state_next = S5;
      end
      S5: begin
        if (r_valid && r_id == 1) state_next = S6;
      end
      S6: begin
        if (_push_ch_grant) state_next = S7;
      end
      S7: begin
        if (push_ready) state_next = S8;
      end
      S8: begin
        state_next = S9;
      end
      S9: begin
        if (_loop_cnt < burst_len - 1) state_next = S5;
        else if (_loop_cnt >= burst_len - 1) state_next = S10;
      end
      S10: begin
        state_next = S11;
      end
      S11: begin
        if (_done_ch_grant) state_next = S12;
      end
      S12: begin
        if (_cnt == 0) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
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
    _ar_ch_req = 0;
    _push_ch_req = 0;
    _done_ch_req = 0;
    case (state_r)
      S0: begin
      end
      S1: begin
        _ar_ch_req = 1;
      end
      S2: begin
        _ar_ch_req = 1;
        ar_valid = 1;
        ar_addr = next_addr_r;
        ar_id = 1;
        ar_len = 8'(burst_len - 1);
        ar_size = 3'd2;
        ar_burst = 2'd1;
      end
      S3: begin
        _ar_ch_req = 1;
        ar_valid = 0;
      end
      S4: begin
      end
      S5: begin
        r_ready = 1;
      end
      S6: begin
        _push_ch_req = 1;
      end
      S7: begin
        _push_ch_req = 1;
        push_valid = 1;
        push_data = r_data;
      end
      S8: begin
        _push_ch_req = 1;
        push_valid = 0;
      end
      S9: begin
      end
      S10: begin
        r_ready = 0;
      end
      S11: begin
        _done_ch_req = 1;
      end
      S12: begin
        _done_ch_req = 1;
      end
      default: ;
    endcase
  end

endmodule

module _ThreadMm2s_ReadReq_2 (
  input logic clk,
  input logic rst,
  input logic ar_ready,
  input logic [32-1:0] base_addr,
  input logic [8-1:0] burst_len,
  input logic push_ready,
  input logic [32-1:0] r_data,
  input logic [2-1:0] r_id,
  input logic r_valid,
  input logic start,
  output logic [32-1:0] ar_addr,
  output logic [2-1:0] ar_burst,
  output logic [2-1:0] ar_id,
  output logic [8-1:0] ar_len,
  output logic [3-1:0] ar_size,
  output logic ar_valid,
  output logic [32-1:0] push_data,
  output logic push_valid,
  output logic r_ready,
  output logic [32-1:0] next_addr_r,
  output logic [16-1:0] xfers_complete_r,
  output logic [16-1:0] xfers_issued_r,
  output logic _push_ch_req,
  input logic _push_ch_grant,
  output logic _ar_ch_req,
  input logic _ar_ch_grant,
  output logic _done_ch_req,
  input logic _done_ch_grant
);

  typedef enum logic [3:0] {
    S0 = 4'd0,
    S1 = 4'd1,
    S2 = 4'd2,
    S3 = 4'd3,
    S4 = 4'd4,
    S5 = 4'd5,
    S6 = 4'd6,
    S7 = 4'd7,
    S8 = 4'd8,
    S9 = 4'd9,
    S10 = 4'd10,
    S11 = 4'd11,
    S12 = 4'd12
  } _ThreadMm2s_ReadReq_2_state_t;
  
  _ThreadMm2s_ReadReq_2_state_t state_r, state_next;
  
  logic [32-1:0] _cnt;
  logic [32-1:0] _loop_cnt;
  
  always_ff @(posedge clk) begin
    if ((!rst)) begin
      state_r <= S0;
      _cnt <= 0;
      _loop_cnt <= 0;
      next_addr_r <= 0;
      xfers_complete_r <= 0;
      xfers_issued_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S2: begin
          if (ar_ready) begin
            if (xfers_issued_r == 0) begin
              next_addr_r <= base_addr;
            end
            xfers_issued_r <= xfers_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S4: begin
          _loop_cnt <= 0;
        end
        S9: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S11: begin
          if (_done_ch_grant) begin
            _cnt <= 1 - 1;
          end
        end
        S12: begin
          if (_cnt == 0) begin
            xfers_complete_r <= xfers_complete_r + 1;
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
        if (_ar_ch_grant) state_next = S2;
      end
      S2: begin
        if (ar_ready) state_next = S3;
      end
      S3: begin
        state_next = S4;
      end
      S4: begin
        state_next = S5;
      end
      S5: begin
        if (r_valid && r_id == 2) state_next = S6;
      end
      S6: begin
        if (_push_ch_grant) state_next = S7;
      end
      S7: begin
        if (push_ready) state_next = S8;
      end
      S8: begin
        state_next = S9;
      end
      S9: begin
        if (_loop_cnt < burst_len - 1) state_next = S5;
        else if (_loop_cnt >= burst_len - 1) state_next = S10;
      end
      S10: begin
        state_next = S11;
      end
      S11: begin
        if (_done_ch_grant) state_next = S12;
      end
      S12: begin
        if (_cnt == 0) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
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
    _push_ch_req = 0;
    _ar_ch_req = 0;
    _done_ch_req = 0;
    case (state_r)
      S0: begin
      end
      S1: begin
        _ar_ch_req = 1;
      end
      S2: begin
        _ar_ch_req = 1;
        ar_valid = 1;
        ar_addr = next_addr_r;
        ar_id = 2;
        ar_len = 8'(burst_len - 1);
        ar_size = 3'd2;
        ar_burst = 2'd1;
      end
      S3: begin
        _ar_ch_req = 1;
        ar_valid = 0;
      end
      S4: begin
      end
      S5: begin
        r_ready = 1;
      end
      S6: begin
        _push_ch_req = 1;
      end
      S7: begin
        _push_ch_req = 1;
        push_valid = 1;
        push_data = r_data;
      end
      S8: begin
        _push_ch_req = 1;
        push_valid = 0;
      end
      S9: begin
      end
      S10: begin
        r_ready = 0;
      end
      S11: begin
        _done_ch_req = 1;
      end
      S12: begin
        _done_ch_req = 1;
      end
      default: ;
    endcase
  end

endmodule

module _ThreadMm2s_ReadReq_3 (
  input logic clk,
  input logic rst,
  input logic ar_ready,
  input logic [32-1:0] base_addr,
  input logic [8-1:0] burst_len,
  input logic push_ready,
  input logic [32-1:0] r_data,
  input logic [2-1:0] r_id,
  input logic r_valid,
  input logic start,
  output logic [32-1:0] ar_addr,
  output logic [2-1:0] ar_burst,
  output logic [2-1:0] ar_id,
  output logic [8-1:0] ar_len,
  output logic [3-1:0] ar_size,
  output logic ar_valid,
  output logic [32-1:0] push_data,
  output logic push_valid,
  output logic r_ready,
  output logic [32-1:0] next_addr_r,
  output logic [16-1:0] xfers_complete_r,
  output logic [16-1:0] xfers_issued_r,
  output logic _ar_ch_req,
  input logic _ar_ch_grant,
  output logic _push_ch_req,
  input logic _push_ch_grant,
  output logic _done_ch_req,
  input logic _done_ch_grant
);

  typedef enum logic [3:0] {
    S0 = 4'd0,
    S1 = 4'd1,
    S2 = 4'd2,
    S3 = 4'd3,
    S4 = 4'd4,
    S5 = 4'd5,
    S6 = 4'd6,
    S7 = 4'd7,
    S8 = 4'd8,
    S9 = 4'd9,
    S10 = 4'd10,
    S11 = 4'd11,
    S12 = 4'd12
  } _ThreadMm2s_ReadReq_3_state_t;
  
  _ThreadMm2s_ReadReq_3_state_t state_r, state_next;
  
  logic [32-1:0] _cnt;
  logic [32-1:0] _loop_cnt;
  
  always_ff @(posedge clk) begin
    if ((!rst)) begin
      state_r <= S0;
      _cnt <= 0;
      _loop_cnt <= 0;
      next_addr_r <= 0;
      xfers_complete_r <= 0;
      xfers_issued_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S2: begin
          if (ar_ready) begin
            if (xfers_issued_r == 0) begin
              next_addr_r <= base_addr;
            end
            xfers_issued_r <= xfers_issued_r + 1;
            next_addr_r <= next_addr_r + (32'($unsigned(burst_len)) << 2);
          end
        end
        S4: begin
          _loop_cnt <= 0;
        end
        S9: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        S11: begin
          if (_done_ch_grant) begin
            _cnt <= 1 - 1;
          end
        end
        S12: begin
          if (_cnt == 0) begin
            xfers_complete_r <= xfers_complete_r + 1;
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
        if (_ar_ch_grant) state_next = S2;
      end
      S2: begin
        if (ar_ready) state_next = S3;
      end
      S3: begin
        state_next = S4;
      end
      S4: begin
        state_next = S5;
      end
      S5: begin
        if (r_valid && r_id == 3) state_next = S6;
      end
      S6: begin
        if (_push_ch_grant) state_next = S7;
      end
      S7: begin
        if (push_ready) state_next = S8;
      end
      S8: begin
        state_next = S9;
      end
      S9: begin
        if (_loop_cnt < burst_len - 1) state_next = S5;
        else if (_loop_cnt >= burst_len - 1) state_next = S10;
      end
      S10: begin
        state_next = S11;
      end
      S11: begin
        if (_done_ch_grant) state_next = S12;
      end
      S12: begin
        if (_cnt == 0) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
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
    _ar_ch_req = 0;
    _push_ch_req = 0;
    _done_ch_req = 0;
    case (state_r)
      S0: begin
      end
      S1: begin
        _ar_ch_req = 1;
      end
      S2: begin
        _ar_ch_req = 1;
        ar_valid = 1;
        ar_addr = next_addr_r;
        ar_id = 3;
        ar_len = 8'(burst_len - 1);
        ar_size = 3'd2;
        ar_burst = 2'd1;
      end
      S3: begin
        _ar_ch_req = 1;
        ar_valid = 0;
      end
      S4: begin
      end
      S5: begin
        r_ready = 1;
      end
      S6: begin
        _push_ch_req = 1;
      end
      S7: begin
        _push_ch_req = 1;
        push_valid = 1;
        push_data = r_data;
      end
      S8: begin
        _push_ch_req = 1;
        push_valid = 0;
      end
      S9: begin
      end
      S10: begin
        r_ready = 0;
      end
      S11: begin
        _done_ch_req = 1;
      end
      S12: begin
        _done_ch_req = 1;
      end
      default: ;
    endcase
  end

endmodule

module ThreadMm2s #(
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

  logic [16-1:0] xfers_complete_r__t3;
  logic [16-1:0] xfers_complete_r__t2;
  logic [16-1:0] xfers_complete_r__t1;
  logic [16-1:0] xfers_complete_r__t0;
  logic [32-1:0] ar_addr__t3;
  logic [32-1:0] ar_addr__t2;
  logic [32-1:0] ar_addr__t1;
  logic [32-1:0] ar_addr__t0;
  logic push_valid__t3;
  logic push_valid__t2;
  logic push_valid__t1;
  logic push_valid__t0;
  logic [2-1:0] ar_id__t3;
  logic [2-1:0] ar_id__t2;
  logic [2-1:0] ar_id__t1;
  logic [2-1:0] ar_id__t0;
  logic [3-1:0] ar_size__t3;
  logic [3-1:0] ar_size__t2;
  logic [3-1:0] ar_size__t1;
  logic [3-1:0] ar_size__t0;
  logic [32-1:0] push_data__t3;
  logic [32-1:0] push_data__t2;
  logic [32-1:0] push_data__t1;
  logic [32-1:0] push_data__t0;
  logic [8-1:0] ar_len__t3;
  logic [8-1:0] ar_len__t2;
  logic [8-1:0] ar_len__t1;
  logic [8-1:0] ar_len__t0;
  logic [32-1:0] next_addr_r__t3;
  logic [32-1:0] next_addr_r__t2;
  logic [32-1:0] next_addr_r__t1;
  logic [32-1:0] next_addr_r__t0;
  logic [16-1:0] xfers_issued_r__t3;
  logic [16-1:0] xfers_issued_r__t2;
  logic [16-1:0] xfers_issued_r__t1;
  logic [16-1:0] xfers_issued_r__t0;
  logic ar_valid__t3;
  logic ar_valid__t2;
  logic ar_valid__t1;
  logic ar_valid__t0;
  logic [2-1:0] ar_burst__t3;
  logic [2-1:0] ar_burst__t2;
  logic [2-1:0] ar_burst__t1;
  logic [2-1:0] ar_burst__t0;
  logic r_ready__t3;
  logic r_ready__t2;
  logic r_ready__t1;
  logic r_ready__t0;
  logic _push_ch_req_3;
  logic _push_ch_req_2;
  logic _push_ch_req_1;
  logic _push_ch_req_0;
  logic _ar_ch_req_3;
  logic _ar_ch_req_2;
  logic _ar_ch_req_1;
  logic _ar_ch_req_0;
  logic _done_ch_req_3;
  logic _done_ch_req_2;
  logic _done_ch_req_1;
  logic _done_ch_req_0;
  logic [16-1:0] xfers_issued_r;
  logic [16-1:0] xfers_complete_r;
  logic [32-1:0] next_addr_r;
  assign halted = 1'b0;
  assign idle_out = xfers_issued_r == 0 && xfers_complete_r == 0;
  assign done = xfers_complete_r == total_xfers && xfers_complete_r != 0;
  _ThreadMm2s_ReadReq_0 _ReadReq_0 (
    .clk(clk),
    .rst(rst),
    .ar_ready(ar_ready),
    .base_addr(base_addr),
    .burst_len(burst_len),
    .push_ready(push_ready),
    .r_data(r_data),
    .r_id(r_id),
    .r_valid(r_valid),
    .start(start),
    .ar_addr(ar_addr__t0),
    .ar_burst(ar_burst__t0),
    .ar_id(ar_id__t0),
    .ar_len(ar_len__t0),
    .ar_size(ar_size__t0),
    .ar_valid(ar_valid__t0),
    .push_data(push_data__t0),
    .push_valid(push_valid__t0),
    .r_ready(r_ready__t0),
    .next_addr_r(next_addr_r__t0),
    .xfers_complete_r(xfers_complete_r__t0),
    .xfers_issued_r(xfers_issued_r__t0),
    ._done_ch_req(_done_ch_req_0),
    ._done_ch_grant(_done_ch_grant_0),
    ._push_ch_req(_push_ch_req_0),
    ._push_ch_grant(_push_ch_grant_0),
    ._ar_ch_req(_ar_ch_req_0),
    ._ar_ch_grant(_ar_ch_grant_0)
  );
  _ThreadMm2s_ReadReq_1 _ReadReq_1 (
    .clk(clk),
    .rst(rst),
    .ar_ready(ar_ready),
    .base_addr(base_addr),
    .burst_len(burst_len),
    .push_ready(push_ready),
    .r_data(r_data),
    .r_id(r_id),
    .r_valid(r_valid),
    .start(start),
    .ar_addr(ar_addr__t1),
    .ar_burst(ar_burst__t1),
    .ar_id(ar_id__t1),
    .ar_len(ar_len__t1),
    .ar_size(ar_size__t1),
    .ar_valid(ar_valid__t1),
    .push_data(push_data__t1),
    .push_valid(push_valid__t1),
    .r_ready(r_ready__t1),
    .next_addr_r(next_addr_r__t1),
    .xfers_complete_r(xfers_complete_r__t1),
    .xfers_issued_r(xfers_issued_r__t1),
    ._ar_ch_req(_ar_ch_req_1),
    ._ar_ch_grant(_ar_ch_grant_1),
    ._push_ch_req(_push_ch_req_1),
    ._push_ch_grant(_push_ch_grant_1),
    ._done_ch_req(_done_ch_req_1),
    ._done_ch_grant(_done_ch_grant_1)
  );
  _ThreadMm2s_ReadReq_2 _ReadReq_2 (
    .clk(clk),
    .rst(rst),
    .ar_ready(ar_ready),
    .base_addr(base_addr),
    .burst_len(burst_len),
    .push_ready(push_ready),
    .r_data(r_data),
    .r_id(r_id),
    .r_valid(r_valid),
    .start(start),
    .ar_addr(ar_addr__t2),
    .ar_burst(ar_burst__t2),
    .ar_id(ar_id__t2),
    .ar_len(ar_len__t2),
    .ar_size(ar_size__t2),
    .ar_valid(ar_valid__t2),
    .push_data(push_data__t2),
    .push_valid(push_valid__t2),
    .r_ready(r_ready__t2),
    .next_addr_r(next_addr_r__t2),
    .xfers_complete_r(xfers_complete_r__t2),
    .xfers_issued_r(xfers_issued_r__t2),
    ._push_ch_req(_push_ch_req_2),
    ._push_ch_grant(_push_ch_grant_2),
    ._ar_ch_req(_ar_ch_req_2),
    ._ar_ch_grant(_ar_ch_grant_2),
    ._done_ch_req(_done_ch_req_2),
    ._done_ch_grant(_done_ch_grant_2)
  );
  _ThreadMm2s_ReadReq_3 _ReadReq_3 (
    .clk(clk),
    .rst(rst),
    .ar_ready(ar_ready),
    .base_addr(base_addr),
    .burst_len(burst_len),
    .push_ready(push_ready),
    .r_data(r_data),
    .r_id(r_id),
    .r_valid(r_valid),
    .start(start),
    .ar_addr(ar_addr__t3),
    .ar_burst(ar_burst__t3),
    .ar_id(ar_id__t3),
    .ar_len(ar_len__t3),
    .ar_size(ar_size__t3),
    .ar_valid(ar_valid__t3),
    .push_data(push_data__t3),
    .push_valid(push_valid__t3),
    .r_ready(r_ready__t3),
    .next_addr_r(next_addr_r__t3),
    .xfers_complete_r(xfers_complete_r__t3),
    .xfers_issued_r(xfers_issued_r__t3),
    ._ar_ch_req(_ar_ch_req_3),
    ._ar_ch_grant(_ar_ch_grant_3),
    ._push_ch_req(_push_ch_req_3),
    ._push_ch_grant(_push_ch_grant_3),
    ._done_ch_req(_done_ch_req_3),
    ._done_ch_grant(_done_ch_grant_3)
  );
  logic _ar_ch_grant_0;
  logic _ar_ch_grant_1;
  logic _ar_ch_grant_2;
  logic _ar_ch_grant_3;
  logic [2-1:0] _ar_ch_last_grant = 3;
  assign _ar_ch_grant_0 = _ar_ch_req_0;
  assign _ar_ch_grant_1 = _ar_ch_req_1 && !_ar_ch_grant_0;
  assign _ar_ch_grant_2 = _ar_ch_req_2 && !_ar_ch_grant_0 && !_ar_ch_grant_1;
  assign _ar_ch_grant_3 = _ar_ch_req_3 && !_ar_ch_grant_0 && !_ar_ch_grant_1 && !_ar_ch_grant_2;
  always_ff @(posedge clk) begin
    if (_ar_ch_grant_0) begin
      _ar_ch_last_grant <= 0;
    end
    if (_ar_ch_grant_1) begin
      _ar_ch_last_grant <= 1;
    end
    if (_ar_ch_grant_2) begin
      _ar_ch_last_grant <= 2;
    end
    if (_ar_ch_grant_3) begin
      _ar_ch_last_grant <= 3;
    end
  end
  logic _push_ch_grant_0;
  logic _push_ch_grant_1;
  logic _push_ch_grant_2;
  logic _push_ch_grant_3;
  logic [2-1:0] _push_ch_last_grant = 3;
  assign _push_ch_grant_0 = _push_ch_req_0;
  assign _push_ch_grant_1 = _push_ch_req_1 && !_push_ch_grant_0;
  assign _push_ch_grant_2 = _push_ch_req_2 && !_push_ch_grant_0 && !_push_ch_grant_1;
  assign _push_ch_grant_3 = _push_ch_req_3 && !_push_ch_grant_0 && !_push_ch_grant_1 && !_push_ch_grant_2;
  always_ff @(posedge clk) begin
    if (_push_ch_grant_0) begin
      _push_ch_last_grant <= 0;
    end
    if (_push_ch_grant_1) begin
      _push_ch_last_grant <= 1;
    end
    if (_push_ch_grant_2) begin
      _push_ch_last_grant <= 2;
    end
    if (_push_ch_grant_3) begin
      _push_ch_last_grant <= 3;
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
  assign r_ready = r_ready__t0 | r_ready__t1 | r_ready__t2 | r_ready__t3;
  assign ar_burst = ar_burst__t0 | ar_burst__t1 | ar_burst__t2 | ar_burst__t3;
  assign ar_valid = ar_valid__t0 | ar_valid__t1 | ar_valid__t2 | ar_valid__t3;
  assign xfers_issued_r = xfers_issued_r__t0 | xfers_issued_r__t1 | xfers_issued_r__t2 | xfers_issued_r__t3;
  assign next_addr_r = next_addr_r__t0 | next_addr_r__t1 | next_addr_r__t2 | next_addr_r__t3;
  assign ar_len = ar_len__t0 | ar_len__t1 | ar_len__t2 | ar_len__t3;
  assign push_data = push_data__t0 | push_data__t1 | push_data__t2 | push_data__t3;
  assign ar_size = ar_size__t0 | ar_size__t1 | ar_size__t2 | ar_size__t3;
  assign ar_id = ar_id__t0 | ar_id__t1 | ar_id__t2 | ar_id__t3;
  assign push_valid = push_valid__t0 | push_valid__t1 | push_valid__t2 | push_valid__t3;
  assign ar_addr = ar_addr__t0 | ar_addr__t1 | ar_addr__t2 | ar_addr__t3;
  assign xfers_complete_r = xfers_complete_r__t0 | xfers_complete_r__t1 | xfers_complete_r__t2 | xfers_complete_r__t3;

endmodule

