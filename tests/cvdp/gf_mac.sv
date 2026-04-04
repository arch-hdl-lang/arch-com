module gf_mac #(
  parameter int WIDTH = 8
) (
  input logic [WIDTH-1:0] a,
  input logic [WIDTH-1:0] b,
  output logic [8-1:0] result
);

  // GF(2^8) multiply-accumulate: XOR of all segment-wise GF products
  // Each segment is 8 bits. This implementation supports WIDTH=8 (1 segment).
  // Irreducible polynomial: x^8 + x^4 + x^3 + x + 1 (0x11B), reduction byte 0x1B
  // Segment 0: compute gf_mul(a[7:0], b[7:0]) using Russian peasant algorithm
  logic [9-1:0] sh0s0;
  logic [9-1:0] sh1s0;
  logic [9-1:0] sh2s0;
  logic [9-1:0] sh3s0;
  logic [9-1:0] sh4s0;
  logic [9-1:0] sh5s0;
  logic [9-1:0] sh6s0;
  logic [8-1:0] a0s0;
  logic [8-1:0] a1s0;
  logic [8-1:0] a2s0;
  logic [8-1:0] a3s0;
  logic [8-1:0] a4s0;
  logic [8-1:0] a5s0;
  logic [8-1:0] a6s0;
  logic [8-1:0] p0s0;
  logic [8-1:0] p1s0;
  logic [8-1:0] p2s0;
  logic [8-1:0] p3s0;
  logic [8-1:0] p4s0;
  logic [8-1:0] p5s0;
  logic [8-1:0] p6s0;
  logic [8-1:0] gfp0;
  assign p0s0 = b[0:0] == 1 ? a[7:0] : 8'($unsigned(0));
  assign sh0s0 = 9'($unsigned(a[7:0])) << 1;
  assign a0s0 = sh0s0[8:8] == 1 ? sh0s0[7:0] ^ 8'($unsigned('h1B)) : sh0s0[7:0];
  assign p1s0 = b[1:1] == 1 ? p0s0 ^ a0s0 : p0s0;
  assign sh1s0 = 9'($unsigned(a0s0)) << 1;
  assign a1s0 = sh1s0[8:8] == 1 ? sh1s0[7:0] ^ 8'($unsigned('h1B)) : sh1s0[7:0];
  assign p2s0 = b[2:2] == 1 ? p1s0 ^ a1s0 : p1s0;
  assign sh2s0 = 9'($unsigned(a1s0)) << 1;
  assign a2s0 = sh2s0[8:8] == 1 ? sh2s0[7:0] ^ 8'($unsigned('h1B)) : sh2s0[7:0];
  assign p3s0 = b[3:3] == 1 ? p2s0 ^ a2s0 : p2s0;
  assign sh3s0 = 9'($unsigned(a2s0)) << 1;
  assign a3s0 = sh3s0[8:8] == 1 ? sh3s0[7:0] ^ 8'($unsigned('h1B)) : sh3s0[7:0];
  assign p4s0 = b[4:4] == 1 ? p3s0 ^ a3s0 : p3s0;
  assign sh4s0 = 9'($unsigned(a3s0)) << 1;
  assign a4s0 = sh4s0[8:8] == 1 ? sh4s0[7:0] ^ 8'($unsigned('h1B)) : sh4s0[7:0];
  assign p5s0 = b[5:5] == 1 ? p4s0 ^ a4s0 : p4s0;
  assign sh5s0 = 9'($unsigned(a4s0)) << 1;
  assign a5s0 = sh5s0[8:8] == 1 ? sh5s0[7:0] ^ 8'($unsigned('h1B)) : sh5s0[7:0];
  assign p6s0 = b[6:6] == 1 ? p5s0 ^ a5s0 : p5s0;
  assign sh6s0 = 9'($unsigned(a5s0)) << 1;
  assign a6s0 = sh6s0[8:8] == 1 ? sh6s0[7:0] ^ 8'($unsigned('h1B)) : sh6s0[7:0];
  assign gfp0 = b[7:7] == 1 ? p6s0 ^ a6s0 : p6s0;
  assign result = gfp0;

endmodule

// iteration 0: accumulate a if b[0]=1
// iteration 1
// iteration 2
// iteration 3
// iteration 4
// iteration 5
// iteration 6
// iteration 7
