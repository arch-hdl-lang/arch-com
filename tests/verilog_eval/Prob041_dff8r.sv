// domain SysDomain

module TopModule (
  input logic clk,
  input logic reset,
  input logic [8-1:0] d,
  output logic [8-1:0] q
);

  logic [8-1:0] q_r;
  always_ff @(posedge clk) begin
    if (reset) begin
      q_r <= 0;
    end else begin
      q_r <= d;
    end
  end
  assign q = q_r;

endmodule

