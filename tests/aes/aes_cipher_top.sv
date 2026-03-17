// AES-128 Encryption Top Module
// Iterative AES-128 encryption: 10 rounds, one per clock cycle.
// Depends on: AesSbox, AesRcon, AesKeyExpand128, Xtime
// domain SysDomain
//   freq_mhz: 100

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
  // ── 16 S-box instances: SubBytes on state matrix ──
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

