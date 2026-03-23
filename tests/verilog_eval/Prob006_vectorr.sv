module TopModule (
  input logic [8-1:0] in_sig,
  output logic [8-1:0] out_sig
);

  always_comb begin
    for (int i = 0; i <= 7; i++) begin
      out_sig[i] = in_sig[(7 - i)];
    end
  end

endmodule

