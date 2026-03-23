// domain SysDomain

module TopModule (
  input logic clk,
  input logic areset,
  input logic x,
  output logic z
);

  logic [1-1:0] state_r;
  always_ff @(posedge clk or posedge areset) begin
    if (areset) begin
      state_r <= 0;
    end else begin
      if ((state_r == 0)) begin
        if (x) begin
          state_r <= 1;
        end
      end
    end
  end
  always_comb begin
    if ((state_r == 0)) begin
      z = x;
    end else begin
      z = (~x);
    end
  end

endmodule

