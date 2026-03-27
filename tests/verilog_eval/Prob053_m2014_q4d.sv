module TopModule (
  input logic clk,
  input logic in,
  output logic out
);

  always_ff @(posedge clk) begin
    out <= in ^ out;
  end

endmodule

