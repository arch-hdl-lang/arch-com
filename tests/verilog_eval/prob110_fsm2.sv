Wrote tests/verilog_eval/prob110_fsm2.sv
oore FSM, async reset
module TopModule (
  input logic clk,
  input logic areset,
  input logic j,
  input logic k,
  output logic out
);

  typedef enum logic [0:0] {
    OFF = 1'd0,
    ON = 1'd1
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      state_r <= OFF;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      OFF: begin
        if (j) state_next = ON;
      end
      ON: begin
        if (k) state_next = OFF;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      OFF: begin
        out = 1'b0;
      end
      ON: begin
        out = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

