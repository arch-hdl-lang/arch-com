// arch#845 regression: pipeline BitSlice/PartSelect slice-base hoist,
// behavioral check under Icarus. Test vectors and expected values are
// worked out by hand from the ARCH semantics (see the Verilator C++
// harness for the identical vectors/expectations). Every output is one
// clock behind its inputs.
//
//   case1: a=8'hA5  b=4'hD  w=16'hBEEF  sel=1
//     w.trunc<8>()      = 8'hEF   -> [5:2]=4'hB   [3+:4]=4'hD
//     b.zext<8>()       = 8'h0D   -> [2+:4]=4'h3
//     {a,b}             = 12'hA5D -> [7:4]=4'h5
//     {3{b}}            = 12'hDDD -> [8+:4]=4'hD
//     a.reverse<1>()    = 8'hA5   -> [2+:4]=4'h9  [7:4]=4'hA  (palindrome)
//     {b,a}             = 12'hDA5 -> [9:6]=4'h6
//     {m_hi,m_lo}       = 8'h5D   -> [5:2]=4'h7   (sel_r=1)
//
//   case2: a=8'hB2  b=4'h6  w=16'h1234  sel=0
//     w.trunc<8>()      = 8'h34   -> [5:2]=4'hD   [3+:4]=4'h6
//     b.zext<8>()       = 8'h06   -> [2+:4]=4'h1
//     {a,b}             = 12'hB26 -> [7:4]=4'h2
//     {3{b}}            = 12'h666 -> [8+:4]=4'h6
//     a.reverse<1>()    = 8'h4D   -> [2+:4]=4'h3  [7:4]=4'h4
//     {b,a}             = 12'h6B2 -> [9:6]=4'hA
//     m_lo.zext<8>()    = 8'h06   -> [5:2]=4'h1   (sel_r=0)
module tb;
  logic clk = 0;
  logic rst = 1;
  logic [7:0] a;
  logic [3:0] b;
  logic [15:0] w;
  logic sel;
  logic [3:0] y_trunc_seq, y_zext_seq, y_concat_seq;
  logic [3:0] y_repeat_seq, y_rev_seq;
  logic [3:0] y_trunc_let, y_concat_let, y_rev_let;
  logic [3:0] y_mux_comb;

  PipeSliceHoist dut(
    .clk(clk), .rst(rst), .a(a), .b(b), .w(w), .sel(sel),
    .y_trunc_seq(y_trunc_seq),
    .y_zext_seq(y_zext_seq),
    .y_concat_seq(y_concat_seq),
    .y_repeat_seq(y_repeat_seq),
    .y_rev_seq(y_rev_seq),
    .y_trunc_let(y_trunc_let),
    .y_concat_let(y_concat_let),
    .y_rev_let(y_rev_let),
    .y_mux_comb(y_mux_comb)
  );

  always #5 clk = ~clk;

  task check(input [3:0] got, input [3:0] want, input string label);
    if (got !== want) begin
      $display("FAIL %0s: got=%h expect=%h", label, got, want);
      $finish(1);
    end
  endtask

  initial begin
    a = 8'h0; b = 4'h0; w = 16'h0; sel = 1'b0;
    @(negedge clk);
    rst = 1'b0;

    a = 8'hA5; b = 4'hD; w = 16'hBEEF; sel = 1'b1;
    @(posedge clk);
    #1;
    check(y_trunc_seq,   4'hB, "trunc_seq case1");
    check(y_zext_seq,    4'h3, "zext_seq case1");
    check(y_concat_seq,  4'h5, "concat_seq case1");
    check(y_repeat_seq,  4'hD, "repeat_seq case1");
    check(y_rev_seq,     4'h9, "rev_seq case1");
    check(y_trunc_let,   4'hD, "trunc_let case1");
    check(y_concat_let,  4'h6, "concat_let case1");
    check(y_rev_let,     4'hA, "rev_let case1");
    check(y_mux_comb,    4'h7, "mux_comb case1");

    a = 8'hB2; b = 4'h6; w = 16'h1234; sel = 1'b0;
    @(posedge clk);
    #1;
    check(y_trunc_seq,   4'hD, "trunc_seq case2");
    check(y_zext_seq,    4'h1, "zext_seq case2");
    check(y_concat_seq,  4'h2, "concat_seq case2");
    check(y_repeat_seq,  4'h6, "repeat_seq case2");
    check(y_rev_seq,     4'h3, "rev_seq case2");
    check(y_trunc_let,   4'h6, "trunc_let case2");
    check(y_concat_let,  4'hA, "concat_let case2");
    check(y_rev_let,     4'h4, "rev_let case2");
    check(y_mux_comb,    4'h1, "mux_comb case2");

    $display("PASS");
    $finish(0);
  end
endmodule
