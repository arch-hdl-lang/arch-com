module TopModule (
  input logic [100-1:0] in_sig,
  output logic [100-1:0] out_both,
  output logic [100-1:0] out_any,
  output logic [100-1:0] out_different
);

  always_comb begin
    for (int i = 0; i <= 98; i++) begin
      out_both[i] = (in_sig[i] & in_sig[(i + 1)]);
    end
    out_both[99] = 0;
    out_any[0] = 0;
    for (int i = 1; i <= 99; i++) begin
      out_any[i] = (in_sig[i] | in_sig[(i - 1)]);
    end
    for (int i = 0; i <= 98; i++) begin
      out_different[i] = (in_sig[i] ^ in_sig[(i + 1)]);
    end
    out_different[99] = (in_sig[99] ^ in_sig[0]);
  end

endmodule

