module sbox (
  input logic [7:0] i_addr,
  output logic [7:0] o_data
);

  always_comb begin
    case (i_addr)
      'h0: o_data = 'h63;
      'h1: o_data = 'h7C;
      'h2: o_data = 'h77;
      'h3: o_data = 'h7B;
      'h4: o_data = 'hF2;
      'h5: o_data = 'h6B;
      'h6: o_data = 'h6F;
      'h7: o_data = 'hC5;
      'h8: o_data = 'h30;
      'h9: o_data = 'h1;
      'hA: o_data = 'h67;
      'hB: o_data = 'h2B;
      'hC: o_data = 'hFE;
      'hD: o_data = 'hD7;
      'hE: o_data = 'hAB;
      'hF: o_data = 'h76;
      'h10: o_data = 'hCA;
      'h11: o_data = 'h82;
      'h12: o_data = 'hC9;
      'h13: o_data = 'h7D;
      'h14: o_data = 'hFA;
      'h15: o_data = 'h59;
      'h16: o_data = 'h47;
      'h17: o_data = 'hF0;
      'h18: o_data = 'hAD;
      'h19: o_data = 'hD4;
      'h1A: o_data = 'hA2;
      'h1B: o_data = 'hAF;
      'h1C: o_data = 'h9C;
      'h1D: o_data = 'hA4;
      'h1E: o_data = 'h72;
      'h1F: o_data = 'hC0;
      'h20: o_data = 'hB7;
      'h21: o_data = 'hFD;
      'h22: o_data = 'h93;
      'h23: o_data = 'h26;
      'h24: o_data = 'h36;
      'h25: o_data = 'h3F;
      'h26: o_data = 'hF7;
      'h27: o_data = 'hCC;
      'h28: o_data = 'h34;
      'h29: o_data = 'hA5;
      'h2A: o_data = 'hE5;
      'h2B: o_data = 'hF1;
      'h2C: o_data = 'h71;
      'h2D: o_data = 'hD8;
      'h2E: o_data = 'h31;
      'h2F: o_data = 'h15;
      'h30: o_data = 'h4;
      'h31: o_data = 'hC7;
      'h32: o_data = 'h23;
      'h33: o_data = 'hC3;
      'h34: o_data = 'h18;
      'h35: o_data = 'h96;
      'h36: o_data = 'h5;
      'h37: o_data = 'h9A;
      'h38: o_data = 'h7;
      'h39: o_data = 'h12;
      'h3A: o_data = 'h80;
      'h3B: o_data = 'hE2;
      'h3C: o_data = 'hEB;
      'h3D: o_data = 'h27;
      'h3E: o_data = 'hB2;
      'h3F: o_data = 'h75;
      'h40: o_data = 'h9;
      'h41: o_data = 'h83;
      'h42: o_data = 'h2C;
      'h43: o_data = 'h1A;
      'h44: o_data = 'h1B;
      'h45: o_data = 'h6E;
      'h46: o_data = 'h5A;
      'h47: o_data = 'hA0;
      'h48: o_data = 'h52;
      'h49: o_data = 'h3B;
      'h4A: o_data = 'hD6;
      'h4B: o_data = 'hB3;
      'h4C: o_data = 'h29;
      'h4D: o_data = 'hE3;
      'h4E: o_data = 'h2F;
      'h4F: o_data = 'h84;
      'h50: o_data = 'h53;
      'h51: o_data = 'hD1;
      'h52: o_data = 'h0;
      'h53: o_data = 'hED;
      'h54: o_data = 'h20;
      'h55: o_data = 'hFC;
      'h56: o_data = 'hB1;
      'h57: o_data = 'h5B;
      'h58: o_data = 'h6A;
      'h59: o_data = 'hCB;
      'h5A: o_data = 'hBE;
      'h5B: o_data = 'h39;
      'h5C: o_data = 'h4A;
      'h5D: o_data = 'h4C;
      'h5E: o_data = 'h58;
      'h5F: o_data = 'hCF;
      'h60: o_data = 'hD0;
      'h61: o_data = 'hEF;
      'h62: o_data = 'hAA;
      'h63: o_data = 'hFB;
      'h64: o_data = 'h43;
      'h65: o_data = 'h4D;
      'h66: o_data = 'h33;
      'h67: o_data = 'h85;
      'h68: o_data = 'h45;
      'h69: o_data = 'hF9;
      'h6A: o_data = 'h2;
      'h6B: o_data = 'h7F;
      'h6C: o_data = 'h50;
      'h6D: o_data = 'h3C;
      'h6E: o_data = 'h9F;
      'h6F: o_data = 'hA8;
      'h70: o_data = 'h51;
      'h71: o_data = 'hA3;
      'h72: o_data = 'h40;
      'h73: o_data = 'h8F;
      'h74: o_data = 'h92;
      'h75: o_data = 'h9D;
      'h76: o_data = 'h38;
      'h77: o_data = 'hF5;
      'h78: o_data = 'hBC;
      'h79: o_data = 'hB6;
      'h7A: o_data = 'hDA;
      'h7B: o_data = 'h21;
      'h7C: o_data = 'h10;
      'h7D: o_data = 'hFF;
      'h7E: o_data = 'hF3;
      'h7F: o_data = 'hD2;
      'h80: o_data = 'hCD;
      'h81: o_data = 'hC;
      'h82: o_data = 'h13;
      'h83: o_data = 'hEC;
      'h84: o_data = 'h5F;
      'h85: o_data = 'h97;
      'h86: o_data = 'h44;
      'h87: o_data = 'h17;
      'h88: o_data = 'hC4;
      'h89: o_data = 'hA7;
      'h8A: o_data = 'h7E;
      'h8B: o_data = 'h3D;
      'h8C: o_data = 'h64;
      'h8D: o_data = 'h5D;
      'h8E: o_data = 'h19;
      'h8F: o_data = 'h73;
      'h90: o_data = 'h60;
      'h91: o_data = 'h81;
      'h92: o_data = 'h4F;
      'h93: o_data = 'hDC;
      'h94: o_data = 'h22;
      'h95: o_data = 'h2A;
      'h96: o_data = 'h90;
      'h97: o_data = 'h88;
      'h98: o_data = 'h46;
      'h99: o_data = 'hEE;
      'h9A: o_data = 'hB8;
      'h9B: o_data = 'h14;
      'h9C: o_data = 'hDE;
      'h9D: o_data = 'h5E;
      'h9E: o_data = 'hB;
      'h9F: o_data = 'hDB;
      'hA0: o_data = 'hE0;
      'hA1: o_data = 'h32;
      'hA2: o_data = 'h3A;
      'hA3: o_data = 'hA;
      'hA4: o_data = 'h49;
      'hA5: o_data = 'h6;
      'hA6: o_data = 'h24;
      'hA7: o_data = 'h5C;
      'hA8: o_data = 'hC2;
      'hA9: o_data = 'hD3;
      'hAA: o_data = 'hAC;
      'hAB: o_data = 'h62;
      'hAC: o_data = 'h91;
      'hAD: o_data = 'h95;
      'hAE: o_data = 'hE4;
      'hAF: o_data = 'h79;
      'hB0: o_data = 'hE7;
      'hB1: o_data = 'hC8;
      'hB2: o_data = 'h37;
      'hB3: o_data = 'h6D;
      'hB4: o_data = 'h8D;
      'hB5: o_data = 'hD5;
      'hB6: o_data = 'h4E;
      'hB7: o_data = 'hA9;
      'hB8: o_data = 'h6C;
      'hB9: o_data = 'h56;
      'hBA: o_data = 'hF4;
      'hBB: o_data = 'hEA;
      'hBC: o_data = 'h65;
      'hBD: o_data = 'h7A;
      'hBE: o_data = 'hAE;
      'hBF: o_data = 'h8;
      'hC0: o_data = 'hBA;
      'hC1: o_data = 'h78;
      'hC2: o_data = 'h25;
      'hC3: o_data = 'h2E;
      'hC4: o_data = 'h1C;
      'hC5: o_data = 'hA6;
      'hC6: o_data = 'hB4;
      'hC7: o_data = 'hC6;
      'hC8: o_data = 'hE8;
      'hC9: o_data = 'hDD;
      'hCA: o_data = 'h74;
      'hCB: o_data = 'h1F;
      'hCC: o_data = 'h4B;
      'hCD: o_data = 'hBD;
      'hCE: o_data = 'h8B;
      'hCF: o_data = 'h8A;
      'hD0: o_data = 'h70;
      'hD1: o_data = 'h3E;
      'hD2: o_data = 'hB5;
      'hD3: o_data = 'h66;
      'hD4: o_data = 'h48;
      'hD5: o_data = 'h3;
      'hD6: o_data = 'hF6;
      'hD7: o_data = 'hE;
      'hD8: o_data = 'h61;
      'hD9: o_data = 'h35;
      'hDA: o_data = 'h57;
      'hDB: o_data = 'hB9;
      'hDC: o_data = 'h86;
      'hDD: o_data = 'hC1;
      'hDE: o_data = 'h1D;
      'hDF: o_data = 'h9E;
      'hE0: o_data = 'hE1;
      'hE1: o_data = 'hF8;
      'hE2: o_data = 'h98;
      'hE3: o_data = 'h11;
      'hE4: o_data = 'h69;
      'hE5: o_data = 'hD9;
      'hE6: o_data = 'h8E;
      'hE7: o_data = 'h94;
      'hE8: o_data = 'h9B;
      'hE9: o_data = 'h1E;
      'hEA: o_data = 'h87;
      'hEB: o_data = 'hE9;
      'hEC: o_data = 'hCE;
      'hED: o_data = 'h55;
      'hEE: o_data = 'h28;
      'hEF: o_data = 'hDF;
      'hF0: o_data = 'h8C;
      'hF1: o_data = 'hA1;
      'hF2: o_data = 'h89;
      'hF3: o_data = 'hD;
      'hF4: o_data = 'hBF;
      'hF5: o_data = 'hE6;
      'hF6: o_data = 'h42;
      'hF7: o_data = 'h68;
      'hF8: o_data = 'h41;
      'hF9: o_data = 'h99;
      'hFA: o_data = 'h2D;
      'hFB: o_data = 'hF;
      'hFC: o_data = 'hB0;
      'hFD: o_data = 'h54;
      'hFE: o_data = 'hBB;
      'hFF: o_data = 'h16;
      default: o_data = 'h0;
    endcase
  end

endmodule

// AES-128 Key Expansion Module (area-optimized)
// Expands 128-bit key into 11 round keys (1408-bit output).
// Uses 4 sbox instances, processes 1 round per cycle (10 cycles).
module key_expansion_128aes (
  input logic clk,
  input logic rst_async_n,
  input logic i_start,
  input logic [127:0] i_key,
  output logic o_done,
  output logic [1407:0] o_expanded_key
);

  // Current working words
  logic [31:0] w0;
  logic [31:0] w1;
  logic [31:0] w2;
  logic [31:0] w3;
  // Round counter (0 = idle/done, 1..10 = expanding)
  logic [3:0] round_cnt;
  // Expanded key storage: 11 round keys x 128 bits
  logic [127:0] rk0;
  logic [127:0] rk1;
  logic [127:0] rk2;
  logic [127:0] rk3;
  logic [127:0] rk4;
  logic [127:0] rk5;
  logic [127:0] rk6;
  logic [127:0] rk7;
  logic [127:0] rk8;
  logic [127:0] rk9;
  logic [127:0] rk10;
  // S-box instances for SubWord(RotWord(w3))
  // RotWord([b0,b1,b2,b3]) = [b1,b2,b3,b0]
  // w3 = [w3[31:24], w3[23:16], w3[15:8], w3[7:0]]
  // After RotWord: [w3[23:16], w3[15:8], w3[7:0], w3[31:24]]
  logic [7:0] sb_out0;
  sbox sb0 (
    .i_addr(w3[23:16]),
    .o_data(sb_out0)
  );
  logic [7:0] sb_out1;
  sbox sb1 (
    .i_addr(w3[15:8]),
    .o_data(sb_out1)
  );
  logic [7:0] sb_out2;
  sbox sb2 (
    .i_addr(w3[7:0]),
    .o_data(sb_out2)
  );
  logic [7:0] sb_out3;
  sbox sb3 (
    .i_addr(w3[31:24]),
    .o_data(sb_out3)
  );
  // Rcon lookup based on round counter
  logic [31:0] rcon_val;
  always_comb begin
    case (round_cnt)
      1: rcon_val = 32'd16777216;
      2: rcon_val = 32'd33554432;
      3: rcon_val = 32'd67108864;
      4: rcon_val = 32'd134217728;
      5: rcon_val = 32'd268435456;
      6: rcon_val = 32'd536870912;
      7: rcon_val = 32'd1073741824;
      8: rcon_val = 32'd2147483648;
      9: rcon_val = 32'd452984832;
      10: rcon_val = 32'd905969664;
      default: rcon_val = 32'd0;
    endcase
  end
  // SubWord(RotWord(w3)) ^ Rcon
  logic [31:0] t_val;
  assign t_val = {sb_out0, sb_out1, sb_out2, sb_out3} ^ rcon_val;
  // Next round key words
  logic [31:0] nw0;
  assign nw0 = w0 ^ t_val;
  logic [31:0] nw1;
  assign nw1 = w0 ^ t_val ^ w1;
  logic [31:0] nw2;
  assign nw2 = w0 ^ t_val ^ w1 ^ w2;
  logic [31:0] nw3;
  assign nw3 = w0 ^ t_val ^ w1 ^ w2 ^ w3;
  logic [127:0] new_rk;
  assign new_rk = {nw0, nw1, nw2, nw3};
  // Working flag
  logic working;
  assign working = round_cnt != 0;
  // Done output
  assign o_done = !working;
  // Output: concatenate all 11 round keys
  assign o_expanded_key = {rk0, rk1, rk2, rk3, rk4, rk5, rk6, rk7, rk8, rk9, rk10};
  // Sequential logic
  always_ff @(posedge clk or negedge rst_async_n) begin
    if ((!rst_async_n)) begin
      rk0 <= 0;
      rk1 <= 0;
      rk10 <= 0;
      rk2 <= 0;
      rk3 <= 0;
      rk4 <= 0;
      rk5 <= 0;
      rk6 <= 0;
      rk7 <= 0;
      rk8 <= 0;
      rk9 <= 0;
      round_cnt <= 0;
      w0 <= 0;
      w1 <= 0;
      w2 <= 0;
      w3 <= 0;
    end else begin
      if (i_start && !working) begin
        // Load initial key
        w0 <= i_key[127:96];
        w1 <= i_key[95:64];
        w2 <= i_key[63:32];
        w3 <= i_key[31:0];
        rk0 <= i_key;
        round_cnt <= 1;
      end else if (working) begin
        // Compute next round key
        w0 <= nw0;
        w1 <= nw1;
        w2 <= nw2;
        w3 <= nw3;
        if (round_cnt == 1) begin
          rk1 <= new_rk;
        end else if (round_cnt == 2) begin
          rk2 <= new_rk;
        end else if (round_cnt == 3) begin
          rk3 <= new_rk;
        end else if (round_cnt == 4) begin
          rk4 <= new_rk;
        end else if (round_cnt == 5) begin
          rk5 <= new_rk;
        end else if (round_cnt == 6) begin
          rk6 <= new_rk;
        end else if (round_cnt == 7) begin
          rk7 <= new_rk;
        end else if (round_cnt == 8) begin
          rk8 <= new_rk;
        end else if (round_cnt == 9) begin
          rk9 <= new_rk;
        end else if (round_cnt == 10) begin
          rk10 <= new_rk;
        end
        if (round_cnt == 10) begin
          round_cnt <= 0;
        end else begin
          round_cnt <= 4'(round_cnt + 1);
        end
      end
    end
  end

endmodule

