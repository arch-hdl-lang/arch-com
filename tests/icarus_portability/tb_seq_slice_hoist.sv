// arch#846 regression: hoist temps synthesized inside a procedural scope
// must still compute the right values once their declaration + continuous
// assign are relocated to module scope (and, for a `function` body, once
// the assignment becomes blocking). Icarus half; the Verilator C++ harness
// (tb_seq_slice_hoist_verilator.cpp) uses identical vectors/expectations.
//
// Hand-derived from the ARCH semantics:
//   w=0xBEEF -> {w,w}[5:2]      = (0xBEEF>>2)&0xF = 4'hB   (registered)
//               w.trunc<8>()    = 8'hEF, [7:4]    = 4'hE   (registered)
//               (w-1)=0xBEEE    -> bit 3          = 1'b1   (registered)
//   w=0x1234 -> {w,w}[5:2]      = (0x1234>>2)&0xF = 4'hD
//               w.trunc<8>()    = 8'h34, [7:4]    = 4'h3
//               (w-1)=0x1233    -> bit 3          = 1'b0
//   a=0xA5   -> {a,a}=0xA5A5, [9:6]               = 4'h6   (comb + function)
//   a=0xB2   -> {a,a}=0xB2B2, [9:6]               = 4'hA
module tb;
  logic clk;
  logic rst;
  logic [7:0] a;
  logic [15:0] w;
  logic sel;
  logic [3:0] y_seq_concat, y_seq_trunc, y_comb_concat, y_fn_concat;
  logic y_seq_idx;

  SeqSliceHoist dut(
    .clk(clk), .rst(rst), .a(a), .w(w), .sel(sel),
    .y_seq_concat(y_seq_concat),
    .y_seq_trunc(y_seq_trunc),
    .y_seq_idx(y_seq_idx),
    .y_comb_concat(y_comb_concat),
    .y_fn_concat(y_fn_concat)
  );

  initial clk = 1'b0;
  always #5 clk = ~clk;

  task check(input [3:0] got, input [3:0] want, input string label);
    if (got !== want) begin
      $display("FAIL %0s: got=%h expect=%h", label, got, want);
      $finish(1);
    end
  endtask

  initial begin
    rst = 1'b1; a = 8'hA5; w = 16'hBEEF; sel = 1'b1;
    @(posedge clk);
    @(negedge clk);
    rst = 1'b0;
    @(posedge clk);
    @(negedge clk);
    check(y_seq_concat,        4'hB, "seq_concat case1");
    check(y_seq_trunc,         4'hE, "seq_trunc case1");
    check({3'b0, y_seq_idx},   4'h1, "seq_idx case1");
    check(y_comb_concat,       4'h6, "comb_concat case1");
    check(y_fn_concat,         4'h6, "fn_concat case1");

    a = 8'hB2; w = 16'h1234; sel = 1'b1;
    @(posedge clk);
    @(negedge clk);
    check(y_seq_concat,        4'hD, "seq_concat case2");
    check(y_seq_trunc,         4'h3, "seq_trunc case2");
    check({3'b0, y_seq_idx},   4'h0, "seq_idx case2");
    check(y_comb_concat,       4'hA, "comb_concat case2");
    check(y_fn_concat,         4'hA, "fn_concat case2");

    sel = 1'b0;
    #1;
    check(y_comb_concat,       4'h0, "comb_concat sel=0");
    check(y_fn_concat,         4'hA, "fn_concat sel=0");

    $display("PASS");
    $finish(0);
  end
endmodule
