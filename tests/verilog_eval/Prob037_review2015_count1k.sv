// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  output logic [10-1:0] q
);

  always_ff @(posedge clk) begin
    if (reset) begin
      q <= 0;
    end else begin
      if (q == 999) begin
        q <= 0;
      end else begin
        q <= 10'(q + 1);
      end
    end
  end

endmodule

