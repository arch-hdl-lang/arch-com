// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  output logic [10-1:0] q
);

  logic [10-1:0] cnt;
  always_ff @(posedge clk) begin
    if (reset) begin
      cnt <= 0;
    end else begin
      if ((cnt == 999)) begin
        cnt <= 0;
      end else begin
        cnt <= 10'((cnt + 1));
      end
    end
  end
  assign q = cnt;

endmodule

