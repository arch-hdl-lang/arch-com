// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  output logic [4-1:0] q
);

  logic [4-1:0] cnt;
  always_ff @(posedge clk) begin
    if (reset) begin
      cnt <= 1;
    end else begin
      if (cnt == 10) begin
        cnt <= 1;
      end else begin
        cnt <= 4'(cnt + 1);
      end
    end
  end
  assign q = cnt;

endmodule

