Wrote tests/verilog_eval/Prob088_ece241_2014_q5b.sv
areset,
  input logic x,
  output logic z
);

  typedef enum logic [0:0] {
    A = 1'd0,
    B = 1'd1
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
        state_next = B;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    case (state_r)
      A: begin
        z = x;
      end
      B: begin
        z = ~x;
      end
      default: ;
    endcase
  end

endmodule

