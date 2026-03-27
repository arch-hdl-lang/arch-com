module TopModule (
  input logic clk,
  input logic [8-1:0] d,
  output logic [8-1:0] q
);

  always_ff @(posedge clk) begin
    q <= d;
  end

endmodule

