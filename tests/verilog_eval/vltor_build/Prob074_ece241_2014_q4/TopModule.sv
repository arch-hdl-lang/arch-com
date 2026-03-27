// domain SysDomain

module TopModule (
  input logic clk,
  input logic x,
  output logic z
);

  logic ff_xor;
  logic ff_and;
  logic ff_or;
  always_ff @(posedge clk) begin
    ff_xor <= x ^ ff_xor;
    ff_and <= x & ~ff_and;
    ff_or <= x | ~ff_or;
  end
  assign z = ~(ff_xor | ff_and | ff_or);

endmodule

