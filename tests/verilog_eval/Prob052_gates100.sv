module TopModule (
  input logic [100-1:0] in_sig,
  output logic out_and,
  output logic out_or,
  output logic out_xor
);

  logic a;
  logic o;
  logic x;
  always_comb begin
    a = in_sig[0];
    o = in_sig[0];
    x = in_sig[0];
    for (int i = 1; i <= 99; i++) begin
      a = (a & in_sig[i]);
      o = (o | in_sig[i]);
      x = (x ^ in_sig[i]);
    end
    out_and = a;
    out_or = o;
    out_xor = x;
  end

endmodule

