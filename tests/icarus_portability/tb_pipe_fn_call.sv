// arch#852 regression: a top-level `function` called from inside a
// `pipeline`, behavioral check under Icarus. Test vectors and expected
// values are worked out by hand from the ARCH semantics (see the Verilator
// C++ harness for the identical vectors/expectations). Every output is one
// clock behind its inputs.
//
//   case1: a=8'hA5 (1010_0101)  sel=1
//     Ident8(a)      = 8'hA5  -> [5:2]=4'h9  [6:3]=4'h4  [7:4]=4'hA
//     Add3(a)        = 8'hA8
//     mux (sel_r=1)  = Ident8(m_a)[7:4] = 4'hA
//
//   case2: a=8'hB2 (1011_0010)  sel=0
//     Ident8(a)      = 8'hB2  -> [5:2]=4'hC  [6:3]=4'h6
//     Add3(a)        = 8'hB5 (1011_0101)
//     mux (sel_r=0)  = Add3(m_a)[3:0] = 4'h5
module tb;
  logic clk = 0;
  logic rst = 1;
  logic [7:0] a;
  logic sel;
  logic [7:0] y_ident_seq, y_add_seq, y_add_let;
  logic [3:0] y_slice_seq, y_slice_let, y_mux_comb;

  PipeFnCall dut(
    .clk(clk), .rst(rst), .a(a), .sel(sel),
    .y_ident_seq(y_ident_seq),
    .y_add_seq(y_add_seq),
    .y_slice_seq(y_slice_seq),
    .y_add_let(y_add_let),
    .y_slice_let(y_slice_let),
    .y_mux_comb(y_mux_comb)
  );

  always #5 clk = ~clk;

  task check(input [7:0] got, input [7:0] want, input string label);
    if (got !== want) begin
      $display("FAIL %0s: got=%h expect=%h", label, got, want);
      $finish(1);
    end
  endtask

  initial begin
    a = 8'h0; sel = 1'b0;
    @(negedge clk);
    rst = 1'b0;

    a = 8'hA5; sel = 1'b1;
    @(posedge clk);
    #1;
    check(y_ident_seq, 8'hA5, "ident_seq case1");
    check(y_add_seq,   8'hA8, "add_seq case1");
    check(y_slice_seq, 8'h09, "slice_seq case1");
    check(y_add_let,   8'hA8, "add_let case1");
    check(y_slice_let, 8'h04, "slice_let case1");
    check(y_mux_comb,  8'h0A, "mux_comb case1");

    a = 8'hB2; sel = 1'b0;
    @(posedge clk);
    #1;
    check(y_ident_seq, 8'hB2, "ident_seq case2");
    check(y_add_seq,   8'hB5, "add_seq case2");
    check(y_slice_seq, 8'h0C, "slice_seq case2");
    check(y_add_let,   8'hB5, "add_let case2");
    check(y_slice_let, 8'h06, "slice_let case2");
    check(y_mux_comb,  8'h05, "mux_comb case2");

    $display("PASS");
    $finish(0);
  end
endmodule
