Wrote tests/verilog_eval/Prob121_2014_q3bfsm.sv
gic reset,
  input logic x,
  output logic z
);

  typedef enum logic [2:0] {
    A = 3'd0,
    B = 3'd1,
    C = 3'd2,
    D = 3'd3,
    E = 3'd4
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
        if (x) state_next = B;
      end
      B: begin
        if (x) state_next = E;
      end
      C: begin
        if (x) state_next = B;
      end
      D: begin
        if (x) state_next = C;
        else if (~x) state_next = B;
      end
      E: begin
        if (~x) state_next = D;
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
        z = 1'b1;
      end
      E: begin
        z = 1'b1;
      end
      default: ;
    endcase
  end

endmodule

