module halfband_fir #(
  parameter int IW = 16,
  parameter int OW = 32,
  parameter int TW = 12,
  parameter int NTAPS = 7
) (
  input logic i_clk,
  input logic i_reset,
  input logic i_tap_wr,
  input logic [12-1:0] i_tap,
  input logic i_ce,
  input logic signed [16-1:0] i_sample,
  output logic o_ce,
  output logic signed [32-1:0] o_result
);

  // Tap storage — write pointer increments on i_tap_wr
  logic signed [12-1:0] tap0;
  logic signed [12-1:0] tap1;
  logic signed [12-1:0] tap2;
  logic signed [12-1:0] tap3;
  logic signed [12-1:0] tap4;
  logic signed [12-1:0] tap5;
  logic signed [12-1:0] tap6;
  logic [3-1:0] tap_ptr;
  // Sample shift register
  logic signed [16-1:0] sr0;
  logic signed [16-1:0] sr1;
  logic signed [16-1:0] sr2;
  logic signed [16-1:0] sr3;
  logic signed [16-1:0] sr4;
  logic signed [16-1:0] sr5;
  logic signed [16-1:0] sr6;
  logic signed [32-1:0] result_r;
  logic ce_r;
  // Extend both operands to 32 bits before multiply → 64-bit product, trunc to 32
  logic signed [32-1:0] p0;
  assign p0 = 32'({{(32-$bits(tap0)){tap0[$bits(tap0)-1]}}, tap0} * {{(32-$bits(sr0)){sr0[$bits(sr0)-1]}}, sr0});
  logic signed [32-1:0] p1;
  assign p1 = 32'({{(32-$bits(tap1)){tap1[$bits(tap1)-1]}}, tap1} * {{(32-$bits(sr1)){sr1[$bits(sr1)-1]}}, sr1});
  logic signed [32-1:0] p2;
  assign p2 = 32'({{(32-$bits(tap2)){tap2[$bits(tap2)-1]}}, tap2} * {{(32-$bits(sr2)){sr2[$bits(sr2)-1]}}, sr2});
  logic signed [32-1:0] p3;
  assign p3 = 32'({{(32-$bits(tap3)){tap3[$bits(tap3)-1]}}, tap3} * {{(32-$bits(sr3)){sr3[$bits(sr3)-1]}}, sr3});
  logic signed [32-1:0] p4;
  assign p4 = 32'({{(32-$bits(tap4)){tap4[$bits(tap4)-1]}}, tap4} * {{(32-$bits(sr4)){sr4[$bits(sr4)-1]}}, sr4});
  logic signed [32-1:0] p5;
  assign p5 = 32'({{(32-$bits(tap5)){tap5[$bits(tap5)-1]}}, tap5} * {{(32-$bits(sr5)){sr5[$bits(sr5)-1]}}, sr5});
  logic signed [32-1:0] p6;
  assign p6 = 32'({{(32-$bits(tap6)){tap6[$bits(tap6)-1]}}, tap6} * {{(32-$bits(sr6)){sr6[$bits(sr6)-1]}}, sr6});
  logic signed [32-1:0] sum01;
  assign sum01 = 32'(p0 + p1);
  logic signed [32-1:0] sum23;
  assign sum23 = 32'(p2 + p3);
  logic signed [32-1:0] sum45;
  assign sum45 = 32'(p4 + p5);
  logic signed [32-1:0] acc;
  assign acc = 32'(32'(sum01 + sum23) + 32'(sum45 + p6));
  assign o_result = result_r;
  assign o_ce = ce_r;
  always_ff @(posedge i_clk) begin
    if (i_reset) begin
      ce_r <= 1'b0;
      result_r <= 0;
      sr0 <= 0;
      sr1 <= 0;
      sr2 <= 0;
      sr3 <= 0;
      sr4 <= 0;
      sr5 <= 0;
      sr6 <= 0;
      tap0 <= 0;
      tap1 <= 0;
      tap2 <= 0;
      tap3 <= 0;
      tap4 <= 0;
      tap5 <= 0;
      tap6 <= 0;
      tap_ptr <= 0;
    end else begin
      ce_r <= i_ce;
      if (i_tap_wr) begin
        if (tap_ptr == 0) begin
          tap0 <= $signed(i_tap);
        end else if (tap_ptr == 1) begin
          tap1 <= $signed(i_tap);
        end else if (tap_ptr == 2) begin
          tap2 <= $signed(i_tap);
        end else if (tap_ptr == 3) begin
          tap3 <= $signed(i_tap);
        end else if (tap_ptr == 4) begin
          tap4 <= $signed(i_tap);
        end else if (tap_ptr == 5) begin
          tap5 <= $signed(i_tap);
        end else begin
          tap6 <= $signed(i_tap);
        end
        tap_ptr <= 3'(tap_ptr + 1);
      end
      if (i_ce) begin
        sr0 <= i_sample;
        sr1 <= sr0;
        sr2 <= sr1;
        sr3 <= sr2;
        sr4 <= sr3;
        sr5 <= sr4;
        sr6 <= sr5;
        result_r <= acc;
      end
    end
  end

endmodule

