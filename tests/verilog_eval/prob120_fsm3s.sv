// VerilogEval Prob120: 4-state Moore FSM, sync reset
// domain SysDomain

module TopModule (
  input logic clk,
  input logic rst,
  input logic in_sig,
  output logic out_sig
);

  typedef enum logic [1:0] {
    A = 2'd0,
    B = 2'd1,
    C = 2'd2,
    D = 2'd3
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= A;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      A: begin
        if (in_sig) state_next = B;
      end
      B: begin
        if (in_sig) state_next = B;
        else if ((~in_sig)) state_next = C;
      end
      C: begin
        if (in_sig) state_next = D;
        else if ((~in_sig)) state_next = A;
      end
      D: begin
        if (in_sig) state_next = B;
        else if ((~in_sig)) state_next = C;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    out_sig = 1'b0; // default
    case (state_r)
      A: begin
      end
      B: begin
      end
      C: begin
      end
      D: begin
        out_sig = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

