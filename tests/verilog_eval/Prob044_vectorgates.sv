// VerilogEval Prob044: Bitwise OR, logical OR, NOT of 3-bit vectors
module TopModule (
  input logic [3-1:0] a,
  input logic [3-1:0] b,
  output logic [3-1:0] out_or_bitwise,
  output logic out_or_logical,
  output logic [6-1:0] out_not
);

  logic [3-1:0] not_a;
  logic [3-1:0] not_b;
  always_comb begin
    out_or_bitwise = (a | b);
    out_or_logical = (((a[0] | a[1]) | a[2]) | ((b[0] | b[1]) | b[2]));
    not_a = (~a);
    not_b = (~b);
    for (int i = 0; i <= 2; i++) begin
      out_not[i] = not_a[i];
    end
    for (int i = 0; i <= 2; i++) begin
      out_not[(3 + i)] = not_b[i];
    end
  end

endmodule

