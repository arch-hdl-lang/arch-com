// GF(2^8) multiply-by-2 (AES polynomial 0x11B)
// GF(2^8) multiply-by-3
// GF(2^8) multiply-by-9
// GF(2^8) multiply-by-0B
// GF(2^8) multiply-by-0D
// GF(2^8) multiply-by-0E
// Encrypt one column: 4 row bytes + 4 key bytes → 32-bit result {e0,e1,e2,e3}
// Decrypt one column
module galois_encryption #(
  parameter int NBW_DATA = 128,
  parameter int NBW_KEY = 32
) (
  input logic clk,
  input logic rst_async_n,
  input logic i_update_key,
  input logic [NBW_KEY-1:0] i_key,
  input logic i_valid,
  input logic [NBW_DATA-1:0] i_data,
  input logic i_encrypt,
  output logic [NBW_DATA-1:0] o_data,
  output logic o_valid
);

  function automatic logic [7:0] xTimes02(input logic [7:0] b);
    logic [8:0] shifted = 9'($unsigned(b)) << 1;
    logic [7:0] lo8 = shifted[7:0];
    return b[7] ? lo8 ^ 8'd27 : lo8;
  endfunction
  
  function automatic logic [7:0] xTimes03(input logic [7:0] b);
    return xTimes02(b) ^ b;
  endfunction
  
  function automatic logic [7:0] xTimes09(input logic [7:0] b);
    return xTimes02(xTimes02(xTimes02(b))) ^ b;
  endfunction
  
  function automatic logic [7:0] xTimes0B(input logic [7:0] b);
    return xTimes02(xTimes02(xTimes02(b))) ^ xTimes03(b);
  endfunction
  
  function automatic logic [7:0] xTimes0D(input logic [7:0] b);
    return xTimes02(xTimes02(xTimes02(b))) ^ xTimes02(xTimes02(b)) ^ b;
  endfunction
  
  function automatic logic [7:0] xTimes0E(input logic [7:0] b);
    return xTimes02(xTimes02(xTimes02(b))) ^ xTimes02(xTimes02(b)) ^ xTimes02(b);
  endfunction
  
  function automatic logic [31:0] encryptColumn(input logic [7:0] b0, input logic [7:0] b1, input logic [7:0] b2, input logic [7:0] b3, input logic [7:0] k0, input logic [7:0] k1, input logic [7:0] k2, input logic [7:0] k3);
    logic [7:0] e0 = xTimes02(b0) ^ xTimes03(b1) ^ b2 ^ b3 ^ k0;
    logic [7:0] e1 = b0 ^ xTimes02(b1) ^ xTimes03(b2) ^ b3 ^ k1;
    logic [7:0] e2 = b0 ^ b1 ^ xTimes02(b2) ^ xTimes03(b3) ^ k2;
    logic [7:0] e3 = xTimes03(b0) ^ b1 ^ b2 ^ xTimes02(b3) ^ k3;
    return {e0, e1, e2, e3};
  endfunction
  
  function automatic logic [31:0] decryptColumn(input logic [7:0] b0, input logic [7:0] b1, input logic [7:0] b2, input logic [7:0] b3, input logic [7:0] k0, input logic [7:0] k1, input logic [7:0] k2, input logic [7:0] k3);
    logic [7:0] x0 = b0 ^ k0;
    logic [7:0] x1 = b1 ^ k1;
    logic [7:0] x2 = b2 ^ k2;
    logic [7:0] x3 = b3 ^ k3;
    logic [7:0] d0 = xTimes0E(x0) ^ xTimes0B(x1) ^ xTimes0D(x2) ^ xTimes09(x3);
    logic [7:0] d1 = xTimes0E(x1) ^ xTimes0B(x2) ^ xTimes0D(x3) ^ xTimes09(x0);
    logic [7:0] d2 = xTimes0E(x2) ^ xTimes0B(x3) ^ xTimes0D(x0) ^ xTimes09(x1);
    logic [7:0] d3 = xTimes0E(x3) ^ xTimes0B(x0) ^ xTimes0D(x1) ^ xTimes09(x2);
    return {d0, d1, d2, d3};
  endfunction
  
  // Key register
  logic [NBW_KEY-1:0] key_reg;
  // Pipeline stage 1: latch inputs
  logic s1_valid;
  logic [NBW_DATA-1:0] s1_data;
  logic s1_encrypt;
  // Pipeline stage 2: hold computed result
  logic s2_valid;
  logic [NBW_DATA-1:0] s2_data;
  // Key bytes
  logic [7:0] k0;
  assign k0 = key_reg[31:24];
  logic [7:0] k1;
  assign k1 = key_reg[23:16];
  logic [7:0] k2;
  assign k2 = key_reg[15:8];
  logic [7:0] k3;
  assign k3 = key_reg[7:0];
  // Stage-1 data bytes: big-endian 4x4 matrix
  // layout: [row][col] at bit (120 - 8*row - 32*col) +: 8
  logic [7:0] d00;
  assign d00 = s1_data[127:120];
  logic [7:0] d10;
  assign d10 = s1_data[119:112];
  logic [7:0] d20;
  assign d20 = s1_data[111:104];
  logic [7:0] d30;
  assign d30 = s1_data[103:96];
  logic [7:0] d01;
  assign d01 = s1_data[95:88];
  logic [7:0] d11;
  assign d11 = s1_data[87:80];
  logic [7:0] d21;
  assign d21 = s1_data[79:72];
  logic [7:0] d31;
  assign d31 = s1_data[71:64];
  logic [7:0] d02;
  assign d02 = s1_data[63:56];
  logic [7:0] d12;
  assign d12 = s1_data[55:48];
  logic [7:0] d22;
  assign d22 = s1_data[47:40];
  logic [7:0] d32;
  assign d32 = s1_data[39:32];
  logic [7:0] d03;
  assign d03 = s1_data[31:24];
  logic [7:0] d13;
  assign d13 = s1_data[23:16];
  logic [7:0] d23;
  assign d23 = s1_data[15:8];
  logic [7:0] d33;
  assign d33 = s1_data[7:0];
  // Process each column
  logic [31:0] enc_col0;
  assign enc_col0 = encryptColumn(d00, d10, d20, d30, k0, k1, k2, k3);
  logic [31:0] enc_col1;
  assign enc_col1 = encryptColumn(d01, d11, d21, d31, k0, k1, k2, k3);
  logic [31:0] enc_col2;
  assign enc_col2 = encryptColumn(d02, d12, d22, d32, k0, k1, k2, k3);
  logic [31:0] enc_col3;
  assign enc_col3 = encryptColumn(d03, d13, d23, d33, k0, k1, k2, k3);
  logic [31:0] dec_col0;
  assign dec_col0 = decryptColumn(d00, d10, d20, d30, k0, k1, k2, k3);
  logic [31:0] dec_col1;
  assign dec_col1 = decryptColumn(d01, d11, d21, d31, k0, k1, k2, k3);
  logic [31:0] dec_col2;
  assign dec_col2 = decryptColumn(d02, d12, d22, d32, k0, k1, k2, k3);
  logic [31:0] dec_col3;
  assign dec_col3 = decryptColumn(d03, d13, d23, d33, k0, k1, k2, k3);
  // Assemble results: column-major layout
  // colN = {row0, row1, row2, row3} of column N
  // Output: col0 in [127:96], col1 in [95:64], col2 in [63:32], col3 in [31:0]
  logic [127:0] enc_result;
  assign enc_result = {enc_col0[31:24], enc_col0[23:16], enc_col0[15:8], enc_col0[7:0], enc_col1[31:24], enc_col1[23:16], enc_col1[15:8], enc_col1[7:0], enc_col2[31:24], enc_col2[23:16], enc_col2[15:8], enc_col2[7:0], enc_col3[31:24], enc_col3[23:16], enc_col3[15:8], enc_col3[7:0]};
  logic [127:0] dec_result;
  assign dec_result = {dec_col0[31:24], dec_col0[23:16], dec_col0[15:8], dec_col0[7:0], dec_col1[31:24], dec_col1[23:16], dec_col1[15:8], dec_col1[7:0], dec_col2[31:24], dec_col2[23:16], dec_col2[15:8], dec_col2[7:0], dec_col3[31:24], dec_col3[23:16], dec_col3[15:8], dec_col3[7:0]};
  logic [127:0] computed;
  assign computed = s1_encrypt ? enc_result : dec_result;
  always_ff @(posedge clk or negedge rst_async_n) begin
    if ((!rst_async_n)) begin
      key_reg <= 0;
      o_data <= 0;
      o_valid <= 0;
      s1_data <= 0;
      s1_encrypt <= 0;
      s1_valid <= 0;
      s2_data <= 0;
      s2_valid <= 0;
    end else begin
      // Key update
      if (i_update_key) begin
        key_reg <= i_key;
      end
      // Stage 1: latch inputs
      s1_valid <= i_valid;
      s1_data <= i_data;
      s1_encrypt <= i_encrypt;
      // Stage 2: compute result, only update data when s1 is valid
      s2_valid <= s1_valid;
      if (s1_valid) begin
        s2_data <= computed;
      end
      // Output: pass through s2; clear o_data to 0 when s2 is not valid
      o_valid <= s2_valid;
      if (s2_valid) begin
        o_data <= s2_data;
      end else begin
        o_data <= 0;
      end
    end
  end

endmodule

