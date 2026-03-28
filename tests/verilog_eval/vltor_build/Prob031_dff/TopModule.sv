module TopModule (
  input logic clk,
  input logic d,
  output logic q
);

  always_ff @(posedge clk) begin
    q <= d;
  end

endmodule

