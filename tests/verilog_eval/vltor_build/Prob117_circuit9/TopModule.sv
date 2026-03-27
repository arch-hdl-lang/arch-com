// domain SysDomain

module TopModule (
  input logic clk,
  input logic a,
  output logic [3-1:0] q
);

  logic [3-1:0] cnt;
  always_ff @(posedge clk) begin
    if (a) begin
      cnt <= 4;
    end else if (cnt == 6) begin
      cnt <= 0;
    end else begin
      cnt <= 3'(cnt + 1);
    end
  end
  assign q = cnt;

endmodule

