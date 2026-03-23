// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic bump_left,
  input logic bump_right,
  output logic walk_left,
  output logic walk_right
);

  logic st;
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      st <= 0;
    end else begin
      if ((~st)) begin
        if (bump_left) begin
          st <= 1;
        end
      end else if (bump_right) begin
        st <= 0;
      end
    end
  end
  assign walk_left = (~st);
  assign walk_right = st;

endmodule

