module brent_kung_adder (
  input logic [31:0] a,
  input logic [31:0] b,
  input logic carry_in,
  output logic [31:0] sum,
  output logic carry_out
);

  // Initial propagate and generate
  logic [31:0] p0;
  logic [31:0] g0;
  // Up-sweep level 1: stride 2, positions 1,3,5,...,31
  logic [31:0] g1;
  logic [31:0] p1;
  // Up-sweep level 2: stride 4, positions 3,7,11,...,31
  logic [31:0] g2;
  logic [31:0] p2;
  // Up-sweep level 3: stride 8, positions 7,15,23,31
  logic [31:0] g3;
  logic [31:0] p3;
  // Up-sweep level 4: stride 16, positions 15,31
  logic [31:0] g4;
  logic [31:0] p4;
  // Up-sweep level 5: position 31 only
  logic [31:0] g5;
  logic [31:0] p5;
  // Down-sweep level 1: position 23
  logic [31:0] g6;
  logic [31:0] p6;
  // Down-sweep level 2: positions 11,19,27
  logic [31:0] g7;
  logic [31:0] p7;
  // Down-sweep level 3: odd positions 5,9,13,17,21,25,29
  logic [31:0] g8;
  logic [31:0] p8;
  // Down-sweep level 4: even positions 2,4,6,...,30
  logic [31:0] g9;
  logic [31:0] p9;
  // Carry vector
  logic [32:0] c;
  always_comb begin
    // Initial P and G
    for (int i = 0; i <= 31; i++) begin
      p0[i] = (a[i +: 1] != 0) ^ (b[i +: 1] != 0);
      g0[i] = (a[i +: 1] != 0) & (b[i +: 1] != 0);
    end
    // === Up-sweep level 1: combine (2i+1, 2i) ===
    for (int i = 0; i <= 31; i++) begin
      g1[i] = g0[i];
      p1[i] = p0[i];
    end
    for (int i = 0; i <= 15; i++) begin
      g1[2 * i + 1] = g0[2 * i + 1] | (p0[2 * i + 1] & g0[2 * i]);
      p1[2 * i + 1] = p0[2 * i + 1] & p0[2 * i];
    end
    // === Up-sweep level 2: combine (4i+3, 4i+1) ===
    for (int i = 0; i <= 31; i++) begin
      g2[i] = g1[i];
      p2[i] = p1[i];
    end
    for (int i = 0; i <= 7; i++) begin
      g2[4 * i + 3] = g1[4 * i + 3] | (p1[4 * i + 3] & g1[4 * i + 1]);
      p2[4 * i + 3] = p1[4 * i + 3] & p1[4 * i + 1];
    end
    // === Up-sweep level 3: combine (8i+7, 8i+3) ===
    for (int i = 0; i <= 31; i++) begin
      g3[i] = g2[i];
      p3[i] = p2[i];
    end
    for (int i = 0; i <= 3; i++) begin
      g3[8 * i + 7] = g2[8 * i + 7] | (p2[8 * i + 7] & g2[8 * i + 3]);
      p3[8 * i + 7] = p2[8 * i + 7] & p2[8 * i + 3];
    end
    // === Up-sweep level 4: combine (16i+15, 16i+7) ===
    for (int i = 0; i <= 31; i++) begin
      g4[i] = g3[i];
      p4[i] = p3[i];
    end
    for (int i = 0; i <= 1; i++) begin
      g4[16 * i + 15] = g3[16 * i + 15] | (p3[16 * i + 15] & g3[16 * i + 7]);
      p4[16 * i + 15] = p3[16 * i + 15] & p3[16 * i + 7];
    end
    // === Up-sweep level 5: combine (31, 15) ===
    for (int i = 0; i <= 31; i++) begin
      g5[i] = g4[i];
      p5[i] = p4[i];
    end
    g5[31] = g4[31] | (p4[31] & g4[15]);
    p5[31] = p4[31] & p4[15];
    // === Down-sweep level 1: position 23 from (23, 15) ===
    for (int i = 0; i <= 31; i++) begin
      g6[i] = g5[i];
      p6[i] = p5[i];
    end
    g6[23] = g5[23] | (p5[23] & g5[15]);
    p6[23] = p5[23] & p5[15];
    // === Down-sweep level 2: positions 11,19,27 ===
    for (int i = 0; i <= 31; i++) begin
      g7[i] = g6[i];
      p7[i] = p6[i];
    end
    g7[11] = g6[11] | (p6[11] & g6[7]);
    p7[11] = p6[11] & p6[7];
    g7[19] = g6[19] | (p6[19] & g6[15]);
    p7[19] = p6[19] & p6[15];
    g7[27] = g6[27] | (p6[27] & g6[23]);
    p7[27] = p6[27] & p6[23];
    // === Down-sweep level 3: positions 5,9,13,17,21,25,29 ===
    for (int i = 0; i <= 31; i++) begin
      g8[i] = g7[i];
      p8[i] = p7[i];
    end
    g8[5] = g7[5] | (p7[5] & g7[3]);
    p8[5] = p7[5] & p7[3];
    g8[9] = g7[9] | (p7[9] & g7[7]);
    p8[9] = p7[9] & p7[7];
    g8[13] = g7[13] | (p7[13] & g7[11]);
    p8[13] = p7[13] & p7[11];
    g8[17] = g7[17] | (p7[17] & g7[15]);
    p8[17] = p7[17] & p7[15];
    g8[21] = g7[21] | (p7[21] & g7[19]);
    p8[21] = p7[21] & p7[19];
    g8[25] = g7[25] | (p7[25] & g7[23]);
    p8[25] = p7[25] & p7[23];
    g8[29] = g7[29] | (p7[29] & g7[27]);
    p8[29] = p7[29] & p7[27];
    // === Down-sweep level 4: even positions 2,4,6,...,30 ===
    for (int i = 0; i <= 31; i++) begin
      g9[i] = g8[i];
      p9[i] = p8[i];
    end
    for (int i = 1; i <= 15; i++) begin
      g9[2 * i] = g8[2 * i] | (p8[2 * i] & g8[2 * i - 1]);
      p9[2 * i] = p8[2 * i] & p8[2 * i - 1];
    end
    // === Compute carries ===
    c[0] = carry_in;
    for (int i = 0; i <= 31; i++) begin
      c[i + 1] = g9[i] | (p9[i] & c[i]);
    end
    // === Compute sum and carry_out ===
    for (int i = 0; i <= 31; i++) begin
      sum[i] = p0[i] ^ c[i];
    end
    carry_out = c[32];
  end

endmodule

