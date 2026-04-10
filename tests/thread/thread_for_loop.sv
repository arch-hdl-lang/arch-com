module _BurstRead_thread (
  input logic clk,
  input logic rst_n,
  input logic ar_ready,
  input logic r_valid,
  output logic ar_valid,
  output logic r_ready
);

  typedef enum logic [1:0] {
    S0 = 2'd0,
    S1 = 2'd1,
    S2 = 2'd2,
    S3 = 2'd3
  } _BurstRead_thread_state_t;
  
  _BurstRead_thread_state_t state_r, state_next;
  
  logic [32-1:0] _loop_cnt;
  
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      state_r <= S0;
      _loop_cnt <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S1: begin
          _loop_cnt <= 0;
        end
        S3: begin
          _loop_cnt <= _loop_cnt + 1;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S0: begin
        if (ar_ready) state_next = S1;
      end
      S1: begin
        state_next = S2;
      end
      S2: begin
        if (r_valid) state_next = S3;
      end
      S3: begin
        if (_loop_cnt < 3) state_next = S2;
        else if (_loop_cnt >= 3) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    ar_valid = 0;
    r_ready = 0;
    case (state_r)
      S0: begin
        ar_valid = 1;
      end
      S1: begin
      end
      S2: begin
        r_ready = 1;
      end
      S3: begin
      end
      default: ;
    endcase
  end

endmodule

module BurstRead (
  input logic clk,
  input logic rst_n,
  output logic ar_valid,
  input logic ar_ready,
  output logic r_ready,
  input logic r_valid,
  input logic [32-1:0] r_data
);

  logic [32-1:0] buf_0;
  logic [32-1:0] buf_1;
  logic [32-1:0] buf_2;
  logic [32-1:0] buf_3;
  _BurstRead_thread _thread (
    .clk(clk),
    .rst_n(rst_n),
    .ar_ready(ar_ready),
    .r_valid(r_valid),
    .ar_valid(ar_valid),
    .r_ready(r_ready)
  );

endmodule

