// domain SysDomain

module TopModule (
  input logic clk,
  input logic enable,
  input logic s_sig,
  input logic a_sig,
  input logic b_sig,
  input logic c_sig,
  output logic z_sig
);

  logic [8-1:0] sr;
  always_ff @(posedge clk) begin
    if (enable) begin
      sr[0] <= s_sig;
      for (int i = 1; i <= 7; i++) begin
        sr[i] <= sr[(i - 1)];
      end
    end
  end
  logic [3-1:0] sel;
  assign sel[2] = a_sig;
  assign sel[1] = b_sig;
  assign sel[0] = c_sig;
  assign z_sig = sr[sel];

endmodule

