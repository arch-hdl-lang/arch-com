// VerilogEval Prob015: Split 16-bit into hi/lo bytes
module TopModule (
  input logic [16-1:0] in_sig,
  output logic [8-1:0] out_hi,
  output logic [8-1:0] out_lo
);

  always_comb begin
    for (int i = 0; i <= 7; i++) begin
      out_lo[i] = in_sig[i];
      out_hi[i] = in_sig[(8 + i)];
    end
  end

endmodule

