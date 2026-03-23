// VerilogEval Prob095: FSM that asserts shift_ena for 4 cycles after reset
// domain SysDomain

module TopModule (
  input logic clk,
  input logic rst,
  output logic shift_ena
);

  typedef enum logic [2:0] {
    B0 = 3'd0,
    B1 = 3'd1,
    B2 = 3'd2,
    B3 = 3'd3,
    DONE = 3'd4
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if (rst) begin
      state_r <= B0;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      B0: begin
        state_next = B1;
      end
      B1: begin
        state_next = B2;
      end
      B2: begin
        state_next = B3;
      end
      B3: begin
        state_next = DONE;
      end
      DONE: begin
        state_next = DONE;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    shift_ena = 1'b1; // default
    case (state_r)
      B0: begin
      end
      B1: begin
      end
      B2: begin
      end
      B3: begin
      end
      DONE: begin
        shift_ena = 1'b0;
      end
      default: ;
    endcase
  end

endmodule

