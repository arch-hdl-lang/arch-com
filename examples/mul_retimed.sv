// Retimed multiplier: pipe_reg delays the product to meet timing.
//
// A 16x16 multiply produces a 32-bit result in one cycle, but the
// combinational delay through the multiplier may exceed the clock period.
// pipe_reg inserts N pipeline stages between the multiply output and
// the consumer, giving synthesis tools room to retime logic into the
// flip-flop chain. The result is available N cycles later.
//
// This is the ARCH equivalent of manually inserting pipeline registers:
//   reg stg1 <= product;
//   reg stg2 <= stg1;
//   result = stg2;
// but expressed in a single line with compile-time stage tracking.
module MulRetimed #(
  parameter int WIDTH = 16
) (
  input logic clk,
  input logic rst,
  input logic [WIDTH-1:0] a,
  input logic [WIDTH-1:0] b,
  input logic valid_in,
  output logic [32-1:0] result,
  output logic valid_out
);

  // Combinational multiply — wide result, long critical path
  logic [32-1:0] product;
  assign product = 32'(32'($unsigned(a)) * 32'($unsigned(b)));
  // Pipeline the result by 2 stages for timing closure
  logic [32-1:0] product_d2_stg1;
  logic [32-1:0] product_d2;
  always_ff @(posedge clk) begin
    if (rst) begin
      product_d2_stg1 <= '0;
      product_d2 <= '0;
    end else begin
      product_d2_stg1 <= product;
      product_d2 <= product_d2_stg1;
    end
  end
  // Pipeline the valid signal by the same depth to stay aligned
  logic valid_d2_stg1;
  logic valid_d2;
  always_ff @(posedge clk) begin
    if (rst) begin
      valid_d2_stg1 <= '0;
      valid_d2 <= '0;
    end else begin
      valid_d2_stg1 <= valid_in;
      valid_d2 <= valid_d2_stg1;
    end
  end
  assign result = product_d2;
  assign valid_out = valid_d2;

endmodule

