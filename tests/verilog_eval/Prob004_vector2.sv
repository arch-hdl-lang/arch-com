// VerilogEval Prob004: Reverse byte order of 32-bit vector
module TopModule (
  input logic [32-1:0] in,
  output logic [32-1:0] out
);

  always_comb begin
    for (int i = 0; i <= 7; i++) begin
      out[i] = in[(24 + i)];
      out[(8 + i)] = in[(16 + i)];
      out[(16 + i)] = in[(8 + i)];
      out[(24 + i)] = in[i];
    end
  end

endmodule

