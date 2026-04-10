// AES-128 Key Expansion Module (area-optimized)
// Expands 128-bit key into 11 round keys (1408-bit output).
// Uses 4 sbox instances, processes 1 round per cycle (10 cycles).
module key_expansion_128aes (
  input logic clk,
  input logic rst_async_n,
  input logic i_start,
  input logic [128-1:0] i_key,
  output logic o_done,
  output logic [1408-1:0] o_expanded_key
);

  // Current working words
  logic [32-1:0] w0;
  logic [32-1:0] w1;
  logic [32-1:0] w2;
  logic [32-1:0] w3;
  // Round counter (0 = idle/done, 1..10 = expanding)
  logic [4-1:0] round_cnt;
  // Expanded key storage: 11 round keys x 128 bits
  logic [128-1:0] rk0;
  logic [128-1:0] rk1;
  logic [128-1:0] rk2;
  logic [128-1:0] rk3;
  logic [128-1:0] rk4;
  logic [128-1:0] rk5;
  logic [128-1:0] rk6;
  logic [128-1:0] rk7;
  logic [128-1:0] rk8;
  logic [128-1:0] rk9;
  logic [128-1:0] rk10;
  // S-box instances for SubWord(RotWord(w3))
  // RotWord([b0,b1,b2,b3]) = [b1,b2,b3,b0]
  // w3 = [w3[31:24], w3[23:16], w3[15:8], w3[7:0]]
  // After RotWord: [w3[23:16], w3[15:8], w3[7:0], w3[31:24]]
  logic [8-1:0] sb_out0;
  sbox sb0 (
    .i_addr(w3[23:16]),
    .o_data(sb_out0)
  );
  logic [8-1:0] sb_out1;
  sbox sb1 (
    .i_addr(w3[15:8]),
    .o_data(sb_out1)
  );
  logic [8-1:0] sb_out2;
  sbox sb2 (
    .i_addr(w3[7:0]),
    .o_data(sb_out2)
  );
  logic [8-1:0] sb_out3;
  sbox sb3 (
    .i_addr(w3[31:24]),
    .o_data(sb_out3)
  );
  // Rcon lookup based on round counter
  logic [32-1:0] rcon_val;
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
  logic [32-1:0] t_val;
  assign t_val = {sb_out0, sb_out1, sb_out2, sb_out3} ^ rcon_val;
  // Next round key words
  logic [32-1:0] nw0;
  assign nw0 = w0 ^ t_val;
  logic [32-1:0] nw1;
  assign nw1 = w0 ^ t_val ^ w1;
  logic [32-1:0] nw2;
  assign nw2 = w0 ^ t_val ^ w1 ^ w2;
  logic [32-1:0] nw3;
  assign nw3 = w0 ^ t_val ^ w1 ^ w2 ^ w3;
  logic [128-1:0] new_rk;
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

