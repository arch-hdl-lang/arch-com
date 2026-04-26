module gf_mul8 (
  input logic [7:0] a_in,
  input logic [7:0] b_in,
  output logic [7:0] p_out
);

  // Russian peasant GF(2^8) multiply, irreducible poly 0x11B.
  // Loop unrolls to 8 combinational "iterations" indexed 0..7.
  //   p[i+1] = b_in[i] ? p[i] ^ a[i] : p[i]
  //   a[i+1] = (a[i] << 1) reduced mod 0x11B
  // Final result = p[8].
  logic [8:0] [7:0] p;
  logic [8:0] [7:0] a;
  logic [7:0] [8:0] sh;
  always_comb begin
    p[0] = 0;
    a[0] = a_in;
    for (int i = 0; i <= 7; i++) begin
      p[i + 1] = b_in[i +: 1] == 1 ? p[i] ^ a[i] : p[i];
      sh[i] = 9'($unsigned(a[i])) << 1;
      a[i + 1] = sh[i][8:8] == 1 ? sh[i][7:0] ^ 8'($unsigned('h1B)) : sh[i][7:0];
    end
    p_out = p[8];
  end

endmodule

