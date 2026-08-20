// arch#861 regression: hoisted bases referencing a live `for`-loop
// iterator, behavioral check under Icarus. Vectors are chosen to separate
// the correct value from the pre-fix miscompiled one — for `y_index` the
// old emission dropped the parens, computing `a + (v[i][3])` truncated to
// one bit instead of bit 3 of `(a + v[i])`:
//
//   vector 1: a=0x08, v={0x00,0x08,0x01,0xF0}
//     y_index correct = 4'b1101, miscompiled = 4'b0010
//   vector 2: a=0x10, v={0xFF,0x00,0x88,0x00}
//     y_index correct = 4'b0101, miscompiled = 4'b1000
//
// See the Verilator C++ harness for the identical vectors/expectations.
module tb;
  logic [7:0] a;
  logic [3:0][7:0] v;
  logic [3:0] y_index;
  logic [3:0][3:0] y_concat_bitslice, y_concat_partselect, y_method_bitslice;

  LoopVarSliceHoist dut(
    .a(a), .v(v),
    .y_index(y_index),
    .y_concat_bitslice(y_concat_bitslice),
    .y_concat_partselect(y_concat_partselect),
    .y_method_bitslice(y_method_bitslice)
  );

  task check(input [3:0] got, input [3:0] want, input string label);
    if (got !== want) begin
      $display("FAIL %0s: got=%h expect=%h", label, got, want);
      $finish(1);
    end
  endtask

  initial begin
    a = 8'h08; v[0]=8'h00; v[1]=8'h08; v[2]=8'h01; v[3]=8'hF0; #1;
    check(y_index, 4'b1101, "index case1");
    check(y_concat_bitslice[0],   4'h0, "concat_bitslice[0] case1");
    check(y_concat_bitslice[1],   4'h2, "concat_bitslice[1] case1");
    check(y_concat_bitslice[2],   4'h0, "concat_bitslice[2] case1");
    check(y_concat_bitslice[3],   4'hC, "concat_bitslice[3] case1");
    check(y_concat_partselect[1], 4'h2, "concat_partselect[1] case1");
    check(y_concat_partselect[3], 4'hC, "concat_partselect[3] case1");
    check(y_method_bitslice[1],   4'h8, "method_bitslice[1] case1");
    check(y_method_bitslice[3],   4'h0, "method_bitslice[3] case1");

    a = 8'h10; v[0]=8'hFF; v[1]=8'h00; v[2]=8'h88; v[3]=8'h00; #1;
    check(y_index, 4'b0101, "index case2");
    check(y_concat_bitslice[0],   4'hF, "concat_bitslice[0] case2");
    check(y_concat_bitslice[2],   4'h2, "concat_bitslice[2] case2");
    check(y_concat_partselect[0], 4'hF, "concat_partselect[0] case2");
    check(y_concat_partselect[2], 4'h2, "concat_partselect[2] case2");
    check(y_method_bitslice[0],   4'hF, "method_bitslice[0] case2");
    check(y_method_bitslice[2],   4'h8, "method_bitslice[2] case2");

    $display("PASS");
    $finish(0);
  end
endmodule
