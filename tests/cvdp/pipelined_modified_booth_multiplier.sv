module pipelined_modified_booth_multiplier (
  input logic clk,
  input logic rst,
  input logic start,
  input logic signed [16-1:0] X,
  input logic signed [16-1:0] Y,
  output logic signed [32-1:0] result,
  output logic done
);

  // Stage 1 registers: latch inputs on start
  logic signed [16-1:0] s1_x;
  logic signed [16-1:0] s1_y;
  logic s1_v;
  // Stage 2 registers: partial products
  logic signed [16-1:0] s2_pp0;
  logic signed [16-1:0] s2_pp1;
  logic signed [16-1:0] s2_pp2;
  logic signed [16-1:0] s2_pp3;
  logic signed [16-1:0] s2_pp4;
  logic signed [16-1:0] s2_pp5;
  logic signed [16-1:0] s2_pp6;
  logic signed [16-1:0] s2_pp7;
  logic s2_v;
  // Stage 3 registers: pair sums
  logic signed [17-1:0] s3_s01;
  logic signed [17-1:0] s3_s23;
  logic signed [17-1:0] s3_s45;
  logic signed [17-1:0] s3_s67;
  logic s3_v;
  // Stage 4 output
  logic signed [32-1:0] s4_out;
  logic s4_v;
  // --- Stage 1 comb: Booth decode and partial products using s1_x, s1_y ---
  logic [17-1:0] b1_ext;
  assign b1_ext = {s1_y, 1'd0};
  logic [17-1:0] b1sh0;
  assign b1sh0 = b1_ext;
  logic [17-1:0] b1sh1;
  assign b1sh1 = b1_ext >> 2;
  logic [17-1:0] b1sh2;
  assign b1sh2 = b1_ext >> 4;
  logic [17-1:0] b1sh3;
  assign b1sh3 = b1_ext >> 6;
  logic [17-1:0] b1sh4;
  assign b1sh4 = b1_ext >> 8;
  logic [17-1:0] b1sh5;
  assign b1sh5 = b1_ext >> 10;
  logic [17-1:0] b1sh6;
  assign b1sh6 = b1_ext >> 12;
  logic [17-1:0] b1sh7;
  assign b1sh7 = b1_ext >> 14;
  logic [3-1:0] c0;
  assign c0 = b1sh0[2:0];
  logic [3-1:0] c1;
  assign c1 = b1sh1[2:0];
  logic [3-1:0] c2;
  assign c2 = b1sh2[2:0];
  logic [3-1:0] c3;
  assign c3 = b1sh3[2:0];
  logic [3-1:0] c4;
  assign c4 = b1sh4[2:0];
  logic [3-1:0] c5;
  assign c5 = b1sh5[2:0];
  logic [3-1:0] c6;
  assign c6 = b1sh6[2:0];
  logic [3-1:0] c7;
  assign c7 = b1sh7[2:0];
  logic signed [16-1:0] az;
  assign az = 16'(s1_x - s1_x);
  logic signed [16-1:0] a2p;
  assign a2p = s1_x << 1;
  logic signed [16-1:0] aneg;
  assign aneg = 16'(az - s1_x);
  logic signed [16-1:0] a2neg;
  assign a2neg = 16'(az - a2p);
  logic iz0;
  assign iz0 = c0 == 0 | c0 == 7;
  logic ip0;
  assign ip0 = c0 == 1 | c0 == 2;
  logic signed [16-1:0] pp0;
  assign pp0 = iz0 ? az : ip0 ? s1_x : c0 == 3 ? a2p : c0 == 4 ? a2neg : aneg;
  logic iz1;
  assign iz1 = c1 == 0 | c1 == 7;
  logic ip1;
  assign ip1 = c1 == 1 | c1 == 2;
  logic signed [16-1:0] pp1;
  assign pp1 = iz1 ? az : ip1 ? s1_x : c1 == 3 ? a2p : c1 == 4 ? a2neg : aneg;
  logic iz2;
  assign iz2 = c2 == 0 | c2 == 7;
  logic ip2;
  assign ip2 = c2 == 1 | c2 == 2;
  logic signed [16-1:0] pp2;
  assign pp2 = iz2 ? az : ip2 ? s1_x : c2 == 3 ? a2p : c2 == 4 ? a2neg : aneg;
  logic iz3;
  assign iz3 = c3 == 0 | c3 == 7;
  logic ip3;
  assign ip3 = c3 == 1 | c3 == 2;
  logic signed [16-1:0] pp3;
  assign pp3 = iz3 ? az : ip3 ? s1_x : c3 == 3 ? a2p : c3 == 4 ? a2neg : aneg;
  logic iz4;
  assign iz4 = c4 == 0 | c4 == 7;
  logic ip4;
  assign ip4 = c4 == 1 | c4 == 2;
  logic signed [16-1:0] pp4;
  assign pp4 = iz4 ? az : ip4 ? s1_x : c4 == 3 ? a2p : c4 == 4 ? a2neg : aneg;
  logic iz5;
  assign iz5 = c5 == 0 | c5 == 7;
  logic ip5;
  assign ip5 = c5 == 1 | c5 == 2;
  logic signed [16-1:0] pp5;
  assign pp5 = iz5 ? az : ip5 ? s1_x : c5 == 3 ? a2p : c5 == 4 ? a2neg : aneg;
  logic iz6;
  assign iz6 = c6 == 0 | c6 == 7;
  logic ip6;
  assign ip6 = c6 == 1 | c6 == 2;
  logic signed [16-1:0] pp6;
  assign pp6 = iz6 ? az : ip6 ? s1_x : c6 == 3 ? a2p : c6 == 4 ? a2neg : aneg;
  logic iz7;
  assign iz7 = c7 == 0 | c7 == 7;
  logic ip7;
  assign ip7 = c7 == 1 | c7 == 2;
  logic signed [16-1:0] pp7;
  assign pp7 = iz7 ? az : ip7 ? s1_x : c7 == 3 ? a2p : c7 == 4 ? a2neg : aneg;
  // --- Stage 2 comb: shift and add pairs ---
  logic signed [17-1:0] e0;
  assign e0 = {{(17-$bits(s2_pp0)){s2_pp0[$bits(s2_pp0)-1]}}, s2_pp0};
  logic signed [17-1:0] e1;
  assign e1 = {{(17-$bits(s2_pp1)){s2_pp1[$bits(s2_pp1)-1]}}, s2_pp1};
  logic signed [17-1:0] e2;
  assign e2 = {{(17-$bits(s2_pp2)){s2_pp2[$bits(s2_pp2)-1]}}, s2_pp2};
  logic signed [17-1:0] e3;
  assign e3 = {{(17-$bits(s2_pp3)){s2_pp3[$bits(s2_pp3)-1]}}, s2_pp3};
  logic signed [17-1:0] e4;
  assign e4 = {{(17-$bits(s2_pp4)){s2_pp4[$bits(s2_pp4)-1]}}, s2_pp4};
  logic signed [17-1:0] e5;
  assign e5 = {{(17-$bits(s2_pp5)){s2_pp5[$bits(s2_pp5)-1]}}, s2_pp5};
  logic signed [17-1:0] e6;
  assign e6 = {{(17-$bits(s2_pp6)){s2_pp6[$bits(s2_pp6)-1]}}, s2_pp6};
  logic signed [17-1:0] e7;
  assign e7 = {{(17-$bits(s2_pp7)){s2_pp7[$bits(s2_pp7)-1]}}, s2_pp7};
  logic signed [17-1:0] t01;
  assign t01 = 17'(e0 + (e1 << 2));
  logic signed [17-1:0] t23;
  assign t23 = 17'((e2 << 4) + (e3 << 6));
  logic signed [17-1:0] t45;
  assign t45 = 17'((e4 << 8) + (e5 << 10));
  logic signed [17-1:0] t67;
  assign t67 = 17'((e6 << 12) + (e7 << 14));
  // --- Stage 3 comb: final sum ---
  logic signed [18-1:0] sum_lo;
  assign sum_lo = s3_s01 + s3_s23;
  logic signed [18-1:0] sum_hi;
  assign sum_hi = s3_s45 + s3_s67;
  logic signed [19-1:0] fsum;
  assign fsum = sum_lo + sum_hi;
  assign result = s4_out;
  assign done = s4_v;
  always_ff @(posedge clk or posedge rst) begin
    if (rst) begin
      s1_v <= 1'b0;
      s1_x <= 0;
      s1_y <= 0;
      s2_pp0 <= 0;
      s2_pp1 <= 0;
      s2_pp2 <= 0;
      s2_pp3 <= 0;
      s2_pp4 <= 0;
      s2_pp5 <= 0;
      s2_pp6 <= 0;
      s2_pp7 <= 0;
      s2_v <= 1'b0;
      s3_s01 <= 0;
      s3_s23 <= 0;
      s3_s45 <= 0;
      s3_s67 <= 0;
      s3_v <= 1'b0;
      s4_out <= 0;
      s4_v <= 1'b0;
    end else begin
      // Stage 1: latch inputs on start
      s1_x <= X;
      s1_y <= Y;
      s1_v <= start;
      // Stage 2: compute partial products from s1 latched values
      s2_pp0 <= pp0;
      s2_pp1 <= pp1;
      s2_pp2 <= pp2;
      s2_pp3 <= pp3;
      s2_pp4 <= pp4;
      s2_pp5 <= pp5;
      s2_pp6 <= pp6;
      s2_pp7 <= pp7;
      s2_v <= s1_v;
      // Stage 3: add pairs
      s3_s01 <= t01;
      s3_s23 <= t23;
      s3_s45 <= t45;
      s3_s67 <= t67;
      s3_v <= s2_v;
      // Stage 4: final sum
      s4_out <= {{(32-$bits(fsum)){fsum[$bits(fsum)-1]}}, fsum};
      s4_v <= s3_v;
    end
  end

endmodule

