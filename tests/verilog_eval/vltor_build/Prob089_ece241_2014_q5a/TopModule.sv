// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic x,
  output logic z
);

  typedef enum logic [1:0] {
    A = 2'd0,
    B = 2'd1,
    C = 2'd2,
    D = 2'd3
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      state_r <= A;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      A: begin
        if (x) state_next = B;
      end
      B: begin
        if (x) state_next = D;
        else if (~x) state_next = C;
      end
      C: begin
        if (x) state_next = D;
      end
      D: begin
        if (~x) state_next = C;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      A: begin
        z = 1'b0;
      end
      B: begin
        z = 1'b1;
      end
      C: begin
        z = 1'b1;
      end
      D: begin
        z = 1'b0;
      end
      default: ;
    endcase
  end

endmodule

