// domain SysDomain

module TopModule (
  input logic clk,
  input logic a,
  output logic q
);

  logic q_r;
  always_ff @(posedge clk) begin
    q_r <= ~a;
  end
  assign q = q_r;

endmodule

