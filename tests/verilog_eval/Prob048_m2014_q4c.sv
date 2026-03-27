module TopModule (
  input logic clk,
  input logic r,
  input logic d,
  output logic q
);

  always_ff @(posedge clk) begin
    if (r) begin
      q <= 0;
    end else begin
      q <= d;
    end
  end

endmodule

