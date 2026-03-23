// VerilogEval Prob155: Lemmings walk/fall/dig/splatter FSM with async reset
// Fall > 20 cycles then hit ground = splat (dead forever)
// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic bump_left,
  input logic bump_right,
  input logic ground,
  input logic dig,
  output logic walk_left,
  output logic walk_right,
  output logic aaah,
  output logic digging
);

  // States: 0=WalkLeft, 1=WalkRight, 2=FallLeft, 3=FallRight, 4=DigLeft, 5=DigRight, 6=Splat
  logic [3-1:0] state_r;
  logic [5-1:0] fall_count;
  // Flag set when fall exceeds 20 cycles; never cleared until reset
  logic long_fall;
  logic [3-1:0] next_state;
  logic [5-1:0] next_fall_count;
  logic next_long_fall;
  always_comb begin
    next_state = state_r;
    next_fall_count = 0;
    next_long_fall = long_fall;
    if ((state_r == 0)) begin
      next_long_fall = 1'b0;
      if ((~ground)) begin
        next_state = 2;
        next_fall_count = 1;
      end else if (dig) begin
        next_state = 4;
      end else if (bump_left) begin
        next_state = 1;
      end
    end else if ((state_r == 1)) begin
      next_long_fall = 1'b0;
      if ((~ground)) begin
        next_state = 3;
        next_fall_count = 1;
      end else if (dig) begin
        next_state = 5;
      end else if (bump_right) begin
        next_state = 0;
      end
    end else if ((state_r == 2)) begin
      if (ground) begin
        if ((long_fall | (fall_count > 20))) begin
          next_state = 6;
        end else begin
          next_state = 0;
        end
      end else begin
        next_state = 2;
        next_fall_count = 5'((fall_count + 1));
        if ((fall_count > 19)) begin
          next_long_fall = 1'b1;
        end
      end
    end else if ((state_r == 3)) begin
      if (ground) begin
        if ((long_fall | (fall_count > 20))) begin
          next_state = 6;
        end else begin
          next_state = 1;
        end
      end else begin
        next_state = 3;
        next_fall_count = 5'((fall_count + 1));
        if ((fall_count > 19)) begin
          next_long_fall = 1'b1;
        end
      end
    end else if ((state_r == 4)) begin
      if ((~ground)) begin
        next_state = 2;
        next_fall_count = 1;
      end
    end else if ((state_r == 5)) begin
      if ((~ground)) begin
        next_state = 3;
        next_fall_count = 1;
      end
    end else if ((state_r == 6)) begin
      next_state = 6;
    end else begin
      next_state = 0;
    end
  end
  // WalkLeft
  // WalkRight
  // FallLeft
  // FallRight
  // DigLeft
  // DigRight
  // Splat: dead forever
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      fall_count <= 0;
      long_fall <= 1'b0;
      state_r <= 0;
    end else begin
      state_r <= next_state;
      fall_count <= next_fall_count;
      long_fall <= next_long_fall;
    end
  end
  assign walk_left = (state_r == 0);
  assign walk_right = (state_r == 1);
  assign aaah = ((state_r == 2) | (state_r == 3));
  assign digging = ((state_r == 4) | (state_r == 5));

endmodule

