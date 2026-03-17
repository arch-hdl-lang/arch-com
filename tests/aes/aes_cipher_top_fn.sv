// AES-128 Encryption — function-based version
// Demonstrates: AesSbox and Xtime as pure combinational functions,
// eliminating 32 inst blocks from AesCipherTop and 4+1 inst blocks from AesKeyExpand128.
// domain SysDomain
//   freq_mhz: 100

// ── Pure combinational functions ──────────────────────────────────────────────
// ── Key Expansion ─────────────────────────────────────────────────────────────
// AesSbox replaces 4 inst sbox blocks; AesRcon is inlined as a let/match.
module AesKeyExpand128 (
  input logic clk,
  input logic kld,
  input logic [128-1:0] key,
  output logic [32-1:0] wo_0,
  output logic [32-1:0] wo_1,
  output logic [32-1:0] wo_2,
  output logic [32-1:0] wo_3
);

  function automatic logic [8-1:0] AesSbox(input logic [8-1:0] a);
    return ((a == 'h0) ? 'h63 : ((a == 'h1) ? 'h7C : ((a == 'h2) ? 'h77 : ((a == 'h3) ? 'h7B : ((a == 'h4) ? 'hF2 : ((a == 'h5) ? 'h6B : ((a == 'h6) ? 'h6F : ((a == 'h7) ? 'hC5 : ((a == 'h8) ? 'h30 : ((a == 'h9) ? 'h1 : ((a == 'hA) ? 'h67 : ((a == 'hB) ? 'h2B : ((a == 'hC) ? 'hFE : ((a == 'hD) ? 'hD7 : ((a == 'hE) ? 'hAB : ((a == 'hF) ? 'h76 : ((a == 'h10) ? 'hCA : ((a == 'h11) ? 'h82 : ((a == 'h12) ? 'hC9 : ((a == 'h13) ? 'h7D : ((a == 'h14) ? 'hFA : ((a == 'h15) ? 'h59 : ((a == 'h16) ? 'h47 : ((a == 'h17) ? 'hF0 : ((a == 'h18) ? 'hAD : ((a == 'h19) ? 'hD4 : ((a == 'h1A) ? 'hA2 : ((a == 'h1B) ? 'hAF : ((a == 'h1C) ? 'h9C : ((a == 'h1D) ? 'hA4 : ((a == 'h1E) ? 'h72 : ((a == 'h1F) ? 'hC0 : ((a == 'h20) ? 'hB7 : ((a == 'h21) ? 'hFD : ((a == 'h22) ? 'h93 : ((a == 'h23) ? 'h26 : ((a == 'h24) ? 'h36 : ((a == 'h25) ? 'h3F : ((a == 'h26) ? 'hF7 : ((a == 'h27) ? 'hCC : ((a == 'h28) ? 'h34 : ((a == 'h29) ? 'hA5 : ((a == 'h2A) ? 'hE5 : ((a == 'h2B) ? 'hF1 : ((a == 'h2C) ? 'h71 : ((a == 'h2D) ? 'hD8 : ((a == 'h2E) ? 'h31 : ((a == 'h2F) ? 'h15 : ((a == 'h30) ? 'h4 : ((a == 'h31) ? 'hC7 : ((a == 'h32) ? 'h23 : ((a == 'h33) ? 'hC3 : ((a == 'h34) ? 'h18 : ((a == 'h35) ? 'h96 : ((a == 'h36) ? 'h5 : ((a == 'h37) ? 'h9A : ((a == 'h38) ? 'h7 : ((a == 'h39) ? 'h12 : ((a == 'h3A) ? 'h80 : ((a == 'h3B) ? 'hE2 : ((a == 'h3C) ? 'hEB : ((a == 'h3D) ? 'h27 : ((a == 'h3E) ? 'hB2 : ((a == 'h3F) ? 'h75 : ((a == 'h40) ? 'h9 : ((a == 'h41) ? 'h83 : ((a == 'h42) ? 'h2C : ((a == 'h43) ? 'h1A : ((a == 'h44) ? 'h1B : ((a == 'h45) ? 'h6E : ((a == 'h46) ? 'h5A : ((a == 'h47) ? 'hA0 : ((a == 'h48) ? 'h52 : ((a == 'h49) ? 'h3B : ((a == 'h4A) ? 'hD6 : ((a == 'h4B) ? 'hB3 : ((a == 'h4C) ? 'h29 : ((a == 'h4D) ? 'hE3 : ((a == 'h4E) ? 'h2F : ((a == 'h4F) ? 'h84 : ((a == 'h50) ? 'h53 : ((a == 'h51) ? 'hD1 : ((a == 'h52) ? 'h0 : ((a == 'h53) ? 'hED : ((a == 'h54) ? 'h20 : ((a == 'h55) ? 'hFC : ((a == 'h56) ? 'hB1 : ((a == 'h57) ? 'h5B : ((a == 'h58) ? 'h6A : ((a == 'h59) ? 'hCB : ((a == 'h5A) ? 'hBE : ((a == 'h5B) ? 'h39 : ((a == 'h5C) ? 'h4A : ((a == 'h5D) ? 'h4C : ((a == 'h5E) ? 'h58 : ((a == 'h5F) ? 'hCF : ((a == 'h60) ? 'hD0 : ((a == 'h61) ? 'hEF : ((a == 'h62) ? 'hAA : ((a == 'h63) ? 'hFB : ((a == 'h64) ? 'h43 : ((a == 'h65) ? 'h4D : ((a == 'h66) ? 'h33 : ((a == 'h67) ? 'h85 : ((a == 'h68) ? 'h45 : ((a == 'h69) ? 'hF9 : ((a == 'h6A) ? 'h2 : ((a == 'h6B) ? 'h7F : ((a == 'h6C) ? 'h50 : ((a == 'h6D) ? 'h3C : ((a == 'h6E) ? 'h9F : ((a == 'h6F) ? 'hA8 : ((a == 'h70) ? 'h51 : ((a == 'h71) ? 'hA3 : ((a == 'h72) ? 'h40 : ((a == 'h73) ? 'h8F : ((a == 'h74) ? 'h92 : ((a == 'h75) ? 'h9D : ((a == 'h76) ? 'h38 : ((a == 'h77) ? 'hF5 : ((a == 'h78) ? 'hBC : ((a == 'h79) ? 'hB6 : ((a == 'h7A) ? 'hDA : ((a == 'h7B) ? 'h21 : ((a == 'h7C) ? 'h10 : ((a == 'h7D) ? 'hFF : ((a == 'h7E) ? 'hF3 : ((a == 'h7F) ? 'hD2 : ((a == 'h80) ? 'hCD : ((a == 'h81) ? 'hC : ((a == 'h82) ? 'h13 : ((a == 'h83) ? 'hEC : ((a == 'h84) ? 'h5F : ((a == 'h85) ? 'h97 : ((a == 'h86) ? 'h44 : ((a == 'h87) ? 'h17 : ((a == 'h88) ? 'hC4 : ((a == 'h89) ? 'hA7 : ((a == 'h8A) ? 'h7E : ((a == 'h8B) ? 'h3D : ((a == 'h8C) ? 'h64 : ((a == 'h8D) ? 'h5D : ((a == 'h8E) ? 'h19 : ((a == 'h8F) ? 'h73 : ((a == 'h90) ? 'h60 : ((a == 'h91) ? 'h81 : ((a == 'h92) ? 'h4F : ((a == 'h93) ? 'hDC : ((a == 'h94) ? 'h22 : ((a == 'h95) ? 'h2A : ((a == 'h96) ? 'h90 : ((a == 'h97) ? 'h88 : ((a == 'h98) ? 'h46 : ((a == 'h99) ? 'hEE : ((a == 'h9A) ? 'hB8 : ((a == 'h9B) ? 'h14 : ((a == 'h9C) ? 'hDE : ((a == 'h9D) ? 'h5E : ((a == 'h9E) ? 'hB : ((a == 'h9F) ? 'hDB : ((a == 'hA0) ? 'hE0 : ((a == 'hA1) ? 'h32 : ((a == 'hA2) ? 'h3A : ((a == 'hA3) ? 'hA : ((a == 'hA4) ? 'h49 : ((a == 'hA5) ? 'h6 : ((a == 'hA6) ? 'h24 : ((a == 'hA7) ? 'h5C : ((a == 'hA8) ? 'hC2 : ((a == 'hA9) ? 'hD3 : ((a == 'hAA) ? 'hAC : ((a == 'hAB) ? 'h62 : ((a == 'hAC) ? 'h91 : ((a == 'hAD) ? 'h95 : ((a == 'hAE) ? 'hE4 : ((a == 'hAF) ? 'h79 : ((a == 'hB0) ? 'hE7 : ((a == 'hB1) ? 'hC8 : ((a == 'hB2) ? 'h37 : ((a == 'hB3) ? 'h6D : ((a == 'hB4) ? 'h8D : ((a == 'hB5) ? 'hD5 : ((a == 'hB6) ? 'h4E : ((a == 'hB7) ? 'hA9 : ((a == 'hB8) ? 'h6C : ((a == 'hB9) ? 'h56 : ((a == 'hBA) ? 'hF4 : ((a == 'hBB) ? 'hEA : ((a == 'hBC) ? 'h65 : ((a == 'hBD) ? 'h7A : ((a == 'hBE) ? 'hAE : ((a == 'hBF) ? 'h8 : ((a == 'hC0) ? 'hBA : ((a == 'hC1) ? 'h78 : ((a == 'hC2) ? 'h25 : ((a == 'hC3) ? 'h2E : ((a == 'hC4) ? 'h1C : ((a == 'hC5) ? 'hA6 : ((a == 'hC6) ? 'hB4 : ((a == 'hC7) ? 'hC6 : ((a == 'hC8) ? 'hE8 : ((a == 'hC9) ? 'hDD : ((a == 'hCA) ? 'h74 : ((a == 'hCB) ? 'h1F : ((a == 'hCC) ? 'h4B : ((a == 'hCD) ? 'hBD : ((a == 'hCE) ? 'h8B : ((a == 'hCF) ? 'h8A : ((a == 'hD0) ? 'h70 : ((a == 'hD1) ? 'h3E : ((a == 'hD2) ? 'hB5 : ((a == 'hD3) ? 'h66 : ((a == 'hD4) ? 'h48 : ((a == 'hD5) ? 'h3 : ((a == 'hD6) ? 'hF6 : ((a == 'hD7) ? 'hE : ((a == 'hD8) ? 'h61 : ((a == 'hD9) ? 'h35 : ((a == 'hDA) ? 'h57 : ((a == 'hDB) ? 'hB9 : ((a == 'hDC) ? 'h86 : ((a == 'hDD) ? 'hC1 : ((a == 'hDE) ? 'h1D : ((a == 'hDF) ? 'h9E : ((a == 'hE0) ? 'hE1 : ((a == 'hE1) ? 'hF8 : ((a == 'hE2) ? 'h98 : ((a == 'hE3) ? 'h11 : ((a == 'hE4) ? 'h69 : ((a == 'hE5) ? 'hD9 : ((a == 'hE6) ? 'h8E : ((a == 'hE7) ? 'h94 : ((a == 'hE8) ? 'h9B : ((a == 'hE9) ? 'h1E : ((a == 'hEA) ? 'h87 : ((a == 'hEB) ? 'hE9 : ((a == 'hEC) ? 'hCE : ((a == 'hED) ? 'h55 : ((a == 'hEE) ? 'h28 : ((a == 'hEF) ? 'hDF : ((a == 'hF0) ? 'h8C : ((a == 'hF1) ? 'hA1 : ((a == 'hF2) ? 'h89 : ((a == 'hF3) ? 'hD : ((a == 'hF4) ? 'hBF : ((a == 'hF5) ? 'hE6 : ((a == 'hF6) ? 'h42 : ((a == 'hF7) ? 'h68 : ((a == 'hF8) ? 'h41 : ((a == 'hF9) ? 'h99 : ((a == 'hFA) ? 'h2D : ((a == 'hFB) ? 'hF : ((a == 'hFC) ? 'hB0 : ((a == 'hFD) ? 'h54 : ((a == 'hFE) ? 'hBB : ((a == 'hFF) ? 'h16 : 'h0))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
  endfunction
  
  function automatic logic [8-1:0] Xtime(input logic [8-1:0] a);
    logic [8-1:0] shifted = 8'((a << 1));
    return (((a & 'h80) == 'h80) ? (shifted ^ 'h1B) : shifted);
  endfunction
  
  logic [32-1:0] w0 = 0;
  logic [32-1:0] w1 = 0;
  logic [32-1:0] w2 = 0;
  logic [32-1:0] w3 = 0;
  logic [4-1:0] rcnt = 0;
  // SubWord: AesSbox on each byte of w3 (RotWord then SubBytes)
  logic [32-1:0] subword;
  assign subword = {AesSbox(w3[23:16]), AesSbox(w3[15:8]), AesSbox(w3[7:0]), AesSbox(w3[31:24])};
  // Round constant (was: inst rcon0: AesRcon)
  logic [32-1:0] rcon_val;
  assign rcon_val = ((rcnt == 'h0) ? 'h1000000 : ((rcnt == 'h1) ? 'h2000000 : ((rcnt == 'h2) ? 'h4000000 : ((rcnt == 'h3) ? 'h8000000 : ((rcnt == 'h4) ? 'h10000000 : ((rcnt == 'h5) ? 'h20000000 : ((rcnt == 'h6) ? 'h40000000 : ((rcnt == 'h7) ? 'h80000000 : ((rcnt == 'h8) ? 'h1B000000 : ((rcnt == 'h9) ? 'h36000000 : 'h0))))))))));
  logic [32-1:0] t;
  assign t = (subword ^ rcon_val);
  logic [32-1:0] nw0;
  assign nw0 = (w0 ^ t);
  logic [32-1:0] nw1;
  assign nw1 = ((w1 ^ w0) ^ t);
  logic [32-1:0] nw2;
  assign nw2 = (((w2 ^ w1) ^ w0) ^ t);
  logic [32-1:0] nw3;
  assign nw3 = ((((w3 ^ w2) ^ w1) ^ w0) ^ t);
  always_ff @(posedge clk) begin
    if (kld) begin
      w0 <= key[127:96];
      w1 <= key[95:64];
      w2 <= key[63:32];
      w3 <= key[31:0];
      rcnt <= 0;
    end else begin
      w0 <= nw0;
      w1 <= nw1;
      w2 <= nw2;
      w3 <= nw3;
      rcnt <= 4'((rcnt + 1));
    end
  end
  assign wo_0 = w0;
  assign wo_1 = w1;
  assign wo_2 = w2;
  assign wo_3 = w3;

endmodule

// ── AES-128 Cipher Top ────────────────────────────────────────────────────────
// Replaces 16 AesSbox inst blocks + 16 Xtime inst blocks with inline calls.
// SubBytes + ShiftRows combined in the sa??_sr lets.
// MixColumns calls Xtime() inline.
module AesCipherTop (
  input logic clk,
  input logic rst,
  input logic ld,
  output logic done,
  input logic [128-1:0] key,
  input logic [128-1:0] text_in,
  output logic [128-1:0] text_out
);

  function automatic logic [8-1:0] AesSbox(input logic [8-1:0] a);
    return ((a == 'h0) ? 'h63 : ((a == 'h1) ? 'h7C : ((a == 'h2) ? 'h77 : ((a == 'h3) ? 'h7B : ((a == 'h4) ? 'hF2 : ((a == 'h5) ? 'h6B : ((a == 'h6) ? 'h6F : ((a == 'h7) ? 'hC5 : ((a == 'h8) ? 'h30 : ((a == 'h9) ? 'h1 : ((a == 'hA) ? 'h67 : ((a == 'hB) ? 'h2B : ((a == 'hC) ? 'hFE : ((a == 'hD) ? 'hD7 : ((a == 'hE) ? 'hAB : ((a == 'hF) ? 'h76 : ((a == 'h10) ? 'hCA : ((a == 'h11) ? 'h82 : ((a == 'h12) ? 'hC9 : ((a == 'h13) ? 'h7D : ((a == 'h14) ? 'hFA : ((a == 'h15) ? 'h59 : ((a == 'h16) ? 'h47 : ((a == 'h17) ? 'hF0 : ((a == 'h18) ? 'hAD : ((a == 'h19) ? 'hD4 : ((a == 'h1A) ? 'hA2 : ((a == 'h1B) ? 'hAF : ((a == 'h1C) ? 'h9C : ((a == 'h1D) ? 'hA4 : ((a == 'h1E) ? 'h72 : ((a == 'h1F) ? 'hC0 : ((a == 'h20) ? 'hB7 : ((a == 'h21) ? 'hFD : ((a == 'h22) ? 'h93 : ((a == 'h23) ? 'h26 : ((a == 'h24) ? 'h36 : ((a == 'h25) ? 'h3F : ((a == 'h26) ? 'hF7 : ((a == 'h27) ? 'hCC : ((a == 'h28) ? 'h34 : ((a == 'h29) ? 'hA5 : ((a == 'h2A) ? 'hE5 : ((a == 'h2B) ? 'hF1 : ((a == 'h2C) ? 'h71 : ((a == 'h2D) ? 'hD8 : ((a == 'h2E) ? 'h31 : ((a == 'h2F) ? 'h15 : ((a == 'h30) ? 'h4 : ((a == 'h31) ? 'hC7 : ((a == 'h32) ? 'h23 : ((a == 'h33) ? 'hC3 : ((a == 'h34) ? 'h18 : ((a == 'h35) ? 'h96 : ((a == 'h36) ? 'h5 : ((a == 'h37) ? 'h9A : ((a == 'h38) ? 'h7 : ((a == 'h39) ? 'h12 : ((a == 'h3A) ? 'h80 : ((a == 'h3B) ? 'hE2 : ((a == 'h3C) ? 'hEB : ((a == 'h3D) ? 'h27 : ((a == 'h3E) ? 'hB2 : ((a == 'h3F) ? 'h75 : ((a == 'h40) ? 'h9 : ((a == 'h41) ? 'h83 : ((a == 'h42) ? 'h2C : ((a == 'h43) ? 'h1A : ((a == 'h44) ? 'h1B : ((a == 'h45) ? 'h6E : ((a == 'h46) ? 'h5A : ((a == 'h47) ? 'hA0 : ((a == 'h48) ? 'h52 : ((a == 'h49) ? 'h3B : ((a == 'h4A) ? 'hD6 : ((a == 'h4B) ? 'hB3 : ((a == 'h4C) ? 'h29 : ((a == 'h4D) ? 'hE3 : ((a == 'h4E) ? 'h2F : ((a == 'h4F) ? 'h84 : ((a == 'h50) ? 'h53 : ((a == 'h51) ? 'hD1 : ((a == 'h52) ? 'h0 : ((a == 'h53) ? 'hED : ((a == 'h54) ? 'h20 : ((a == 'h55) ? 'hFC : ((a == 'h56) ? 'hB1 : ((a == 'h57) ? 'h5B : ((a == 'h58) ? 'h6A : ((a == 'h59) ? 'hCB : ((a == 'h5A) ? 'hBE : ((a == 'h5B) ? 'h39 : ((a == 'h5C) ? 'h4A : ((a == 'h5D) ? 'h4C : ((a == 'h5E) ? 'h58 : ((a == 'h5F) ? 'hCF : ((a == 'h60) ? 'hD0 : ((a == 'h61) ? 'hEF : ((a == 'h62) ? 'hAA : ((a == 'h63) ? 'hFB : ((a == 'h64) ? 'h43 : ((a == 'h65) ? 'h4D : ((a == 'h66) ? 'h33 : ((a == 'h67) ? 'h85 : ((a == 'h68) ? 'h45 : ((a == 'h69) ? 'hF9 : ((a == 'h6A) ? 'h2 : ((a == 'h6B) ? 'h7F : ((a == 'h6C) ? 'h50 : ((a == 'h6D) ? 'h3C : ((a == 'h6E) ? 'h9F : ((a == 'h6F) ? 'hA8 : ((a == 'h70) ? 'h51 : ((a == 'h71) ? 'hA3 : ((a == 'h72) ? 'h40 : ((a == 'h73) ? 'h8F : ((a == 'h74) ? 'h92 : ((a == 'h75) ? 'h9D : ((a == 'h76) ? 'h38 : ((a == 'h77) ? 'hF5 : ((a == 'h78) ? 'hBC : ((a == 'h79) ? 'hB6 : ((a == 'h7A) ? 'hDA : ((a == 'h7B) ? 'h21 : ((a == 'h7C) ? 'h10 : ((a == 'h7D) ? 'hFF : ((a == 'h7E) ? 'hF3 : ((a == 'h7F) ? 'hD2 : ((a == 'h80) ? 'hCD : ((a == 'h81) ? 'hC : ((a == 'h82) ? 'h13 : ((a == 'h83) ? 'hEC : ((a == 'h84) ? 'h5F : ((a == 'h85) ? 'h97 : ((a == 'h86) ? 'h44 : ((a == 'h87) ? 'h17 : ((a == 'h88) ? 'hC4 : ((a == 'h89) ? 'hA7 : ((a == 'h8A) ? 'h7E : ((a == 'h8B) ? 'h3D : ((a == 'h8C) ? 'h64 : ((a == 'h8D) ? 'h5D : ((a == 'h8E) ? 'h19 : ((a == 'h8F) ? 'h73 : ((a == 'h90) ? 'h60 : ((a == 'h91) ? 'h81 : ((a == 'h92) ? 'h4F : ((a == 'h93) ? 'hDC : ((a == 'h94) ? 'h22 : ((a == 'h95) ? 'h2A : ((a == 'h96) ? 'h90 : ((a == 'h97) ? 'h88 : ((a == 'h98) ? 'h46 : ((a == 'h99) ? 'hEE : ((a == 'h9A) ? 'hB8 : ((a == 'h9B) ? 'h14 : ((a == 'h9C) ? 'hDE : ((a == 'h9D) ? 'h5E : ((a == 'h9E) ? 'hB : ((a == 'h9F) ? 'hDB : ((a == 'hA0) ? 'hE0 : ((a == 'hA1) ? 'h32 : ((a == 'hA2) ? 'h3A : ((a == 'hA3) ? 'hA : ((a == 'hA4) ? 'h49 : ((a == 'hA5) ? 'h6 : ((a == 'hA6) ? 'h24 : ((a == 'hA7) ? 'h5C : ((a == 'hA8) ? 'hC2 : ((a == 'hA9) ? 'hD3 : ((a == 'hAA) ? 'hAC : ((a == 'hAB) ? 'h62 : ((a == 'hAC) ? 'h91 : ((a == 'hAD) ? 'h95 : ((a == 'hAE) ? 'hE4 : ((a == 'hAF) ? 'h79 : ((a == 'hB0) ? 'hE7 : ((a == 'hB1) ? 'hC8 : ((a == 'hB2) ? 'h37 : ((a == 'hB3) ? 'h6D : ((a == 'hB4) ? 'h8D : ((a == 'hB5) ? 'hD5 : ((a == 'hB6) ? 'h4E : ((a == 'hB7) ? 'hA9 : ((a == 'hB8) ? 'h6C : ((a == 'hB9) ? 'h56 : ((a == 'hBA) ? 'hF4 : ((a == 'hBB) ? 'hEA : ((a == 'hBC) ? 'h65 : ((a == 'hBD) ? 'h7A : ((a == 'hBE) ? 'hAE : ((a == 'hBF) ? 'h8 : ((a == 'hC0) ? 'hBA : ((a == 'hC1) ? 'h78 : ((a == 'hC2) ? 'h25 : ((a == 'hC3) ? 'h2E : ((a == 'hC4) ? 'h1C : ((a == 'hC5) ? 'hA6 : ((a == 'hC6) ? 'hB4 : ((a == 'hC7) ? 'hC6 : ((a == 'hC8) ? 'hE8 : ((a == 'hC9) ? 'hDD : ((a == 'hCA) ? 'h74 : ((a == 'hCB) ? 'h1F : ((a == 'hCC) ? 'h4B : ((a == 'hCD) ? 'hBD : ((a == 'hCE) ? 'h8B : ((a == 'hCF) ? 'h8A : ((a == 'hD0) ? 'h70 : ((a == 'hD1) ? 'h3E : ((a == 'hD2) ? 'hB5 : ((a == 'hD3) ? 'h66 : ((a == 'hD4) ? 'h48 : ((a == 'hD5) ? 'h3 : ((a == 'hD6) ? 'hF6 : ((a == 'hD7) ? 'hE : ((a == 'hD8) ? 'h61 : ((a == 'hD9) ? 'h35 : ((a == 'hDA) ? 'h57 : ((a == 'hDB) ? 'hB9 : ((a == 'hDC) ? 'h86 : ((a == 'hDD) ? 'hC1 : ((a == 'hDE) ? 'h1D : ((a == 'hDF) ? 'h9E : ((a == 'hE0) ? 'hE1 : ((a == 'hE1) ? 'hF8 : ((a == 'hE2) ? 'h98 : ((a == 'hE3) ? 'h11 : ((a == 'hE4) ? 'h69 : ((a == 'hE5) ? 'hD9 : ((a == 'hE6) ? 'h8E : ((a == 'hE7) ? 'h94 : ((a == 'hE8) ? 'h9B : ((a == 'hE9) ? 'h1E : ((a == 'hEA) ? 'h87 : ((a == 'hEB) ? 'hE9 : ((a == 'hEC) ? 'hCE : ((a == 'hED) ? 'h55 : ((a == 'hEE) ? 'h28 : ((a == 'hEF) ? 'hDF : ((a == 'hF0) ? 'h8C : ((a == 'hF1) ? 'hA1 : ((a == 'hF2) ? 'h89 : ((a == 'hF3) ? 'hD : ((a == 'hF4) ? 'hBF : ((a == 'hF5) ? 'hE6 : ((a == 'hF6) ? 'h42 : ((a == 'hF7) ? 'h68 : ((a == 'hF8) ? 'h41 : ((a == 'hF9) ? 'h99 : ((a == 'hFA) ? 'h2D : ((a == 'hFB) ? 'hF : ((a == 'hFC) ? 'hB0 : ((a == 'hFD) ? 'h54 : ((a == 'hFE) ? 'hBB : ((a == 'hFF) ? 'h16 : 'h0))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
  endfunction
  
  function automatic logic [8-1:0] Xtime(input logic [8-1:0] a);
    logic [8-1:0] shifted = 8'((a << 1));
    return (((a & 'h80) == 'h80) ? (shifted ^ 'h1B) : shifted);
  endfunction
  
  // ── Control registers ──
  logic [4-1:0] dcnt = 0;
  logic ld_r = 1'b0;
  logic done_r = 1'b0;
  // ── Input buffer ──
  logic [128-1:0] text_in_r = 0;
  // ── State matrix: 16 individual 8-bit registers ──
  logic [8-1:0] sa00 = 0;
  logic [8-1:0] sa01 = 0;
  logic [8-1:0] sa02 = 0;
  logic [8-1:0] sa03 = 0;
  logic [8-1:0] sa10 = 0;
  logic [8-1:0] sa11 = 0;
  logic [8-1:0] sa12 = 0;
  logic [8-1:0] sa13 = 0;
  logic [8-1:0] sa20 = 0;
  logic [8-1:0] sa21 = 0;
  logic [8-1:0] sa22 = 0;
  logic [8-1:0] sa23 = 0;
  logic [8-1:0] sa30 = 0;
  logic [8-1:0] sa31 = 0;
  logic [8-1:0] sa32 = 0;
  logic [8-1:0] sa33 = 0;
  // ── Output register ──
  logic [128-1:0] text_out_r = 0;
  // ── Key expansion instance ──
  logic [32-1:0] w0;
  logic [32-1:0] w1;
  logic [32-1:0] w2;
  logic [32-1:0] w3;
  AesKeyExpand128 key_exp (
    .clk(clk),
    .kld(ld),
    .key(key),
    .wo_0(w0),
    .wo_1(w1),
    .wo_2(w2),
    .wo_3(w3)
  );
  // ── SubBytes + ShiftRows (was: 16 AesSbox inst + 16 let) ──
  // Row 0: no shift
  logic [8-1:0] sa00_sr;
  assign sa00_sr = AesSbox(sa00);
  logic [8-1:0] sa01_sr;
  assign sa01_sr = AesSbox(sa01);
  logic [8-1:0] sa02_sr;
  assign sa02_sr = AesSbox(sa02);
  logic [8-1:0] sa03_sr;
  assign sa03_sr = AesSbox(sa03);
  // Row 1: shift left 1
  logic [8-1:0] sa10_sr;
  assign sa10_sr = AesSbox(sa11);
  logic [8-1:0] sa11_sr;
  assign sa11_sr = AesSbox(sa12);
  logic [8-1:0] sa12_sr;
  assign sa12_sr = AesSbox(sa13);
  logic [8-1:0] sa13_sr;
  assign sa13_sr = AesSbox(sa10);
  // Row 2: shift left 2
  logic [8-1:0] sa20_sr;
  assign sa20_sr = AesSbox(sa22);
  logic [8-1:0] sa21_sr;
  assign sa21_sr = AesSbox(sa23);
  logic [8-1:0] sa22_sr;
  assign sa22_sr = AesSbox(sa20);
  logic [8-1:0] sa23_sr;
  assign sa23_sr = AesSbox(sa21);
  // Row 3: shift left 3
  logic [8-1:0] sa30_sr;
  assign sa30_sr = AesSbox(sa33);
  logic [8-1:0] sa31_sr;
  assign sa31_sr = AesSbox(sa30);
  logic [8-1:0] sa32_sr;
  assign sa32_sr = AesSbox(sa31);
  logic [8-1:0] sa33_sr;
  assign sa33_sr = AesSbox(sa32);
  // ── MixColumns (Xtime called inline — was: 16 Xtime inst) ──
  logic [8-1:0] sa00_mc;
  assign sa00_mc = ((((Xtime(sa00_sr) ^ Xtime(sa10_sr)) ^ sa10_sr) ^ sa20_sr) ^ sa30_sr);
  logic [8-1:0] sa10_mc;
  assign sa10_mc = ((((sa00_sr ^ Xtime(sa10_sr)) ^ Xtime(sa20_sr)) ^ sa20_sr) ^ sa30_sr);
  logic [8-1:0] sa20_mc;
  assign sa20_mc = ((((sa00_sr ^ sa10_sr) ^ Xtime(sa20_sr)) ^ Xtime(sa30_sr)) ^ sa30_sr);
  logic [8-1:0] sa30_mc;
  assign sa30_mc = ((((Xtime(sa00_sr) ^ sa00_sr) ^ sa10_sr) ^ sa20_sr) ^ Xtime(sa30_sr));
  logic [8-1:0] sa01_mc;
  assign sa01_mc = ((((Xtime(sa01_sr) ^ Xtime(sa11_sr)) ^ sa11_sr) ^ sa21_sr) ^ sa31_sr);
  logic [8-1:0] sa11_mc;
  assign sa11_mc = ((((sa01_sr ^ Xtime(sa11_sr)) ^ Xtime(sa21_sr)) ^ sa21_sr) ^ sa31_sr);
  logic [8-1:0] sa21_mc;
  assign sa21_mc = ((((sa01_sr ^ sa11_sr) ^ Xtime(sa21_sr)) ^ Xtime(sa31_sr)) ^ sa31_sr);
  logic [8-1:0] sa31_mc;
  assign sa31_mc = ((((Xtime(sa01_sr) ^ sa01_sr) ^ sa11_sr) ^ sa21_sr) ^ Xtime(sa31_sr));
  logic [8-1:0] sa02_mc;
  assign sa02_mc = ((((Xtime(sa02_sr) ^ Xtime(sa12_sr)) ^ sa12_sr) ^ sa22_sr) ^ sa32_sr);
  logic [8-1:0] sa12_mc;
  assign sa12_mc = ((((sa02_sr ^ Xtime(sa12_sr)) ^ Xtime(sa22_sr)) ^ sa22_sr) ^ sa32_sr);
  logic [8-1:0] sa22_mc;
  assign sa22_mc = ((((sa02_sr ^ sa12_sr) ^ Xtime(sa22_sr)) ^ Xtime(sa32_sr)) ^ sa32_sr);
  logic [8-1:0] sa32_mc;
  assign sa32_mc = ((((Xtime(sa02_sr) ^ sa02_sr) ^ sa12_sr) ^ sa22_sr) ^ Xtime(sa32_sr));
  logic [8-1:0] sa03_mc;
  assign sa03_mc = ((((Xtime(sa03_sr) ^ Xtime(sa13_sr)) ^ sa13_sr) ^ sa23_sr) ^ sa33_sr);
  logic [8-1:0] sa13_mc;
  assign sa13_mc = ((((sa03_sr ^ Xtime(sa13_sr)) ^ Xtime(sa23_sr)) ^ sa23_sr) ^ sa33_sr);
  logic [8-1:0] sa23_mc;
  assign sa23_mc = ((((sa03_sr ^ sa13_sr) ^ Xtime(sa23_sr)) ^ Xtime(sa33_sr)) ^ sa33_sr);
  logic [8-1:0] sa33_mc;
  assign sa33_mc = ((((Xtime(sa03_sr) ^ sa03_sr) ^ sa13_sr) ^ sa23_sr) ^ Xtime(sa33_sr));
  // ── AddRoundKey after MixColumns (rounds 1-9) ──
  logic [8-1:0] sa00_ark;
  assign sa00_ark = (sa00_mc ^ w0[31:24]);
  logic [8-1:0] sa10_ark;
  assign sa10_ark = (sa10_mc ^ w0[23:16]);
  logic [8-1:0] sa20_ark;
  assign sa20_ark = (sa20_mc ^ w0[15:8]);
  logic [8-1:0] sa30_ark;
  assign sa30_ark = (sa30_mc ^ w0[7:0]);
  logic [8-1:0] sa01_ark;
  assign sa01_ark = (sa01_mc ^ w1[31:24]);
  logic [8-1:0] sa11_ark;
  assign sa11_ark = (sa11_mc ^ w1[23:16]);
  logic [8-1:0] sa21_ark;
  assign sa21_ark = (sa21_mc ^ w1[15:8]);
  logic [8-1:0] sa31_ark;
  assign sa31_ark = (sa31_mc ^ w1[7:0]);
  logic [8-1:0] sa02_ark;
  assign sa02_ark = (sa02_mc ^ w2[31:24]);
  logic [8-1:0] sa12_ark;
  assign sa12_ark = (sa12_mc ^ w2[23:16]);
  logic [8-1:0] sa22_ark;
  assign sa22_ark = (sa22_mc ^ w2[15:8]);
  logic [8-1:0] sa32_ark;
  assign sa32_ark = (sa32_mc ^ w2[7:0]);
  logic [8-1:0] sa03_ark;
  assign sa03_ark = (sa03_mc ^ w3[31:24]);
  logic [8-1:0] sa13_ark;
  assign sa13_ark = (sa13_mc ^ w3[23:16]);
  logic [8-1:0] sa23_ark;
  assign sa23_ark = (sa23_mc ^ w3[15:8]);
  logic [8-1:0] sa33_ark;
  assign sa33_ark = (sa33_mc ^ w3[7:0]);
  // ── AddRoundKey after ShiftRows only (final round, no MixColumns) ──
  logic [8-1:0] sa00_fark;
  assign sa00_fark = (sa00_sr ^ w0[31:24]);
  logic [8-1:0] sa10_fark;
  assign sa10_fark = (sa10_sr ^ w0[23:16]);
  logic [8-1:0] sa20_fark;
  assign sa20_fark = (sa20_sr ^ w0[15:8]);
  logic [8-1:0] sa30_fark;
  assign sa30_fark = (sa30_sr ^ w0[7:0]);
  logic [8-1:0] sa01_fark;
  assign sa01_fark = (sa01_sr ^ w1[31:24]);
  logic [8-1:0] sa11_fark;
  assign sa11_fark = (sa11_sr ^ w1[23:16]);
  logic [8-1:0] sa21_fark;
  assign sa21_fark = (sa21_sr ^ w1[15:8]);
  logic [8-1:0] sa31_fark;
  assign sa31_fark = (sa31_sr ^ w1[7:0]);
  logic [8-1:0] sa02_fark;
  assign sa02_fark = (sa02_sr ^ w2[31:24]);
  logic [8-1:0] sa12_fark;
  assign sa12_fark = (sa12_sr ^ w2[23:16]);
  logic [8-1:0] sa22_fark;
  assign sa22_fark = (sa22_sr ^ w2[15:8]);
  logic [8-1:0] sa32_fark;
  assign sa32_fark = (sa32_sr ^ w2[7:0]);
  logic [8-1:0] sa03_fark;
  assign sa03_fark = (sa03_sr ^ w3[31:24]);
  logic [8-1:0] sa13_fark;
  assign sa13_fark = (sa13_sr ^ w3[23:16]);
  logic [8-1:0] sa23_fark;
  assign sa23_fark = (sa23_sr ^ w3[15:8]);
  logic [8-1:0] sa33_fark;
  assign sa33_fark = (sa33_sr ^ w3[7:0]);
  // ── FSM control + state matrix updates ──
  always_ff @(posedge clk) begin
    if (rst) begin
      dcnt <= 0;
      done_r <= 1'b0;
      ld_r <= 1'b0;
      sa00 <= 0;
      sa01 <= 0;
      sa02 <= 0;
      sa03 <= 0;
      sa10 <= 0;
      sa11 <= 0;
      sa12 <= 0;
      sa13 <= 0;
      sa20 <= 0;
      sa21 <= 0;
      sa22 <= 0;
      sa23 <= 0;
      sa30 <= 0;
      sa31 <= 0;
      sa32 <= 0;
      sa33 <= 0;
      text_in_r <= 0;
      text_out_r <= 0;
    end else begin
      ld_r <= ld;
      if (ld) begin
        dcnt <= 'hB;
      end else if ((dcnt != 0)) begin
        dcnt <= 4'((dcnt - 1));
      end
      if (ld_r) begin
        sa00 <= (text_in_r[127:120] ^ w0[31:24]);
        sa10 <= (text_in_r[119:112] ^ w0[23:16]);
        sa20 <= (text_in_r[111:104] ^ w0[15:8]);
        sa30 <= (text_in_r[103:96] ^ w0[7:0]);
        sa01 <= (text_in_r[95:88] ^ w1[31:24]);
        sa11 <= (text_in_r[87:80] ^ w1[23:16]);
        sa21 <= (text_in_r[79:72] ^ w1[15:8]);
        sa31 <= (text_in_r[71:64] ^ w1[7:0]);
        sa02 <= (text_in_r[63:56] ^ w2[31:24]);
        sa12 <= (text_in_r[55:48] ^ w2[23:16]);
        sa22 <= (text_in_r[47:40] ^ w2[15:8]);
        sa32 <= (text_in_r[39:32] ^ w2[7:0]);
        sa03 <= (text_in_r[31:24] ^ w3[31:24]);
        sa13 <= (text_in_r[23:16] ^ w3[23:16]);
        sa23 <= (text_in_r[15:8] ^ w3[15:8]);
        sa33 <= (text_in_r[7:0] ^ w3[7:0]);
      end else if ((dcnt > 1)) begin
        sa00 <= sa00_ark;
        sa01 <= sa01_ark;
        sa02 <= sa02_ark;
        sa03 <= sa03_ark;
        sa10 <= sa10_ark;
        sa11 <= sa11_ark;
        sa12 <= sa12_ark;
        sa13 <= sa13_ark;
        sa20 <= sa20_ark;
        sa21 <= sa21_ark;
        sa22 <= sa22_ark;
        sa23 <= sa23_ark;
        sa30 <= sa30_ark;
        sa31 <= sa31_ark;
        sa32 <= sa32_ark;
        sa33 <= sa33_ark;
      end else if ((dcnt == 1)) begin
        sa00 <= sa00_fark;
        sa01 <= sa01_fark;
        sa02 <= sa02_fark;
        sa03 <= sa03_fark;
        sa10 <= sa10_fark;
        sa11 <= sa11_fark;
        sa12 <= sa12_fark;
        sa13 <= sa13_fark;
        sa20 <= sa20_fark;
        sa21 <= sa21_fark;
        sa22 <= sa22_fark;
        sa23 <= sa23_fark;
        sa30 <= sa30_fark;
        sa31 <= sa31_fark;
        sa32 <= sa32_fark;
        sa33 <= sa33_fark;
      end
      if (ld) begin
        text_in_r <= text_in;
      end
      if (ld) begin
        done_r <= 1'b0;
      end else if ((dcnt == 1)) begin
        done_r <= 1'b1;
      end
      if ((dcnt == 1)) begin
        text_out_r <= {sa00_fark, sa10_fark, sa20_fark, sa30_fark, sa01_fark, sa11_fark, sa21_fark, sa31_fark, sa02_fark, sa12_fark, sa22_fark, sa32_fark, sa03_fark, sa13_fark, sa23_fark, sa33_fark};
      end
    end
  end
  assign done = done_r;
  assign text_out = text_out_r;

endmodule

