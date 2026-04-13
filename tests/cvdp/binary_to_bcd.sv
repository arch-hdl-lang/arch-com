module binary_to_bcd (
  input logic [7:0] binary_in,
  output logic [11:0] bcd_out
);

  // Double Dabble: 8 steps, each step: add-3 to nibbles >= 5, then shift left 1
  // 20-bit register: [19:16]=hundreds, [15:12]=tens, [11:8]=ones, [7:0]=binary shift-in
  logic [19:0] s0;
  logic [19:0] s1;
  logic [19:0] s2;
  logic [19:0] s3;
  logic [19:0] s4;
  logic [19:0] s5;
  logic [19:0] s6;
  logic [19:0] s7;
  logic [19:0] s8;
  logic [19:0] a0;
  logic [19:0] a1;
  logic [19:0] a2;
  logic [19:0] a3;
  logic [19:0] a4;
  logic [19:0] a5;
  logic [19:0] a6;
  logic [19:0] a7;
  // shifted versions (21-bit to avoid losing the top bit in shift expr)
  logic [20:0] sh0;
  logic [20:0] sh1;
  logic [20:0] sh2;
  logic [20:0] sh3;
  logic [20:0] sh4;
  logic [20:0] sh5;
  logic [20:0] sh6;
  logic [20:0] sh7;
  assign s0 = 20'($unsigned(binary_in));
  // Step 1 adjust: check nibbles >= 5, add 3 if so
  assign a0 = 20'($unsigned(s0[19:16] >= 5 ? 4'(s0[19:16] + 3) : s0[19:16])) << 16 | 20'($unsigned(s0[15:12] >= 5 ? 4'(s0[15:12] + 3) : s0[15:12])) << 12 | 20'($unsigned(s0[11:8] >= 5 ? 4'(s0[11:8] + 3) : s0[11:8])) << 8 | 20'($unsigned(s0[7:0]));
  assign sh0 = 21'($unsigned(a0)) << 1;
  assign s1 = sh0[19:0];
  // Step 2 adjust
  assign a1 = 20'($unsigned(s1[19:16] >= 5 ? 4'(s1[19:16] + 3) : s1[19:16])) << 16 | 20'($unsigned(s1[15:12] >= 5 ? 4'(s1[15:12] + 3) : s1[15:12])) << 12 | 20'($unsigned(s1[11:8] >= 5 ? 4'(s1[11:8] + 3) : s1[11:8])) << 8 | 20'($unsigned(s1[7:0]));
  assign sh1 = 21'($unsigned(a1)) << 1;
  assign s2 = sh1[19:0];
  // Step 3 adjust
  assign a2 = 20'($unsigned(s2[19:16] >= 5 ? 4'(s2[19:16] + 3) : s2[19:16])) << 16 | 20'($unsigned(s2[15:12] >= 5 ? 4'(s2[15:12] + 3) : s2[15:12])) << 12 | 20'($unsigned(s2[11:8] >= 5 ? 4'(s2[11:8] + 3) : s2[11:8])) << 8 | 20'($unsigned(s2[7:0]));
  assign sh2 = 21'($unsigned(a2)) << 1;
  assign s3 = sh2[19:0];
  // Step 4 adjust
  assign a3 = 20'($unsigned(s3[19:16] >= 5 ? 4'(s3[19:16] + 3) : s3[19:16])) << 16 | 20'($unsigned(s3[15:12] >= 5 ? 4'(s3[15:12] + 3) : s3[15:12])) << 12 | 20'($unsigned(s3[11:8] >= 5 ? 4'(s3[11:8] + 3) : s3[11:8])) << 8 | 20'($unsigned(s3[7:0]));
  assign sh3 = 21'($unsigned(a3)) << 1;
  assign s4 = sh3[19:0];
  // Step 5 adjust
  assign a4 = 20'($unsigned(s4[19:16] >= 5 ? 4'(s4[19:16] + 3) : s4[19:16])) << 16 | 20'($unsigned(s4[15:12] >= 5 ? 4'(s4[15:12] + 3) : s4[15:12])) << 12 | 20'($unsigned(s4[11:8] >= 5 ? 4'(s4[11:8] + 3) : s4[11:8])) << 8 | 20'($unsigned(s4[7:0]));
  assign sh4 = 21'($unsigned(a4)) << 1;
  assign s5 = sh4[19:0];
  // Step 6 adjust
  assign a5 = 20'($unsigned(s5[19:16] >= 5 ? 4'(s5[19:16] + 3) : s5[19:16])) << 16 | 20'($unsigned(s5[15:12] >= 5 ? 4'(s5[15:12] + 3) : s5[15:12])) << 12 | 20'($unsigned(s5[11:8] >= 5 ? 4'(s5[11:8] + 3) : s5[11:8])) << 8 | 20'($unsigned(s5[7:0]));
  assign sh5 = 21'($unsigned(a5)) << 1;
  assign s6 = sh5[19:0];
  // Step 7 adjust
  assign a6 = 20'($unsigned(s6[19:16] >= 5 ? 4'(s6[19:16] + 3) : s6[19:16])) << 16 | 20'($unsigned(s6[15:12] >= 5 ? 4'(s6[15:12] + 3) : s6[15:12])) << 12 | 20'($unsigned(s6[11:8] >= 5 ? 4'(s6[11:8] + 3) : s6[11:8])) << 8 | 20'($unsigned(s6[7:0]));
  assign sh6 = 21'($unsigned(a6)) << 1;
  assign s7 = sh6[19:0];
  // Step 8 adjust
  assign a7 = 20'($unsigned(s7[19:16] >= 5 ? 4'(s7[19:16] + 3) : s7[19:16])) << 16 | 20'($unsigned(s7[15:12] >= 5 ? 4'(s7[15:12] + 3) : s7[15:12])) << 12 | 20'($unsigned(s7[11:8] >= 5 ? 4'(s7[11:8] + 3) : s7[11:8])) << 8 | 20'($unsigned(s7[7:0]));
  assign sh7 = 21'($unsigned(a7)) << 1;
  assign s8 = sh7[19:0];
  assign bcd_out = s8[19:8];

endmodule

