// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic in,
  output logic out
);

  typedef enum logic [0:0] {
    A = 1'd0,
    B = 1'd1
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= B;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      A: begin
        if ((~in)) state_next = B;
      end
      B: begin
        if ((~in)) state_next = A;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    out = 1'b0; // default
    case (state_r)
      A: begin
      end
      B: begin
        out = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

