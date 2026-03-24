// VerilogEval Prob149: Water reservoir FSM with dfr (decreasing flow rate)
// 6 states: level + direction
// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic [3-1:0] s,
  output logic fr2,
  output logic fr1,
  output logic fr0,
  output logic dfr
);

  // States: 0=A2(below,falling), 1=B1(mid-low,rising), 2=B2(mid-low,falling)
  //         3=C1(mid-high,rising), 4=C2(mid-high,falling), 5=D1(above,rising)
  logic [3-1:0] state_r;
  logic [3-1:0] next_state;
  always_comb begin
    if ((state_r == 0)) begin
      if (s[0]) begin
        next_state = 1;
      end else begin
        next_state = 0;
      end
    end else if ((state_r == 1)) begin
      if (s[1]) begin
        next_state = 3;
      end else if (s[0]) begin
        next_state = 1;
      end else begin
        next_state = 0;
      end
    end else if ((state_r == 2)) begin
      if (s[1]) begin
        next_state = 3;
      end else if (s[0]) begin
        next_state = 2;
      end else begin
        next_state = 0;
      end
    end else if ((state_r == 3)) begin
      if (s[2]) begin
        next_state = 5;
      end else if (s[1]) begin
        next_state = 3;
      end else begin
        next_state = 2;
      end
    end else if ((state_r == 4)) begin
      if (s[2]) begin
        next_state = 5;
      end else if (s[1]) begin
        next_state = 4;
      end else begin
        next_state = 2;
      end
    end else if ((state_r == 5)) begin
      if (s[2]) begin
        next_state = 5;
      end else begin
        next_state = 4;
      end
    end else begin
      next_state = 0;
    end
  end
  // A2: below s0
  // B1: mid-low, rising
  // B2: mid-low, falling
  // C1: mid-high, rising
  // C2: mid-high, falling
  // D1: above s2
  always_ff @(posedge clk) begin
    if (reset) begin
      state_r <= 0;
    end else begin
      state_r <= next_state;
    end
  end
  always_comb begin
    if ((state_r == 0)) begin
      fr2 = 1'b1;
      fr1 = 1'b1;
      fr0 = 1'b1;
      dfr = 1'b1;
    end else if ((state_r == 1)) begin
      fr2 = 1'b0;
      fr1 = 1'b1;
      fr0 = 1'b1;
      dfr = 1'b0;
    end else if ((state_r == 2)) begin
      fr2 = 1'b0;
      fr1 = 1'b1;
      fr0 = 1'b1;
      dfr = 1'b1;
    end else if ((state_r == 3)) begin
      fr2 = 1'b0;
      fr1 = 1'b0;
      fr0 = 1'b1;
      dfr = 1'b0;
    end else if ((state_r == 4)) begin
      fr2 = 1'b0;
      fr1 = 1'b0;
      fr0 = 1'b1;
      dfr = 1'b1;
    end else if ((state_r == 5)) begin
      fr2 = 1'b0;
      fr1 = 1'b0;
      fr0 = 1'b0;
      dfr = 1'b0;
    end else begin
      fr2 = 1'b0;
      fr1 = 1'b0;
      fr0 = 1'b0;
      dfr = 1'b0;
    end
  end

endmodule

// Output logic: {fr2, fr1, fr0, dfr}
// A2: 1111
// B1: 0110
// B2: 0111
// C1: 0010
// C2: 0011
// D1: 0000
