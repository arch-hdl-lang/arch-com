// domain SysDomain

module TopModule (
  input logic clk,
  input logic j,
  input logic k,
  output logic Q
);

  logic q_r;
  always_ff @(posedge clk) begin
    if ((j & k)) begin
      q_r <= (~q_r);
    end else if (j) begin
      q_r <= 1;
    end else if (k) begin
      q_r <= 0;
    end
  end
  assign Q = q_r;

endmodule

