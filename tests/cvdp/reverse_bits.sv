module reverse_bits (
  input logic [32-1:0] num_in,
  output logic [32-1:0] num_out
);

  always_comb begin
    for (int i = 0; i <= 31; i++) begin
      num_out[i] = num_in[31 - i];
    end
  end

endmodule

