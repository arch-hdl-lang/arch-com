// Thread-based multi-outstanding MM2S read engine.
// Each thread index = AXI ID. Thread i issues burst i.
// No seq blocks — all state managed by threads.
// All state driven by threads only
// done when all participating threads complete
// (unused threads have done=false but are gated by total_xfers check)
// Controller thread: latches start, clears done flags
module _ThreadMm2s_Controller (
  input logic clk,
  input logic rst,
  input logic [32-1:0] base_addr,
  input logic [8-1:0] burst_len,
  input logic done,
  input logic start,
  input logic [16-1:0] total_xfers,
  output logic active_r,
  output logic [32-1:0] base_addr_r,
  output logic [8-1:0] burst_len_r,
  input logic done_0,
  output logic done_0_wr,
  output logic done_0_we,
  input logic done_1,
  output logic done_1_wr,
  output logic done_1_we,
  input logic done_2,
  output logic done_2_wr,
  output logic done_2_we,
  input logic done_3,
  output logic done_3_wr,
  output logic done_3_we,
  output logic [16-1:0] total_xfers_r
);

  typedef enum logic [1:0] {
    S0 = 2'd0,
    S1 = 2'd1,
    S2 = 2'd2
  } _ThreadMm2s_Controller_state_t;
  
  _ThreadMm2s_Controller_state_t state_r, state_next;
  
  logic [32-1:0] _cnt;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= S0;
      _cnt <= 0;
      active_r <= 1'b0;
      base_addr_r <= 0;
      burst_len_r <= 0;
      done_0_wr <= 0;
      done_1_wr <= 0;
      done_2_wr <= 0;
      done_3_wr <= 0;
      total_xfers_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S1: begin
          if (done) begin
            total_xfers_r <= total_xfers;
            base_addr_r <= base_addr;
            burst_len_r <= burst_len;
            active_r <= 1'b1;
            // Clear done flags for all threads
            done_0_wr <= 1'b0;
            done_1_wr <= 1'b0;
            done_2_wr <= 1'b0;
            done_3_wr <= 1'b0;
          end
          if (done) begin
            _cnt <= 1 - 1;
          end
        end
        S2: begin
          if (_cnt == 0) begin
            // Wait for completion (done is combinational from done_0..3 + total_xfers)
            active_r <= 1'b0;
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
        if (done) state_next = S2;
      end
      S2: begin
        if (_cnt == 0) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    done_0_we = 1'b0;
    done_1_we = 1'b0;
    done_2_we = 1'b0;
    done_3_we = 1'b0;
    case (state_r)
      S0: begin
      end
      S1: begin
        done_0_we = 1'b1;
        done_1_we = 1'b1;
        done_2_we = 1'b1;
        done_3_we = 1'b1;
      end
      S2: begin
      end
      default: ;
    endcase
  end

endmodule

// Read thread 0
module _ThreadMm2s_ReadReq_0 (
  input logic clk,
  input logic rst,
  input logic active_r,
  input logic ar_ready,
  input logic [32-1:0] base_addr_r,
  input logic [8-1:0] burst_len_r,
  input logic [32-1:0] r_data,
  input logic [2-1:0] r_id,
  input logic r_valid,
  output logic [32-1:0] ar_addr,
  output logic [2-1:0] ar_burst,
  output logic [2-1:0] ar_id,
  output logic [8-1:0] ar_len,
  output logic [3-1:0] ar_size,
  output logic ar_valid,
  output logic [32-1:0] push_data,
  output logic push_valid,
  output logic r_ready,
  input logic done_0,
  output logic done_0_wr,
  output logic done_0_we,
  output logic _ar_ch_req,
  input logic _ar_ch_grant
);

  typedef enum logic [2:0] {
    S0 = 3'd0,
    S1 = 3'd1,
    S2 = 3'd2,
    S3 = 3'd3,
    S4 = 3'd4,
    S5 = 3'd5,
    S6 = 3'd6,
    S7 = 3'd7
  } _ThreadMm2s_ReadReq_0_state_t;
  
  _ThreadMm2s_ReadReq_0_state_t state_r, state_next;
  
  logic [32-1:0] _cnt;
  logic [32-1:0] _loop_cnt;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= S0;
      _cnt <= 0;
      _loop_cnt <= 0;
      done_0_wr <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S4: begin
          _loop_cnt <= 0;
        end
        S6: begin
          _loop_cnt <= _loop_cnt + 1;
          _cnt <= 1 - 1;
        end
        S7: begin
          if (_cnt == 0) begin
            done_0_wr <= 1'b1;
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
        if (active_r && !done_0) state_next = S1;
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
        if (_loop_cnt < burst_len_r - 1) state_next = S5;
        else if (_loop_cnt >= burst_len_r - 1) state_next = S7;
      end
      S7: begin
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
    done_0_we = 1'b0;
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
        ar_addr = base_addr_r;
        ar_id = 0;
        ar_len = 8'(burst_len_r - 1);
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
        push_valid = r_valid && r_id == 0;
        push_data = r_data;
      end
      S6: begin
      end
      S7: begin
        r_ready = 0;
        done_0_we = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

// Read thread 1
module _ThreadMm2s_ReadReq_1 (
  input logic clk,
  input logic rst,
  input logic active_r,
  input logic ar_ready,
  input logic [32-1:0] base_addr_r,
  input logic [8-1:0] burst_len_r,
  input logic [32-1:0] r_data,
  input logic [2-1:0] r_id,
  input logic r_valid,
  output logic [32-1:0] ar_addr,
  output logic [2-1:0] ar_burst,
  output logic [2-1:0] ar_id,
  output logic [8-1:0] ar_len,
  output logic [3-1:0] ar_size,
  output logic ar_valid,
  output logic [32-1:0] push_data,
  output logic push_valid,
  output logic r_ready,
  input logic done_1,
  output logic done_1_wr,
  output logic done_1_we,
  output logic _ar_ch_req,
  input logic _ar_ch_grant
);

  typedef enum logic [2:0] {
    S0 = 3'd0,
    S1 = 3'd1,
    S2 = 3'd2,
    S3 = 3'd3,
    S4 = 3'd4,
    S5 = 3'd5,
    S6 = 3'd6,
    S7 = 3'd7
  } _ThreadMm2s_ReadReq_1_state_t;
  
  _ThreadMm2s_ReadReq_1_state_t state_r, state_next;
  
  logic [32-1:0] _cnt;
  logic [32-1:0] _loop_cnt;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= S0;
      _cnt <= 0;
      _loop_cnt <= 0;
      done_1_wr <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S4: begin
          _loop_cnt <= 0;
        end
        S6: begin
          _loop_cnt <= _loop_cnt + 1;
          _cnt <= 1 - 1;
        end
        S7: begin
          if (_cnt == 0) begin
            done_1_wr <= 1'b1;
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
        if (active_r && !done_1) state_next = S1;
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
        if (_loop_cnt < burst_len_r - 1) state_next = S5;
        else if (_loop_cnt >= burst_len_r - 1) state_next = S7;
      end
      S7: begin
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
    done_1_we = 1'b0;
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
        ar_addr = base_addr_r + (32'($unsigned(burst_len_r)) << 2);
        ar_id = 1;
        ar_len = 8'(burst_len_r - 1);
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
        push_valid = r_valid && r_id == 1;
        push_data = r_data;
      end
      S6: begin
      end
      S7: begin
        r_ready = 0;
        done_1_we = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

// Read thread 2
module _ThreadMm2s_ReadReq_2 (
  input logic clk,
  input logic rst,
  input logic active_r,
  input logic ar_ready,
  input logic [32-1:0] base_addr_r,
  input logic [8-1:0] burst_len_r,
  input logic [32-1:0] r_data,
  input logic [2-1:0] r_id,
  input logic r_valid,
  output logic [32-1:0] ar_addr,
  output logic [2-1:0] ar_burst,
  output logic [2-1:0] ar_id,
  output logic [8-1:0] ar_len,
  output logic [3-1:0] ar_size,
  output logic ar_valid,
  output logic [32-1:0] push_data,
  output logic push_valid,
  output logic r_ready,
  input logic done_2,
  output logic done_2_wr,
  output logic done_2_we,
  output logic _ar_ch_req,
  input logic _ar_ch_grant
);

  typedef enum logic [2:0] {
    S0 = 3'd0,
    S1 = 3'd1,
    S2 = 3'd2,
    S3 = 3'd3,
    S4 = 3'd4,
    S5 = 3'd5,
    S6 = 3'd6,
    S7 = 3'd7
  } _ThreadMm2s_ReadReq_2_state_t;
  
  _ThreadMm2s_ReadReq_2_state_t state_r, state_next;
  
  logic [32-1:0] _cnt;
  logic [32-1:0] _loop_cnt;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= S0;
      _cnt <= 0;
      _loop_cnt <= 0;
      done_2_wr <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S4: begin
          _loop_cnt <= 0;
        end
        S6: begin
          _loop_cnt <= _loop_cnt + 1;
          _cnt <= 1 - 1;
        end
        S7: begin
          if (_cnt == 0) begin
            done_2_wr <= 1'b1;
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
        if (active_r && !done_2) state_next = S1;
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
        if (_loop_cnt < burst_len_r - 1) state_next = S5;
        else if (_loop_cnt >= burst_len_r - 1) state_next = S7;
      end
      S7: begin
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
    done_2_we = 1'b0;
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
        ar_addr = base_addr_r + 2 * (32'($unsigned(burst_len_r)) << 2);
        ar_id = 2;
        ar_len = 8'(burst_len_r - 1);
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
        push_valid = r_valid && r_id == 2;
        push_data = r_data;
      end
      S6: begin
      end
      S7: begin
        r_ready = 0;
        done_2_we = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

// Read thread 3
module _ThreadMm2s_ReadReq_3 (
  input logic clk,
  input logic rst,
  input logic active_r,
  input logic ar_ready,
  input logic [32-1:0] base_addr_r,
  input logic [8-1:0] burst_len_r,
  input logic [32-1:0] r_data,
  input logic [2-1:0] r_id,
  input logic r_valid,
  output logic [32-1:0] ar_addr,
  output logic [2-1:0] ar_burst,
  output logic [2-1:0] ar_id,
  output logic [8-1:0] ar_len,
  output logic [3-1:0] ar_size,
  output logic ar_valid,
  output logic [32-1:0] push_data,
  output logic push_valid,
  output logic r_ready,
  input logic done_3,
  output logic done_3_wr,
  output logic done_3_we,
  output logic _ar_ch_req,
  input logic _ar_ch_grant
);

  typedef enum logic [2:0] {
    S0 = 3'd0,
    S1 = 3'd1,
    S2 = 3'd2,
    S3 = 3'd3,
    S4 = 3'd4,
    S5 = 3'd5,
    S6 = 3'd6,
    S7 = 3'd7
  } _ThreadMm2s_ReadReq_3_state_t;
  
  _ThreadMm2s_ReadReq_3_state_t state_r, state_next;
  
  logic [32-1:0] _cnt;
  logic [32-1:0] _loop_cnt;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= S0;
      _cnt <= 0;
      _loop_cnt <= 0;
      done_3_wr <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S4: begin
          _loop_cnt <= 0;
        end
        S6: begin
          _loop_cnt <= _loop_cnt + 1;
          _cnt <= 1 - 1;
        end
        S7: begin
          if (_cnt == 0) begin
            done_3_wr <= 1'b1;
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
        if (active_r && !done_3) state_next = S1;
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
        if (_loop_cnt < burst_len_r - 1) state_next = S5;
        else if (_loop_cnt >= burst_len_r - 1) state_next = S7;
      end
      S7: begin
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
    done_3_we = 1'b0;
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
        ar_addr = base_addr_r + 3 * (32'($unsigned(burst_len_r)) << 2);
        ar_id = 3;
        ar_len = 8'(burst_len_r - 1);
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
        push_valid = r_valid && r_id == 3;
        push_data = r_data;
      end
      S6: begin
      end
      S7: begin
        r_ready = 0;
        done_3_we = 1'b1;
      end
      default: ;
    endcase
  end

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

  logic done_1_wr__t1;
  logic done_1_we__t1;
  logic done_1_wr__t0;
  logic done_1_we__t0;
  logic done_0_wr__t1;
  logic done_0_we__t1;
  logic done_0_wr__t0;
  logic done_0_we__t0;
  logic done_3_wr__t1;
  logic done_3_we__t1;
  logic done_3_wr__t0;
  logic done_3_we__t0;
  logic done_2_wr__t1;
  logic done_2_we__t1;
  logic done_2_wr__t0;
  logic done_2_we__t0;
  logic [8-1:0] ar_len__t3;
  logic [8-1:0] ar_len__t2;
  logic [8-1:0] ar_len__t1;
  logic [8-1:0] ar_len__t0;
  logic [32-1:0] ar_addr__t3;
  logic [32-1:0] ar_addr__t2;
  logic [32-1:0] ar_addr__t1;
  logic [32-1:0] ar_addr__t0;
  logic [2-1:0] ar_id__t3;
  logic [2-1:0] ar_id__t2;
  logic [2-1:0] ar_id__t1;
  logic [2-1:0] ar_id__t0;
  logic [2-1:0] ar_burst__t3;
  logic [2-1:0] ar_burst__t2;
  logic [2-1:0] ar_burst__t1;
  logic [2-1:0] ar_burst__t0;
  logic [3-1:0] ar_size__t3;
  logic [3-1:0] ar_size__t2;
  logic [3-1:0] ar_size__t1;
  logic [3-1:0] ar_size__t0;
  logic [32-1:0] push_data__t3;
  logic [32-1:0] push_data__t2;
  logic [32-1:0] push_data__t1;
  logic [32-1:0] push_data__t0;
  logic ar_valid__t3;
  logic ar_valid__t2;
  logic ar_valid__t1;
  logic ar_valid__t0;
  logic push_valid__t3;
  logic push_valid__t2;
  logic push_valid__t1;
  logic push_valid__t0;
  logic r_ready__t3;
  logic r_ready__t2;
  logic r_ready__t1;
  logic r_ready__t0;
  logic _ar_ch_req_3;
  logic _ar_ch_req_2;
  logic _ar_ch_req_1;
  logic _ar_ch_req_0;
  logic active_r;
  logic [16-1:0] total_xfers_r;
  logic [32-1:0] base_addr_r;
  logic [8-1:0] burst_len_r;
  logic done_0;
  logic done_1;
  logic done_2;
  logic done_3;
  assign halted = 1'b0;
  assign idle_out = !active_r;
  assign done = active_r && total_xfers_r != 0 && (done_0 || total_xfers_r < 1) && (done_1 || total_xfers_r < 2) && (done_2 || total_xfers_r < 3) && (done_3 || total_xfers_r < 4);
  _ThreadMm2s_Controller _Controller (
    .clk(clk),
    .rst(rst),
    .base_addr(base_addr),
    .burst_len(burst_len),
    .done(done),
    .start(start),
    .total_xfers(total_xfers),
    .active_r(active_r),
    .base_addr_r(base_addr_r),
    .burst_len_r(burst_len_r),
    .done_0(done_0),
    .done_0_wr(done_0_wr__t0),
    .done_0_we(done_0_we__t0),
    .done_1(done_1),
    .done_1_wr(done_1_wr__t0),
    .done_1_we(done_1_we__t0),
    .done_2(done_2),
    .done_2_wr(done_2_wr__t0),
    .done_2_we(done_2_we__t0),
    .done_3(done_3),
    .done_3_wr(done_3_wr__t0),
    .done_3_we(done_3_we__t0),
    .total_xfers_r(total_xfers_r)
  );
  _ThreadMm2s_ReadReq_0 _ReadReq_0 (
    .clk(clk),
    .rst(rst),
    .active_r(active_r),
    .ar_ready(ar_ready),
    .base_addr_r(base_addr_r),
    .burst_len_r(burst_len_r),
    .r_data(r_data),
    .r_id(r_id),
    .r_valid(r_valid),
    .ar_addr(ar_addr__t0),
    .ar_burst(ar_burst__t0),
    .ar_id(ar_id__t0),
    .ar_len(ar_len__t0),
    .ar_size(ar_size__t0),
    .ar_valid(ar_valid__t0),
    .push_data(push_data__t0),
    .push_valid(push_valid__t0),
    .r_ready(r_ready__t0),
    .done_0(done_0),
    .done_0_wr(done_0_wr__t1),
    .done_0_we(done_0_we__t1),
    ._ar_ch_req(_ar_ch_req_0),
    ._ar_ch_grant(_ar_ch_grant_0)
  );
  _ThreadMm2s_ReadReq_1 _ReadReq_1 (
    .clk(clk),
    .rst(rst),
    .active_r(active_r),
    .ar_ready(ar_ready),
    .base_addr_r(base_addr_r),
    .burst_len_r(burst_len_r),
    .r_data(r_data),
    .r_id(r_id),
    .r_valid(r_valid),
    .ar_addr(ar_addr__t1),
    .ar_burst(ar_burst__t1),
    .ar_id(ar_id__t1),
    .ar_len(ar_len__t1),
    .ar_size(ar_size__t1),
    .ar_valid(ar_valid__t1),
    .push_data(push_data__t1),
    .push_valid(push_valid__t1),
    .r_ready(r_ready__t1),
    .done_1(done_1),
    .done_1_wr(done_1_wr__t1),
    .done_1_we(done_1_we__t1),
    ._ar_ch_req(_ar_ch_req_1),
    ._ar_ch_grant(_ar_ch_grant_1)
  );
  _ThreadMm2s_ReadReq_2 _ReadReq_2 (
    .clk(clk),
    .rst(rst),
    .active_r(active_r),
    .ar_ready(ar_ready),
    .base_addr_r(base_addr_r),
    .burst_len_r(burst_len_r),
    .r_data(r_data),
    .r_id(r_id),
    .r_valid(r_valid),
    .ar_addr(ar_addr__t2),
    .ar_burst(ar_burst__t2),
    .ar_id(ar_id__t2),
    .ar_len(ar_len__t2),
    .ar_size(ar_size__t2),
    .ar_valid(ar_valid__t2),
    .push_data(push_data__t2),
    .push_valid(push_valid__t2),
    .r_ready(r_ready__t2),
    .done_2(done_2),
    .done_2_wr(done_2_wr__t1),
    .done_2_we(done_2_we__t1),
    ._ar_ch_req(_ar_ch_req_2),
    ._ar_ch_grant(_ar_ch_grant_2)
  );
  _ThreadMm2s_ReadReq_3 _ReadReq_3 (
    .clk(clk),
    .rst(rst),
    .active_r(active_r),
    .ar_ready(ar_ready),
    .base_addr_r(base_addr_r),
    .burst_len_r(burst_len_r),
    .r_data(r_data),
    .r_id(r_id),
    .r_valid(r_valid),
    .ar_addr(ar_addr__t3),
    .ar_burst(ar_burst__t3),
    .ar_id(ar_id__t3),
    .ar_len(ar_len__t3),
    .ar_size(ar_size__t3),
    .ar_valid(ar_valid__t3),
    .push_data(push_data__t3),
    .push_valid(push_valid__t3),
    .r_ready(r_ready__t3),
    .done_3(done_3),
    .done_3_wr(done_3_wr__t1),
    .done_3_we(done_3_we__t1),
    ._ar_ch_req(_ar_ch_req_3),
    ._ar_ch_grant(_ar_ch_grant_3)
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
  assign r_ready = r_ready__t0 | r_ready__t1 | r_ready__t2 | r_ready__t3;
  assign push_valid = push_valid__t0 | push_valid__t1 | push_valid__t2 | push_valid__t3;
  assign ar_valid = ar_valid__t0 | ar_valid__t1 | ar_valid__t2 | ar_valid__t3;
  assign push_data = push_data__t0 | push_data__t1 | push_data__t2 | push_data__t3;
  assign ar_size = ar_size__t0 | ar_size__t1 | ar_size__t2 | ar_size__t3;
  assign ar_burst = ar_burst__t0 | ar_burst__t1 | ar_burst__t2 | ar_burst__t3;
  assign ar_id = ar_id__t0 | ar_id__t1 | ar_id__t2 | ar_id__t3;
  assign ar_addr = ar_addr__t0 | ar_addr__t1 | ar_addr__t2 | ar_addr__t3;
  assign ar_len = ar_len__t0 | ar_len__t1 | ar_len__t2 | ar_len__t3;
  always_ff @(posedge clk) begin
    if (rst) begin
      done_2 <= 1'b0;
    end else begin
      if (done_2_we__t0) begin
        done_2 <= done_2_wr__t0;
      end else if (done_2_we__t1) begin
        done_2 <= done_2_wr__t1;
      end
    end
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      done_3 <= 1'b0;
    end else begin
      if (done_3_we__t0) begin
        done_3 <= done_3_wr__t0;
      end else if (done_3_we__t1) begin
        done_3 <= done_3_wr__t1;
      end
    end
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      done_0 <= 1'b0;
    end else begin
      if (done_0_we__t0) begin
        done_0 <= done_0_wr__t0;
      end else if (done_0_we__t1) begin
        done_0 <= done_0_wr__t1;
      end
    end
  end
  always_ff @(posedge clk) begin
    if (rst) begin
      done_1 <= 1'b0;
    end else begin
      if (done_1_we__t0) begin
        done_1 <= done_1_wr__t0;
      end else if (done_1_we__t1) begin
        done_1 <= done_1_wr__t1;
      end
    end
  end

endmodule

