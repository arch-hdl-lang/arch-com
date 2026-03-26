Wrote tests/verilog_eval/Prob138_2012_q2fsm.sv
utput z
module TopModule (
  input logic clk,
  input logic reset,
  input logic w,
  output logic z
);

  typedef enum logic [2:0] {
    A = 3'd0,
    B = 3'd1,
    C = 3'd2,
    D = 3'd3,
    E = 3'd4,
    F = 3'd5
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
        if (w) state_next = B;
      end
      B: begin
        if (w) state_next = C;
        else if (~w) state_next = D;
      end
      C: begin
        if (w) state_next = E;
        else if (~w) state_next = D;
      end
      D: begin
        if (w) state_next = F;
        else if (~w) state_next = A;
      end
      E: begin
        if (w) state_next = E;
        else if (~w) state_next = D;
      end
      F: begin
        if (w) state_next = C;
        else if (~w) state_next = D;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    z = 1'b0;
    case (state_r)
      A: begin
      end
      B: begin
      end
      C: begin
      end
      D: begin
      end
      E: begin
        z = 1'b1;
      end
      F: begin
        z = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

