// domain SysDomain

module TopModule (
  input logic clk,
  input logic r,
  input logic d,
  output logic q
);

  logic q_r;
  always_ff @(posedge clk) begin
    if (r) begin
      q_r <= 0;
    end else begin
      q_r <= d;
    end
  end
  assign q = q_r;

endmodule

