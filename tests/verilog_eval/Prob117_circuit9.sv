// domain SysDomain

module TopModule (
  input logic clk,
  input logic a,
  output logic [3-1:0] q
);

  always_ff @(posedge clk) begin
    if (a) begin
      q <= 4;
    end else if (q == 6) begin
      q <= 0;
    end else begin
      q <= 3'(q + 1);
    end
  end

endmodule

