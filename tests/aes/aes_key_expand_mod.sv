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

