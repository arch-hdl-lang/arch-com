module TopModule (
  input logic clk,
  input logic reset,
  input logic [8-1:0] d,
  output logic [8-1:0] q
);

  always_ff @(posedge clk) begin
    if (reset) begin
      q <= 0;
    end else begin
      q <= d;
    end
  end

endmodule

