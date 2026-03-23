// domain SysDomain

module TopModule (
  input logic clk,
  input logic l_sig,
  input logic q_in,
  input logic r_in,
  output logic q_sig
);

  logic q_r;
  always_ff @(posedge clk) begin
    if (l_sig) begin
      q_r <= r_in;
    end else begin
      q_r <= q_in;
    end
  end
  assign q_sig = q_r;

endmodule

