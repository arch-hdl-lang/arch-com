// domain SysDomain
//   freq_mhz: 100

// AES Forward S-Box: 256x8 lookup table (pure combinational)
// Implements the SubBytes transformation for AES encryption.
module AesSbox (
  input logic [8-1:0] a,
  output logic [8-1:0] b
);

  always_comb begin
    case (a)
      'h0: b = 'h63;
      'h1: b = 'h7C;
      'h2: b = 'h77;
      'h3: b = 'h7B;
      'h4: b = 'hF2;
      'h5: b = 'h6B;
      'h6: b = 'h6F;
      'h7: b = 'hC5;
      'h8: b = 'h30;
      'h9: b = 'h1;
      'hA: b = 'h67;
      'hB: b = 'h2B;
      'hC: b = 'hFE;
      'hD: b = 'hD7;
      'hE: b = 'hAB;
      'hF: b = 'h76;
      'h10: b = 'hCA;
      'h11: b = 'h82;
      'h12: b = 'hC9;
      'h13: b = 'h7D;
      'h14: b = 'hFA;
      'h15: b = 'h59;
      'h16: b = 'h47;
      'h17: b = 'hF0;
      'h18: b = 'hAD;
      'h19: b = 'hD4;
      'h1A: b = 'hA2;
      'h1B: b = 'hAF;
      'h1C: b = 'h9C;
      'h1D: b = 'hA4;
      'h1E: b = 'h72;
      'h1F: b = 'hC0;
      'h20: b = 'hB7;
      'h21: b = 'hFD;
      'h22: b = 'h93;
      'h23: b = 'h26;
      'h24: b = 'h36;
      'h25: b = 'h3F;
      'h26: b = 'hF7;
      'h27: b = 'hCC;
      'h28: b = 'h34;
      'h29: b = 'hA5;
      'h2A: b = 'hE5;
      'h2B: b = 'hF1;
      'h2C: b = 'h71;
      'h2D: b = 'hD8;
      'h2E: b = 'h31;
      'h2F: b = 'h15;
      'h30: b = 'h4;
      'h31: b = 'hC7;
      'h32: b = 'h23;
      'h33: b = 'hC3;
      'h34: b = 'h18;
      'h35: b = 'h96;
      'h36: b = 'h5;
      'h37: b = 'h9A;
      'h38: b = 'h7;
      'h39: b = 'h12;
      'h3A: b = 'h80;
      'h3B: b = 'hE2;
      'h3C: b = 'hEB;
      'h3D: b = 'h27;
      'h3E: b = 'hB2;
      'h3F: b = 'h75;
      'h40: b = 'h9;
      'h41: b = 'h83;
      'h42: b = 'h2C;
      'h43: b = 'h1A;
      'h44: b = 'h1B;
      'h45: b = 'h6E;
      'h46: b = 'h5A;
      'h47: b = 'hA0;
      'h48: b = 'h52;
      'h49: b = 'h3B;
      'h4A: b = 'hD6;
      'h4B: b = 'hB3;
      'h4C: b = 'h29;
      'h4D: b = 'hE3;
      'h4E: b = 'h2F;
      'h4F: b = 'h84;
      'h50: b = 'h53;
      'h51: b = 'hD1;
      'h52: b = 'h0;
      'h53: b = 'hED;
      'h54: b = 'h20;
      'h55: b = 'hFC;
      'h56: b = 'hB1;
      'h57: b = 'h5B;
      'h58: b = 'h6A;
      'h59: b = 'hCB;
      'h5A: b = 'hBE;
      'h5B: b = 'h39;
      'h5C: b = 'h4A;
      'h5D: b = 'h4C;
      'h5E: b = 'h58;
      'h5F: b = 'hCF;
      'h60: b = 'hD0;
      'h61: b = 'hEF;
      'h62: b = 'hAA;
      'h63: b = 'hFB;
      'h64: b = 'h43;
      'h65: b = 'h4D;
      'h66: b = 'h33;
      'h67: b = 'h85;
      'h68: b = 'h45;
      'h69: b = 'hF9;
      'h6A: b = 'h2;
      'h6B: b = 'h7F;
      'h6C: b = 'h50;
      'h6D: b = 'h3C;
      'h6E: b = 'h9F;
      'h6F: b = 'hA8;
      'h70: b = 'h51;
      'h71: b = 'hA3;
      'h72: b = 'h40;
      'h73: b = 'h8F;
      'h74: b = 'h92;
      'h75: b = 'h9D;
      'h76: b = 'h38;
      'h77: b = 'hF5;
      'h78: b = 'hBC;
      'h79: b = 'hB6;
      'h7A: b = 'hDA;
      'h7B: b = 'h21;
      'h7C: b = 'h10;
      'h7D: b = 'hFF;
      'h7E: b = 'hF3;
      'h7F: b = 'hD2;
      'h80: b = 'hCD;
      'h81: b = 'hC;
      'h82: b = 'h13;
      'h83: b = 'hEC;
      'h84: b = 'h5F;
      'h85: b = 'h97;
      'h86: b = 'h44;
      'h87: b = 'h17;
      'h88: b = 'hC4;
      'h89: b = 'hA7;
      'h8A: b = 'h7E;
      'h8B: b = 'h3D;
      'h8C: b = 'h64;
      'h8D: b = 'h5D;
      'h8E: b = 'h19;
      'h8F: b = 'h73;
      'h90: b = 'h60;
      'h91: b = 'h81;
      'h92: b = 'h4F;
      'h93: b = 'hDC;
      'h94: b = 'h22;
      'h95: b = 'h2A;
      'h96: b = 'h90;
      'h97: b = 'h88;
      'h98: b = 'h46;
      'h99: b = 'hEE;
      'h9A: b = 'hB8;
      'h9B: b = 'h14;
      'h9C: b = 'hDE;
      'h9D: b = 'h5E;
      'h9E: b = 'hB;
      'h9F: b = 'hDB;
      'hA0: b = 'hE0;
      'hA1: b = 'h32;
      'hA2: b = 'h3A;
      'hA3: b = 'hA;
      'hA4: b = 'h49;
      'hA5: b = 'h6;
      'hA6: b = 'h24;
      'hA7: b = 'h5C;
      'hA8: b = 'hC2;
      'hA9: b = 'hD3;
      'hAA: b = 'hAC;
      'hAB: b = 'h62;
      'hAC: b = 'h91;
      'hAD: b = 'h95;
      'hAE: b = 'hE4;
      'hAF: b = 'h79;
      'hB0: b = 'hE7;
      'hB1: b = 'hC8;
      'hB2: b = 'h37;
      'hB3: b = 'h6D;
      'hB4: b = 'h8D;
      'hB5: b = 'hD5;
      'hB6: b = 'h4E;
      'hB7: b = 'hA9;
      'hB8: b = 'h6C;
      'hB9: b = 'h56;
      'hBA: b = 'hF4;
      'hBB: b = 'hEA;
      'hBC: b = 'h65;
      'hBD: b = 'h7A;
      'hBE: b = 'hAE;
      'hBF: b = 'h8;
      'hC0: b = 'hBA;
      'hC1: b = 'h78;
      'hC2: b = 'h25;
      'hC3: b = 'h2E;
      'hC4: b = 'h1C;
      'hC5: b = 'hA6;
      'hC6: b = 'hB4;
      'hC7: b = 'hC6;
      'hC8: b = 'hE8;
      'hC9: b = 'hDD;
      'hCA: b = 'h74;
      'hCB: b = 'h1F;
      'hCC: b = 'h4B;
      'hCD: b = 'hBD;
      'hCE: b = 'h8B;
      'hCF: b = 'h8A;
      'hD0: b = 'h70;
      'hD1: b = 'h3E;
      'hD2: b = 'hB5;
      'hD3: b = 'h66;
      'hD4: b = 'h48;
      'hD5: b = 'h3;
      'hD6: b = 'hF6;
      'hD7: b = 'hE;
      'hD8: b = 'h61;
      'hD9: b = 'h35;
      'hDA: b = 'h57;
      'hDB: b = 'hB9;
      'hDC: b = 'h86;
      'hDD: b = 'hC1;
      'hDE: b = 'h1D;
      'hDF: b = 'h9E;
      'hE0: b = 'hE1;
      'hE1: b = 'hF8;
      'hE2: b = 'h98;
      'hE3: b = 'h11;
      'hE4: b = 'h69;
      'hE5: b = 'hD9;
      'hE6: b = 'h8E;
      'hE7: b = 'h94;
      'hE8: b = 'h9B;
      'hE9: b = 'h1E;
      'hEA: b = 'h87;
      'hEB: b = 'hE9;
      'hEC: b = 'hCE;
      'hED: b = 'h55;
      'hEE: b = 'h28;
      'hEF: b = 'hDF;
      'hF0: b = 'h8C;
      'hF1: b = 'hA1;
      'hF2: b = 'h89;
      'hF3: b = 'hD;
      'hF4: b = 'hBF;
      'hF5: b = 'hE6;
      'hF6: b = 'h42;
      'hF7: b = 'h68;
      'hF8: b = 'h41;
      'hF9: b = 'h99;
      'hFA: b = 'h2D;
      'hFB: b = 'hF;
      'hFC: b = 'hB0;
      'hFD: b = 'h54;
      'hFE: b = 'hBB;
      'hFF: b = 'h16;
      default: b = 'h0;
    endcase
  end

endmodule

// AES Round Constant Generator (standalone module)
module AesRcon (
  input logic clk,
  input logic kld,
  output logic [32-1:0] out_rcon
);

  logic [4-1:0] rcnt = 0;
  always_ff @(posedge clk) begin
    if (kld) begin
      rcnt <= 0;
    end else begin
      rcnt <= 4'((rcnt + 1));
    end
  end
  always_comb begin
    case (rcnt)
      'h0: out_rcon = 'h1000000;
      'h1: out_rcon = 'h2000000;
      'h2: out_rcon = 'h4000000;
      'h3: out_rcon = 'h8000000;
      'h4: out_rcon = 'h10000000;
      'h5: out_rcon = 'h20000000;
      'h6: out_rcon = 'h40000000;
      'h7: out_rcon = 'h80000000;
      'h8: out_rcon = 'h1B000000;
      'h9: out_rcon = 'h36000000;
      default: out_rcon = 'h0;
    endcase
  end

endmodule

// AES 128-bit Key Expansion (standalone module, depends on AesSbox + AesRcon)
module AesKeyExpand128 (
  input logic clk,
  input logic kld,
  input logic [128-1:0] key,
  output logic [32-1:0] wo_0,
  output logic [32-1:0] wo_1,
  output logic [32-1:0] wo_2,
  output logic [32-1:0] wo_3
);

  logic [32-1:0] w0 = 0;
  logic [32-1:0] w1 = 0;
  logic [32-1:0] w2 = 0;
  logic [32-1:0] w3 = 0;
  logic [8-1:0] subword0;
  AesSbox sbox0 (
    .a(w3[23:16]),
    .b(subword0)
  );
  logic [8-1:0] subword1;
  AesSbox sbox1 (
    .a(w3[15:8]),
    .b(subword1)
  );
  logic [8-1:0] subword2;
  AesSbox sbox2 (
    .a(w3[7:0]),
    .b(subword2)
  );
  logic [8-1:0] subword3;
  AesSbox sbox3 (
    .a(w3[31:24]),
    .b(subword3)
  );
  logic [32-1:0] rcon_val;
  AesRcon rcon0 (
    .clk(clk),
    .kld(kld),
    .out_rcon(rcon_val)
  );
  logic [32-1:0] t;
  assign t = ({subword0, subword1, subword2, subword3} ^ rcon_val);
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
    end else begin
      w0 <= nw0;
      w1 <= nw1;
      w2 <= nw2;
      w3 <= nw3;
    end
  end
  assign wo_0 = w0;
  assign wo_1 = w1;
  assign wo_2 = w2;
  assign wo_3 = w3;

endmodule

// GF(2^8) multiply by 2
// xtime(a) = (a << 1) ^ (0x1b if a[7] else 0x00)
module Xtime (
  input logic [8-1:0] a,
  output logic [8-1:0] y
);

  always_comb begin
    case ((a & 'h80))
      'h80: y = (8'((a << 1)) ^ 'h1B);
      default: y = 8'((a << 1));
    endcase
  end

endmodule

// AES-128 Encryption Top Module (standalone, depends on AesSbox, AesRcon, AesKeyExpand128, Xtime)
module AesCipherTop (
  input logic clk,
  input logic rst,
  input logic ld,
  output logic done,
  input logic [128-1:0] key,
  input logic [128-1:0] text_in,
  output logic [128-1:0] text_out
);

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
  // ── 16 S-box instances ──
  logic [8-1:0] sa00_sub;
  AesSbox us00 (
    .a(sa00),
    .b(sa00_sub)
  );
  logic [8-1:0] sa01_sub;
  AesSbox us01 (
    .a(sa01),
    .b(sa01_sub)
  );
  logic [8-1:0] sa02_sub;
  AesSbox us02 (
    .a(sa02),
    .b(sa02_sub)
  );
  logic [8-1:0] sa03_sub;
  AesSbox us03 (
    .a(sa03),
    .b(sa03_sub)
  );
  logic [8-1:0] sa10_sub;
  AesSbox us10 (
    .a(sa10),
    .b(sa10_sub)
  );
  logic [8-1:0] sa11_sub;
  AesSbox us11 (
    .a(sa11),
    .b(sa11_sub)
  );
  logic [8-1:0] sa12_sub;
  AesSbox us12 (
    .a(sa12),
    .b(sa12_sub)
  );
  logic [8-1:0] sa13_sub;
  AesSbox us13 (
    .a(sa13),
    .b(sa13_sub)
  );
  logic [8-1:0] sa20_sub;
  AesSbox us20 (
    .a(sa20),
    .b(sa20_sub)
  );
  logic [8-1:0] sa21_sub;
  AesSbox us21 (
    .a(sa21),
    .b(sa21_sub)
  );
  logic [8-1:0] sa22_sub;
  AesSbox us22 (
    .a(sa22),
    .b(sa22_sub)
  );
  logic [8-1:0] sa23_sub;
  AesSbox us23 (
    .a(sa23),
    .b(sa23_sub)
  );
  logic [8-1:0] sa30_sub;
  AesSbox us30 (
    .a(sa30),
    .b(sa30_sub)
  );
  logic [8-1:0] sa31_sub;
  AesSbox us31 (
    .a(sa31),
    .b(sa31_sub)
  );
  logic [8-1:0] sa32_sub;
  AesSbox us32 (
    .a(sa32),
    .b(sa32_sub)
  );
  logic [8-1:0] sa33_sub;
  AesSbox us33 (
    .a(sa33),
    .b(sa33_sub)
  );
  // ── ShiftRows ──
  logic [8-1:0] sa00_sr;
  assign sa00_sr = sa00_sub;
  logic [8-1:0] sa01_sr;
  assign sa01_sr = sa01_sub;
  logic [8-1:0] sa02_sr;
  assign sa02_sr = sa02_sub;
  logic [8-1:0] sa03_sr;
  assign sa03_sr = sa03_sub;
  logic [8-1:0] sa10_sr;
  assign sa10_sr = sa11_sub;
  logic [8-1:0] sa11_sr;
  assign sa11_sr = sa12_sub;
  logic [8-1:0] sa12_sr;
  assign sa12_sr = sa13_sub;
  logic [8-1:0] sa13_sr;
  assign sa13_sr = sa10_sub;
  logic [8-1:0] sa20_sr;
  assign sa20_sr = sa22_sub;
  logic [8-1:0] sa21_sr;
  assign sa21_sr = sa23_sub;
  logic [8-1:0] sa22_sr;
  assign sa22_sr = sa20_sub;
  logic [8-1:0] sa23_sr;
  assign sa23_sr = sa21_sub;
  logic [8-1:0] sa30_sr;
  assign sa30_sr = sa33_sub;
  logic [8-1:0] sa31_sr;
  assign sa31_sr = sa30_sub;
  logic [8-1:0] sa32_sr;
  assign sa32_sr = sa31_sub;
  logic [8-1:0] sa33_sr;
  assign sa33_sr = sa32_sub;
  // ── xtime instances for MixColumns ──
  logic [8-1:0] xt00_y;
  Xtime xt00 (
    .a(sa00_sr),
    .y(xt00_y)
  );
  logic [8-1:0] xt01_y;
  Xtime xt01 (
    .a(sa01_sr),
    .y(xt01_y)
  );
  logic [8-1:0] xt02_y;
  Xtime xt02 (
    .a(sa02_sr),
    .y(xt02_y)
  );
  logic [8-1:0] xt03_y;
  Xtime xt03 (
    .a(sa03_sr),
    .y(xt03_y)
  );
  logic [8-1:0] xt10_y;
  Xtime xt10 (
    .a(sa10_sr),
    .y(xt10_y)
  );
  logic [8-1:0] xt11_y;
  Xtime xt11 (
    .a(sa11_sr),
    .y(xt11_y)
  );
  logic [8-1:0] xt12_y;
  Xtime xt12 (
    .a(sa12_sr),
    .y(xt12_y)
  );
  logic [8-1:0] xt13_y;
  Xtime xt13 (
    .a(sa13_sr),
    .y(xt13_y)
  );
  logic [8-1:0] xt20_y;
  Xtime xt20 (
    .a(sa20_sr),
    .y(xt20_y)
  );
  logic [8-1:0] xt21_y;
  Xtime xt21 (
    .a(sa21_sr),
    .y(xt21_y)
  );
  logic [8-1:0] xt22_y;
  Xtime xt22 (
    .a(sa22_sr),
    .y(xt22_y)
  );
  logic [8-1:0] xt23_y;
  Xtime xt23 (
    .a(sa23_sr),
    .y(xt23_y)
  );
  logic [8-1:0] xt30_y;
  Xtime xt30 (
    .a(sa30_sr),
    .y(xt30_y)
  );
  logic [8-1:0] xt31_y;
  Xtime xt31 (
    .a(sa31_sr),
    .y(xt31_y)
  );
  logic [8-1:0] xt32_y;
  Xtime xt32 (
    .a(sa32_sr),
    .y(xt32_y)
  );
  logic [8-1:0] xt33_y;
  Xtime xt33 (
    .a(sa33_sr),
    .y(xt33_y)
  );
  // ── MixColumns ──
  logic [8-1:0] sa00_mc;
  assign sa00_mc = ((((xt00_y ^ xt10_y) ^ sa10_sr) ^ sa20_sr) ^ sa30_sr);
  logic [8-1:0] sa10_mc;
  assign sa10_mc = ((((sa00_sr ^ xt10_y) ^ xt20_y) ^ sa20_sr) ^ sa30_sr);
  logic [8-1:0] sa20_mc;
  assign sa20_mc = ((((sa00_sr ^ sa10_sr) ^ xt20_y) ^ xt30_y) ^ sa30_sr);
  logic [8-1:0] sa30_mc;
  assign sa30_mc = ((((xt00_y ^ sa00_sr) ^ sa10_sr) ^ sa20_sr) ^ xt30_y);
  logic [8-1:0] sa01_mc;
  assign sa01_mc = ((((xt01_y ^ xt11_y) ^ sa11_sr) ^ sa21_sr) ^ sa31_sr);
  logic [8-1:0] sa11_mc;
  assign sa11_mc = ((((sa01_sr ^ xt11_y) ^ xt21_y) ^ sa21_sr) ^ sa31_sr);
  logic [8-1:0] sa21_mc;
  assign sa21_mc = ((((sa01_sr ^ sa11_sr) ^ xt21_y) ^ xt31_y) ^ sa31_sr);
  logic [8-1:0] sa31_mc;
  assign sa31_mc = ((((xt01_y ^ sa01_sr) ^ sa11_sr) ^ sa21_sr) ^ xt31_y);
  logic [8-1:0] sa02_mc;
  assign sa02_mc = ((((xt02_y ^ xt12_y) ^ sa12_sr) ^ sa22_sr) ^ sa32_sr);
  logic [8-1:0] sa12_mc;
  assign sa12_mc = ((((sa02_sr ^ xt12_y) ^ xt22_y) ^ sa22_sr) ^ sa32_sr);
  logic [8-1:0] sa22_mc;
  assign sa22_mc = ((((sa02_sr ^ sa12_sr) ^ xt22_y) ^ xt32_y) ^ sa32_sr);
  logic [8-1:0] sa32_mc;
  assign sa32_mc = ((((xt02_y ^ sa02_sr) ^ sa12_sr) ^ sa22_sr) ^ xt32_y);
  logic [8-1:0] sa03_mc;
  assign sa03_mc = ((((xt03_y ^ xt13_y) ^ sa13_sr) ^ sa23_sr) ^ sa33_sr);
  logic [8-1:0] sa13_mc;
  assign sa13_mc = ((((sa03_sr ^ xt13_y) ^ xt23_y) ^ sa23_sr) ^ sa33_sr);
  logic [8-1:0] sa23_mc;
  assign sa23_mc = ((((sa03_sr ^ sa13_sr) ^ xt23_y) ^ xt33_y) ^ sa33_sr);
  logic [8-1:0] sa33_mc;
  assign sa33_mc = ((((xt03_y ^ sa03_sr) ^ sa13_sr) ^ sa23_sr) ^ xt33_y);
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
  // ── AddRoundKey after ShiftRows only (final round) ──
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
      end else begin
        if ((dcnt != 0)) begin
          dcnt <= 4'((dcnt - 1));
        end
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
      end else begin
        if ((dcnt > 1)) begin
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
        end else begin
          if ((dcnt == 1)) begin
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
        end
      end
      if (ld) begin
        text_in_r <= text_in;
      end
      if (ld) begin
        done_r <= 1'b0;
      end else begin
        if ((dcnt == 1)) begin
          done_r <= 1'b1;
        end
      end
      if ((dcnt == 1)) begin
        text_out_r <= {sa00_fark, sa10_fark, sa20_fark, sa30_fark, sa01_fark, sa11_fark, sa21_fark, sa31_fark, sa02_fark, sa12_fark, sa22_fark, sa32_fark, sa03_fark, sa13_fark, sa23_fark, sa33_fark};
      end
    end
  end
  assign done = done_r;
  assign text_out = text_out_r;

endmodule

