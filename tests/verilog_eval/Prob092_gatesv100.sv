Wrote tests/verilog_eval/Prob092_gatesv100.sv

  output logic [100-1:0] out_both,
  output logic [100-1:0] out_any,
  output logic [100-1:0] out_different
);

  always_comb begin
    for (int i = 0; i <= 98; i++) begin
      out_both[i] = in[i] & in[i + 1];
    end
    out_both[99] = 0;
    out_any[0] = 0;
    for (int i = 1; i <= 99; i++) begin
      out_any[i] = in[i] | in[i - 1];
    end
    for (int i = 0; i <= 98; i++) begin
      out_different[i] = in[i] ^ in[i + 1];
    end
    out_different[99] = in[99] ^ in[0];
  end

endmodule

