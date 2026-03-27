// domain SysDomain

module TopModule (
  input logic clk,
  input logic in,
  output logic out
);

  logic q_r;
  always_ff @(posedge clk) begin
    q_r <= in ^ q_r;
  end
  assign out = q_r;

endmodule

