module _CalibOnce_thread (
  input logic clk,
  input logic rst_n,
  input logic cal_done,
  output logic cal_start,
  output logic cal_valid_r
);

  typedef enum logic [1:0] {
    S0 = 2'd0,
    S1 = 2'd1,
    DONE = 2'd2
  } _CalibOnce_thread_state_t;
  
  _CalibOnce_thread_state_t state_r, state_next;
  
  always_ff @(posedge clk or negedge rst_n) begin
    if ((!rst_n)) begin
      state_r <= S0;
      cal_valid_r <= 0;
    end else begin
      state_r <= state_next;
      case (state_r)
        S1: begin
          cal_valid_r <= 1;
        end
        default: ;
      endcase
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S0: begin
        if (cal_done) state_next = S1;
      end
      S1: begin
        state_next = DONE;
      end
      DONE: begin
        state_next = DONE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    cal_start = 0;
    case (state_r)
      S0: begin
        cal_start = 1;
      end
      S1: begin
        cal_start = 0;
      end
      DONE: begin
      end
      default: ;
    endcase
  end

endmodule

module CalibOnce (
  input logic clk,
  input logic rst_n,
  output logic cal_start,
  input logic cal_done,
  output logic cal_valid
);

  logic cal_valid_r;
  assign cal_valid = cal_valid_r;
  _CalibOnce_thread _thread (
    .clk(clk),
    .rst_n(rst_n),
    .cal_done(cal_done),
    .cal_start(cal_start),
    .cal_valid_r(cal_valid_r)
  );

endmodule

