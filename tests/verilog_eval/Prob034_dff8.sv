// domain SysDomain

module TopModule (
  input logic clk,
  input logic [8-1:0] d,
  output logic [8-1:0] q
);

  logic [8-1:0] q_r;
  always_ff @(posedge clk) begin
    q_r <= d;
  end
  assign q = q_r;

endmodule

