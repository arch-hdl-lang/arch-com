module TopModule (
  input logic [1024-1:0] in,
  input logic [8-1:0] sel,
  output logic [4-1:0] out
);

  always_comb begin
    for (int i = 0; i <= 3; i++) begin
      out[i] = in[sel * 4 + i];
    end
  end

endmodule

