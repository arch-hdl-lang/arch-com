// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic slowena,
  output logic [4-1:0] q
);

  logic [4-1:0] cnt;
  always_ff @(posedge clk) begin
    if (reset) begin
      cnt <= 0;
    end else begin
      if (slowena) begin
        if (cnt == 9) begin
          cnt <= 0;
        end else begin
          cnt <= 4'(cnt + 1);
        end
      end
    end
  end
  assign q = cnt;

endmodule

