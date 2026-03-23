module TopModule (
  input logic [8-1:0] in_sig,
  output logic [32-1:0] out_sig
);

  always_comb begin
    for (int i = 0; i <= 7; i++) begin
      out_sig[i] = in_sig[i];
    end
    for (int i = 8; i <= 31; i++) begin
      out_sig[i] = in_sig[7];
    end
  end

endmodule

