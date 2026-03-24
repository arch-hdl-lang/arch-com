module TopModule (
  input logic [100-1:0] in,
  output logic out_and,
  output logic out_or,
  output logic out_xor
);

  logic a;
  logic o;
  logic x;
  always_comb begin
    a = in[0];
    o = in[0];
    x = in[0];
    for (int i = 1; i <= 99; i++) begin
      a = (a & in[i]);
      o = (o | in[i]);
      x = (x ^ in[i]);
    end
    out_and = a;
    out_or = o;
    out_xor = x;
  end

endmodule

