Wrote tests/verilog_eval/Prob096_review2015_fsmseq.sv
tart_shifting forever
module TopModule (
  input logic clk,
  input logic reset,
  input logic data,
  output logic start_shifting
);

  typedef enum logic [2:0] {
    S = 3'd0,
    S1 = 3'd1,
    S11 = 3'd2,
    S110 = 3'd3,
    DONE = 3'd4
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= S;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S: begin
        if (data) state_next = S1;
      end
      S1: begin
        if (data) state_next = S11;
        else if (~data) state_next = S;
      end
      S11: begin
        if (data) state_next = S11;
        else if (~data) state_next = S110;
      end
      S110: begin
        if (data) state_next = DONE;
        else if (~data) state_next = S;
      end
      DONE: begin
        state_next = DONE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    start_shifting = 1'b0;
    case (state_r)
      S: begin
      end
      S1: begin
      end
      S11: begin
      end
      S110: begin
      end
      DONE: begin
        start_shifting = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

