Wrote tests/verilog_eval/Prob149_ece241_2013_q4.sv
r (decreasing flow rate)
// 6 states: level + direction
module TopModule (
  input logic clk,
  input logic reset,
  input logic [3-1:0] s,
  output logic fr2,
  output logic fr1,
  output logic fr0,
  output logic dfr
);

  typedef enum logic [2:0] {
    A2 = 3'd0,
    B1 = 3'd1,
    B2 = 3'd2,
    C1 = 3'd3,
    C2 = 3'd4,
    D1 = 3'd5
  } TopModule_state_t;
  
  TopModule_state_t state_r, state_next;
  
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= A2;
    end else begin
      state_r <= state_next;
    end
  end
  
  always_comb begin
    state_next = state_r; // hold by default
    case (state_r)
      A2: begin
        if (s[0]) state_next = B1;
      end
      B1: begin
        if (s[1]) state_next = C1;
        else if (~s[0]) state_next = A2;
      end
      B2: begin
        if (s[1]) state_next = C1;
        else if (~s[0]) state_next = A2;
      end
      C1: begin
        if (s[2]) state_next = D1;
        else if (~s[1]) state_next = B2;
      end
      C2: begin
        if (s[2]) state_next = D1;
        else if (~s[1]) state_next = B2;
      end
      D1: begin
        if (~s[2]) state_next = C2;
      end
      default: state_next = state_r;
    endcase
  end
  
  always_comb begin
    fr2 = 1'b0;
    fr1 = 1'b0;
    fr0 = 1'b0;
    dfr = 1'b0;
    case (state_r)
      A2: begin
        fr2 = 1'b1;
        fr1 = 1'b1;
        fr0 = 1'b1;
        dfr = 1'b1;
      end
      B1: begin
        fr1 = 1'b1;
        fr0 = 1'b1;
      end
      B2: begin
        fr1 = 1'b1;
        fr0 = 1'b1;
        dfr = 1'b1;
      end
      C1: begin
        fr0 = 1'b1;
      end
      C2: begin
        fr0 = 1'b1;
        dfr = 1'b1;
      end
      D1: begin
      end
      default: ;
    endcase
  end

endmodule

