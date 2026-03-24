// domain SysDomain

module TopModule (
  input logic clk,
  input logic enable,
  input logic S,
  input logic A,
  input logic B,
  input logic C,
  output logic Z
);

  logic [8-1:0] sr;
  always_ff @(posedge clk) begin
    if (enable) begin
      sr[0] <= S;
      for (int i = 1; i <= 7; i++) begin
        sr[i] <= sr[(i - 1)];
      end
    end
  end
  logic [3-1:0] sel;
  assign sel = {A, B, C};
  assign Z = sr[sel];

endmodule

