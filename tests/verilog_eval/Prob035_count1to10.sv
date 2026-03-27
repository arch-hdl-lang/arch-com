module TopModule (
  input logic clk,
  input logic reset,
  output logic [4-1:0] q
);

  always_ff @(posedge clk) begin
    if (reset) begin
      q <= 1;
    end else begin
      q <= q == 10 ? 1 : 4'(q + 1);
    end
  end

endmodule

