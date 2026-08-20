// arch#813 P1 regression: behavioral check under Icarus for the base kinds
// the retired allowlist used to refuse. Expectations worked out by hand
// from the ARCH semantics (see the Verilator harness for identical
// vectors):
//   a=0xA5 b=0x30 s=2 c=1 -> arith=(0x75)[3:0]=5  chain=(0xA)[1:0]=2
//                            shift=(0x52)[3:0]=2  tern=(0xA5)[3:0]=5
//                            lit=0xFF[3:2]=3
//   a=0x3C b=0x0F s=0 c=0 -> arith=(0x2D)[3:0]=D  chain=(0x3)[1:0]=3
//                            shift=(0x1E)[3:0]=E  tern=(0x0F)[3:0]=F
//                            lit=0xFF[1:0]=3
module tb;
  logic [7:0] a, b;
  logic [2:0] s;
  logic c;
  logic [3:0] y_arith, y_shift, y_tern;
  logic [1:0] y_chain, y_lit;

  UniversalSliceHoist dut(
    .a(a), .b(b), .s(s), .c(c),
    .y_arith(y_arith), .y_chain(y_chain), .y_shift(y_shift),
    .y_tern(y_tern), .y_lit(y_lit)
  );

  task check(input [3:0] got, input [3:0] want, input string label);
    if (got !== want) begin
      $display("FAIL %0s: got=%h expect=%h", label, got, want);
      $finish(1);
    end
  endtask

  initial begin
    a = 8'hA5; b = 8'h30; s = 3'd2; c = 1'b1; #1;
    check(y_arith, 4'h5, "arith case1");
    check({2'b0, y_chain}, 4'h2, "chain case1");
    check(y_shift, 4'h2, "shift case1");
    check(y_tern,  4'h5, "tern case1");
    check({2'b0, y_lit}, 4'h3, "lit case1");

    a = 8'h3C; b = 8'h0F; s = 3'd0; c = 1'b0; #1;
    check(y_arith, 4'hD, "arith case2");
    check({2'b0, y_chain}, 4'h3, "chain case2");
    check(y_shift, 4'hE, "shift case2");
    check(y_tern,  4'hF, "tern case2");
    check({2'b0, y_lit}, 4'h3, "lit case2");

    $display("PASS");
    $finish(0);
  end
endmodule
