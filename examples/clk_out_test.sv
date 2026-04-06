module ClkDiv2 (
  input logic clk_in,
  input logic rst,
  output logic clk_out
);

  logic toggle;
  always_ff @(posedge clk_in) begin
    if (rst) begin
      toggle <= 1'b0;
    end else begin
      toggle <= ~toggle;
    end
  end
  assign clk_out = toggle;

endmodule

