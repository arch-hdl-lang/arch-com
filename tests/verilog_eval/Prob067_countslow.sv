// domain SysDomain

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
      if (slowena) begin
        if (q == 9) begin
          q <= 0;
        end else begin
          q <= 4'(q + 1);
        end
      end
    end
  end

endmodule

