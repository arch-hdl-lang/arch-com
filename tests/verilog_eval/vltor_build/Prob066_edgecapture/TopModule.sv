// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic [32-1:0] in,
  output logic [32-1:0] out
);

  logic [32-1:0] prev;
  logic [32-1:0] captured;
  always_ff @(posedge clk) begin
    if (reset) begin
      captured <= 0;
    end else begin
      captured <= captured | prev & ~in;
    end
  end
  always_ff @(posedge clk) begin
    prev <= in;
  end
  assign out = captured;

endmodule

