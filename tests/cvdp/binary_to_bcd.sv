module binary_to_bcd (
  input logic [8-1:0] binary_in,
  output logic [12-1:0] bcd_out
);

  // Double-dabble: 20-bit shift register, 8 iterations
  // Each iteration: check each BCD nibble >= 5 → add 3, then shift left
  logic [20-1:0] s0;
  logic [20-1:0] s1;
  logic [20-1:0] s2;
  logic [20-1:0] s3;
  logic [20-1:0] s4;
  logic [20-1:0] s5;
  logic [20-1:0] s6;
  logic [20-1:0] s7;
  logic [20-1:0] s8;
  always_comb begin
    s0 = 20'($unsigned(binary_in));
    s1 = s0 << 1;
    s2[7:0] = s1[7:0];
    s2[11:8] = s1[11:8];
    if (s1[11:8] >= 5) begin
      s2[11:8] = 4'(s1[11:8] + 3);
    end
    s2[19:12] = s1[19:12];
    s2 = s2 << 1;
    s3[7:0] = s2[7:0];
    s3[11:8] = s2[11:8];
    if (s2[11:8] >= 5) begin
      s3[11:8] = 4'(s2[11:8] + 3);
    end
    s3[19:12] = s2[19:12];
    s3 = s3 << 1;
    s4[7:0] = s3[7:0];
    s4[11:8] = s3[11:8];
    if (s3[11:8] >= 5) begin
      s4[11:8] = 4'(s3[11:8] + 3);
    end
    s4[15:12] = s3[15:12];
    if (s3[15:12] >= 5) begin
      s4[15:12] = 4'(s3[15:12] + 3);
    end
    s4[19:16] = s3[19:16];
    s4 = s4 << 1;
    s5[7:0] = s4[7:0];
    s5[11:8] = s4[11:8];
    if (s4[11:8] >= 5) begin
      s5[11:8] = 4'(s4[11:8] + 3);
    end
    s5[15:12] = s4[15:12];
    if (s4[15:12] >= 5) begin
      s5[15:12] = 4'(s4[15:12] + 3);
    end
    s5[19:16] = s4[19:16];
    s5 = s5 << 1;
    s6[7:0] = s5[7:0];
    s6[11:8] = s5[11:8];
    if (s5[11:8] >= 5) begin
      s6[11:8] = 4'(s5[11:8] + 3);
    end
    s6[15:12] = s5[15:12];
    if (s5[15:12] >= 5) begin
      s6[15:12] = 4'(s5[15:12] + 3);
    end
    s6[19:16] = s5[19:16];
    s6 = s6 << 1;
    s7[7:0] = s6[7:0];
    s7[11:8] = s6[11:8];
    if (s6[11:8] >= 5) begin
      s7[11:8] = 4'(s6[11:8] + 3);
    end
    s7[15:12] = s6[15:12];
    if (s6[15:12] >= 5) begin
      s7[15:12] = 4'(s6[15:12] + 3);
    end
    s7[19:16] = s6[19:16];
    s7 = s7 << 1;
    s8[7:0] = s7[7:0];
    s8[11:8] = s7[11:8];
    if (s7[11:8] >= 5) begin
      s8[11:8] = 4'(s7[11:8] + 3);
    end
    s8[15:12] = s7[15:12];
    if (s7[15:12] >= 5) begin
      s8[15:12] = 4'(s7[15:12] + 3);
    end
    s8[19:16] = s7[19:16];
    s8 = s8 << 1;
    bcd_out = s8[19:8];
  end

endmodule

// Iteration 1: shift left, then adjust
// Iteration 2
// Iteration 3
// Iteration 4
// Iteration 5
// Iteration 6
// Iteration 7
// Iteration 8 (last): adjust then shift
