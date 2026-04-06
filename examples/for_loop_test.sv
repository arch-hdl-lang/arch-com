// domain SysDomain

module ForLoopTest (
  input logic clk,
  input logic rst,
  input logic [8-1:0] din,
  output logic [8-1:0] dout
);

  logic [8-1:0] sr [4-1:0];
  always_ff @(posedge clk) begin
    if (rst) begin
      for (int __ri0 = 0; __ri0 < 4; __ri0++) begin
        sr[__ri0] <= 0;
      end
    end else begin
      sr[0] <= din;
      for (int i = 1; i <= 3; i++) begin
        sr[i] <= sr[i - 1];
      end
    end
  end
  assign dout = sr[3];

endmodule

