// VerilogEval Prob142: Lemmings walk/fall FSM with async reset
// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic bump_left,
  input logic bump_right,
  input logic ground,
  output logic walk_left,
  output logic walk_right,
  output logic aaah
);

  // States: 0=WalkLeft, 1=WalkRight, 2=FallLeft, 3=FallRight
  logic [2-1:0] state_r;
  logic [2-1:0] next_state;
  always_comb begin
    next_state = state_r;
    if ((state_r == 0)) begin
      if ((~ground)) begin
        next_state = 2;
      end else if (bump_left) begin
        next_state = 1;
      end
    end else if ((state_r == 1)) begin
      if ((~ground)) begin
        next_state = 3;
      end else if (bump_right) begin
        next_state = 0;
      end
    end else if ((state_r == 2)) begin
      if (ground) begin
        next_state = 0;
      end
    end else if ((state_r == 3)) begin
      if (ground) begin
        next_state = 1;
      end
    end else begin
      next_state = 0;
    end
  end
  // Walking left
  // Walking right
  // Falling (was walking left)
  // Falling (was walking right)
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      state_r <= 0;
    end else begin
      state_r <= next_state;
    end
  end
  assign walk_left = (state_r == 0);
  assign walk_right = (state_r == 1);
  assign aaah = ((state_r == 2) | (state_r == 3));

endmodule

