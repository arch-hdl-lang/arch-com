// VerilogEval Prob004: Reverse byte order of 32-bit vector
module TopModule (
  input logic [32-1:0] in_sig,
  output logic [32-1:0] out_sig
);

  always_comb begin
    for (int i = 0; i <= 7; i++) begin
      out_sig[i] = in_sig[(24 + i)];
      out_sig[(8 + i)] = in_sig[(16 + i)];
      out_sig[(16 + i)] = in_sig[(8 + i)];
      out_sig[(24 + i)] = in_sig[i];
    end
  end

endmodule

