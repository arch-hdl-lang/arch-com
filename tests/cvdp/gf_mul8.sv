module gf_mul8 (
  input logic [8-1:0] a_in,
  input logic [8-1:0] b_in,
  output logic [8-1:0] p_out
);

  // Russian peasant GF(2^8) multiply, irreducible poly 0x11B
  // Unrolled 8 iterations. Shift left: extend to 9 bits, shift, XOR upper bit check.
  logic [9-1:0] sh0;
  logic [9-1:0] sh1;
  logic [9-1:0] sh2;
  logic [9-1:0] sh3;
  logic [9-1:0] sh4;
  logic [9-1:0] sh5;
  logic [9-1:0] sh6;
  logic [8-1:0] p0;
  logic [8-1:0] a0;
  logic [8-1:0] p1;
  logic [8-1:0] a1;
  logic [8-1:0] p2;
  logic [8-1:0] a2;
  logic [8-1:0] p3;
  logic [8-1:0] a3;
  logic [8-1:0] p4;
  logic [8-1:0] a4;
  logic [8-1:0] p5;
  logic [8-1:0] a5;
  logic [8-1:0] p6;
  logic [8-1:0] a6;
  logic [8-1:0] p7;
  assign p0 = b_in[0:0] == 1 ? a_in : 8'($unsigned(0));
  assign sh0 = 9'($unsigned(a_in)) << 1;
  assign a0 = sh0[8:8] == 1 ? sh0[7:0] ^ 8'($unsigned('h1B)) : sh0[7:0];
  assign p1 = b_in[1:1] == 1 ? p0 ^ a0 : p0;
  assign sh1 = 9'($unsigned(a0)) << 1;
  assign a1 = sh1[8:8] == 1 ? sh1[7:0] ^ 8'($unsigned('h1B)) : sh1[7:0];
  assign p2 = b_in[2:2] == 1 ? p1 ^ a1 : p1;
  assign sh2 = 9'($unsigned(a1)) << 1;
  assign a2 = sh2[8:8] == 1 ? sh2[7:0] ^ 8'($unsigned('h1B)) : sh2[7:0];
  assign p3 = b_in[3:3] == 1 ? p2 ^ a2 : p2;
  assign sh3 = 9'($unsigned(a2)) << 1;
  assign a3 = sh3[8:8] == 1 ? sh3[7:0] ^ 8'($unsigned('h1B)) : sh3[7:0];
  assign p4 = b_in[4:4] == 1 ? p3 ^ a3 : p3;
  assign sh4 = 9'($unsigned(a3)) << 1;
  assign a4 = sh4[8:8] == 1 ? sh4[7:0] ^ 8'($unsigned('h1B)) : sh4[7:0];
  assign p5 = b_in[5:5] == 1 ? p4 ^ a4 : p4;
  assign sh5 = 9'($unsigned(a4)) << 1;
  assign a5 = sh5[8:8] == 1 ? sh5[7:0] ^ 8'($unsigned('h1B)) : sh5[7:0];
  assign p6 = b_in[6:6] == 1 ? p5 ^ a5 : p5;
  assign sh6 = 9'($unsigned(a5)) << 1;
  assign a6 = sh6[8:8] == 1 ? sh6[7:0] ^ 8'($unsigned('h1B)) : sh6[7:0];
  assign p7 = b_in[7:7] == 1 ? p6 ^ a6 : p6;
  assign p_out = p7;

endmodule

// iteration 0
// iteration 1
// iteration 2
// iteration 3
// iteration 4
// iteration 5
// iteration 6
// iteration 7
