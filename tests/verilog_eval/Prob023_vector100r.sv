module TopModule (
  input logic [100-1:0] in_sig,
  output logic [100-1:0] out_sig
);

  always_comb begin
    for (int i = 0; i <= 99; i++) begin
      out_sig[i] = in_sig[(99 - i)];
    end
  end

endmodule

