module TopModule (
  input logic clk,
  input logic reset,
  input logic slowena,
  output logic [4-1:0] q
);

  always_ff @(posedge clk) begin
    if (reset) begin
      q <= 0;
    end else begin
      q <= slowena ? q == 9 ? 0 : 4'(q + 1) : q;
    end
  end

endmodule

