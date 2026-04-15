module Binary2BCD (
  input logic [7:0] num,
  output logic [3:0] thousand,
  output logic [3:0] hundred,
  output logic [3:0] ten,
  output logic [3:0] one
);

  // Double-dabble: 20-bit shift register [19:16]=thousand [15:12]=hundred [11:8]=ten [7:0]=input
  logic [19:0] sh0;
  logic [19:0] sh1;
  logic [19:0] sh2;
  logic [19:0] sh3;
  logic [19:0] sh4;
  logic [19:0] sh5;
  logic [19:0] sh6;
  logic [19:0] sh7;
  logic [19:0] sh8;
  always_comb begin
    sh0 = 20'($unsigned(num));
    // Iteration 1: just shift (no BCD digits populated yet)
    sh1 = sh0 << 1;
    // Iteration 2
    sh2[7:0] = sh1[7:0];
    sh2[11:8] = sh1[11:8];
    if (sh1[11:8] >= 5) begin
      sh2[11:8] = 4'(sh1[11:8] + 3);
    end
    sh2[19:12] = sh1[19:12];
    sh2 = sh2 << 1;
    // Iteration 3
    sh3[7:0] = sh2[7:0];
    sh3[11:8] = sh2[11:8];
    if (sh2[11:8] >= 5) begin
      sh3[11:8] = 4'(sh2[11:8] + 3);
    end
    sh3[19:12] = sh2[19:12];
    sh3 = sh3 << 1;
    // Iteration 4
    sh4[7:0] = sh3[7:0];
    sh4[11:8] = sh3[11:8];
    if (sh3[11:8] >= 5) begin
      sh4[11:8] = 4'(sh3[11:8] + 3);
    end
    sh4[15:12] = sh3[15:12];
    if (sh3[15:12] >= 5) begin
      sh4[15:12] = 4'(sh3[15:12] + 3);
    end
    sh4[19:16] = sh3[19:16];
    sh4 = sh4 << 1;
    // Iteration 5
    sh5[7:0] = sh4[7:0];
    sh5[11:8] = sh4[11:8];
    if (sh4[11:8] >= 5) begin
      sh5[11:8] = 4'(sh4[11:8] + 3);
    end
    sh5[15:12] = sh4[15:12];
    if (sh4[15:12] >= 5) begin
      sh5[15:12] = 4'(sh4[15:12] + 3);
    end
    sh5[19:16] = sh4[19:16];
    sh5 = sh5 << 1;
    // Iteration 6
    sh6[7:0] = sh5[7:0];
    sh6[11:8] = sh5[11:8];
    if (sh5[11:8] >= 5) begin
      sh6[11:8] = 4'(sh5[11:8] + 3);
    end
    sh6[15:12] = sh5[15:12];
    if (sh5[15:12] >= 5) begin
      sh6[15:12] = 4'(sh5[15:12] + 3);
    end
    sh6[19:16] = sh5[19:16];
    sh6 = sh6 << 1;
    // Iteration 7
    sh7[7:0] = sh6[7:0];
    sh7[11:8] = sh6[11:8];
    if (sh6[11:8] >= 5) begin
      sh7[11:8] = 4'(sh6[11:8] + 3);
    end
    sh7[15:12] = sh6[15:12];
    if (sh6[15:12] >= 5) begin
      sh7[15:12] = 4'(sh6[15:12] + 3);
    end
    sh7[19:16] = sh6[19:16];
    sh7 = sh7 << 1;
    // Iteration 8
    sh8[7:0] = sh7[7:0];
    sh8[11:8] = sh7[11:8];
    if (sh7[11:8] >= 5) begin
      sh8[11:8] = 4'(sh7[11:8] + 3);
    end
    sh8[15:12] = sh7[15:12];
    if (sh7[15:12] >= 5) begin
      sh8[15:12] = 4'(sh7[15:12] + 3);
    end
    sh8[19:16] = sh7[19:16];
    sh8 = sh8 << 1;
    // For 8-bit input (0..255), valid BCD digits are in sh8[19:8].
    thousand = 0;
    hundred = sh8[19:16];
    ten = sh8[15:12];
    one = sh8[11:8];
  end

endmodule

