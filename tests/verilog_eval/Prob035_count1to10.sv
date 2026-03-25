// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  output logic [4-1:0] q
);

  always_ff @(posedge clk) begin
    if (reset) begin
      q <= 1;
    end else begin
      if (q == 10) begin
        q <= 1;
      end else begin
        q <= 4'(q + 1);
      end
    end
  end

endmodule

