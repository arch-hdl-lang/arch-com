module TopModule (
  input logic [8-1:0] in,
  output logic [32-1:0] out
);

  always_comb begin
    for (int i = 0; i <= 7; i++) begin
      out[i] = in[i];
    end
    for (int i = 8; i <= 31; i++) begin
      out[i] = in[7];
    end
  end

endmodule

