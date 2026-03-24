module TopModule (
  input logic [8-1:0] in,
  output logic [8-1:0] out
);

  always_comb begin
    for (int i = 0; i <= 7; i++) begin
      out[i] = in[(7 - i)];
    end
  end

endmodule

