// arch#810 regression: FunctionCall/MethodCall BitSlice/PartSelect base
// behavioral check under Icarus. Test vectors and expected values are
// worked out by hand from the ARCH semantics (see the Verilator C++
// harness for the identical vectors/expectations):
//   a=0xA5 -> Ident8(a)=8'hA5      -> [5:2]/[2+:4] = 4'h9
//   a=0xB2 -> Ident8(a)=8'hB2      -> [5:2]/[2+:4] = 4'hC
//   w=0xBEEF -> trunc<8> = 8'hEF   -> [5:2]/[2+:4] = 4'hB
//   w=0x1234 -> trunc<8> = 8'h34   -> [5:2]/[2+:4] = 4'hD
//   b=0xD  -> zext<8>   = 8'h0D    -> [5:2]/[2+:4] = 4'h3
//   b=0x6  -> zext<8>   = 8'h06    -> [5:2]/[2+:4] = 4'h1
//   a=0xA5 -> reverse<1> = 8'hA5   -> [5:2]/[2+:4] = 4'h9   (palindrome)
//   a=0xB2 -> reverse<1> = 8'h4D   -> [5:2]/[2+:4] = 4'h3
module tb;
  logic [7:0] a;
  logic [3:0] b;
  logic [15:0] w;
  logic [3:0] y_func_bitslice, y_func_partselect;
  logic [3:0] y_trunc_bitslice, y_trunc_partselect;
  logic [3:0] y_zext_bitslice, y_zext_partselect;
  logic [3:0] y_reverse_bitslice, y_reverse_partselect;

  CallSliceHoist dut(
    .a(a), .b(b), .w(w),
    .y_func_bitslice(y_func_bitslice),
    .y_func_partselect(y_func_partselect),
    .y_trunc_bitslice(y_trunc_bitslice),
    .y_trunc_partselect(y_trunc_partselect),
    .y_zext_bitslice(y_zext_bitslice),
    .y_zext_partselect(y_zext_partselect),
    .y_reverse_bitslice(y_reverse_bitslice),
    .y_reverse_partselect(y_reverse_partselect)
  );

  task check(input [3:0] got, input [3:0] want, input string label);
    if (got !== want) begin
      $display("FAIL %0s: got=%h expect=%h", label, got, want);
      $finish(1);
    end
  endtask

  initial begin
    a = 8'hA5; b = 4'hD; w = 16'hBEEF; #1;
    check(y_func_bitslice,      4'h9, "func_bitslice case1");
    check(y_func_partselect,    4'h9, "func_partselect case1");
    check(y_trunc_bitslice,     4'hB, "trunc_bitslice case1");
    check(y_trunc_partselect,   4'hB, "trunc_partselect case1");
    check(y_zext_bitslice,      4'h3, "zext_bitslice case1");
    check(y_zext_partselect,    4'h3, "zext_partselect case1");
    check(y_reverse_bitslice,   4'h9, "reverse_bitslice case1");
    check(y_reverse_partselect, 4'h9, "reverse_partselect case1");

    a = 8'hB2; b = 4'h6; w = 16'h1234; #1;
    check(y_func_bitslice,      4'hC, "func_bitslice case2");
    check(y_func_partselect,    4'hC, "func_partselect case2");
    check(y_trunc_bitslice,     4'hD, "trunc_bitslice case2");
    check(y_trunc_partselect,   4'hD, "trunc_partselect case2");
    check(y_zext_bitslice,      4'h1, "zext_bitslice case2");
    check(y_zext_partselect,    4'h1, "zext_partselect case2");
    check(y_reverse_bitslice,   4'h3, "reverse_bitslice case2");
    check(y_reverse_partselect, 4'h3, "reverse_partselect case2");

    $display("PASS");
    $finish(0);
  end
endmodule
