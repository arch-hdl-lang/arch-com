// domain SysDomain

module TopModule (
  input logic clk,
  input logic in_sig,
  output logic out_sig
);

  logic q_r;
  always_ff @(posedge clk) begin
    q_r <= (in_sig ^ q_r);
  end
  assign out_sig = q_r;

endmodule

