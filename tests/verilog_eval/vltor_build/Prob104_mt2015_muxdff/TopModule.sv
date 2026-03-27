// domain SysDomain

module TopModule (
  input logic clk,
  input logic L,
  input logic q_in,
  input logic r_in,
  output logic Q
);

  logic q_r;
  always_ff @(posedge clk) begin
    if (L) begin
      q_r <= r_in;
    end else begin
      q_r <= q_in;
    end
  end
  assign Q = q_r;

endmodule

