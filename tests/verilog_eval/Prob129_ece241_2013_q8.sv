Wrote tests/verilog_eval/Prob129_ece241_2013_q8.sv
ping 101, async active-low reset
module TopModule (
  input logic clk,
  input logic aresetn,
  input logic x,
  output logic z
);

  typedef enum logic [1:0] {
    S0 = 2'd0,
    S1 = 2'd1,
    S10 = 2'd2
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk or negedge aresetn) begin
    if ((!aresetn)) begin
      state_r <= S0;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      S0: begin
        if (x) state_next = S1;
      end
      S1: begin
        if (~x) state_next = S10;
      end
      S10: begin
        if (x) state_next = S1;
        else if (~x) state_next = S0;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    z = 1'b0;
    case (state_r)
      S0: begin
      end
      S1: begin
      end
      S10: begin
        z = 1'b0;
        if (x) begin
          z = 1'b1;
        end
      end
      default: ;
    endcase
  end

endmodule

