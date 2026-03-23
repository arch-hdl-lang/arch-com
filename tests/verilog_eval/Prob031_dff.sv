// domain SysDomain

module TopModule (
  input logic clk,
  input logic d,
  output logic q
);

  logic q_r;
  always_ff @(posedge clk) begin
    q_r <= d;
  end
  assign q = q_r;

endmodule

