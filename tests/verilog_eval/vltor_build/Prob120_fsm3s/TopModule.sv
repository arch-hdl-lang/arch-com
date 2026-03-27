// VerilogEval Prob120: 4-state Moore FSM, sync reset
// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic in,
  output logic out
);

  typedef enum logic [1:0] {
    A = 2'd0,
    B = 2'd1,
    C = 2'd2,
    D = 2'd3
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= A;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      A: begin
        if (in) state_next = B;
      end
      B: begin
        if (in) state_next = B;
        else if (~in) state_next = C;
      end
      C: begin
        if (in) state_next = D;
        else if (~in) state_next = A;
      end
      D: begin
        if (in) state_next = B;
        else if (~in) state_next = C;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      A: begin
        out = 1'b0;
      end
      B: begin
        out = 1'b0;
      end
      C: begin
        out = 1'b0;
      end
      D: begin
        out = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

