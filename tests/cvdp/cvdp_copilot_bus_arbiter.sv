module cvdp_copilot_bus_arbiter (
  input logic clk,
  input logic reset,
  input logic req1,
  input logic req2,
  output logic grant1 = 1'b0,
  output logic grant2 = 1'b0
);

  logic [2:0] state_r = 0;
  logic [2:0] next_state;
  always_comb begin
    if (state_r == 0) begin
      // IDLE
      if (req1 & req2) begin
        next_state = 2;
      end else if (req1) begin
        next_state = 1;
      end else if (req2) begin
        next_state = 2;
      end else begin
        next_state = 0;
      end
    end else if (state_r == 1) begin
      // GRANT_1
      if (req2) begin
        next_state = 2;
      end else if (req1) begin
        next_state = 1;
      end else begin
        next_state = 3;
      end
    end else if (state_r == 2) begin
      // GRANT_2
      if (req2) begin
        next_state = 2;
      end else if (req1) begin
        next_state = 1;
      end else begin
        next_state = 3;
      end
    end else if (state_r == 3) begin
      // CLEAR
      if (req1 & req2) begin
        next_state = 2;
      end else if (req1) begin
        next_state = 1;
      end else if (req2) begin
        next_state = 2;
      end else begin
        next_state = 0;
      end
    end else begin
      next_state = 0;
    end
  end
  always_ff @(posedge clk or posedge reset) begin
    if (reset) begin
      grant1 <= 1'b0;
      grant2 <= 1'b0;
      state_r <= 0;
    end else begin
      state_r <= next_state;
      grant1 <= next_state == 1;
      grant2 <= next_state == 2;
    end
  end

endmodule

