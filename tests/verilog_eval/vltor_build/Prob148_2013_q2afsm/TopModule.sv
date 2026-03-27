// VerilogEval Prob148: 4-state priority arbiter, active-low sync reset (resetn)
// domain SysDomain

module TopModule (
  input logic clk,
  input logic resetn,
  input logic [3-1:0] r,
  output logic [3-1:0] g
);

  typedef enum logic [1:0] {
    IDLE = 2'd0,
    GRANT0 = 2'd1,
    GRANT1 = 2'd2,
    GRANT2 = 2'd3
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if ((!resetn)) begin
      state_r <= IDLE;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      IDLE: begin
        if (r[0]) state_next = GRANT0;
        else if (~r[0] & r[1]) state_next = GRANT1;
        else if (~r[0] & ~r[1] & r[2]) state_next = GRANT2;
      end
      GRANT0: begin
        if (~r[0]) state_next = IDLE;
      end
      GRANT1: begin
        if (~r[1]) state_next = IDLE;
      end
      GRANT2: begin
        if (~r[2]) state_next = IDLE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    g = 0;
    case (state_r)
      IDLE: begin
      end
      GRANT0: begin
        g = 1;
      end
      GRANT1: begin
        g = 2;
      end
      GRANT2: begin
        g = 4;
      end
      default: ;
    endcase
  end

endmodule

