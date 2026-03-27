module TopModule (
  input logic clk,
  input logic reset,
  output logic [10-1:0] q
);

  always_ff @(posedge clk) begin
    if (reset) begin
      q <= 0;
    end else begin
      q <= q == 999 ? 0 : 10'(q + 1);
    end
  end

endmodule

