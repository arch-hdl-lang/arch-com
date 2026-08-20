// arch#807 regression: Concat/Repeat BitSlice/PartSelect base behavioral
// check under Icarus. Test vectors and expected values are worked out by
// hand from the ARCH semantics (see the Verilator C++ harness for the
// identical vectors/expectations):
//   a=0x3, c=0xA -> {a,c}=8'h3A -> [5:2]/[2+:4] = 4'hE
//   a=0xF, c=0x0 -> {a,c}=8'hF0 -> [5:2]/[2+:4] = 4'hC
//   a=0x3        -> {2{a}}=8'h33 -> [5:2]/[2+:4] = 4'hC
//   a=0x9        -> {2{a}}=8'h99 -> [5:2]/[2+:4] = 4'h6
module tb;
  logic [3:0] a, c;
  logic [3:0] y_concat_bitslice, y_concat_partselect;
  logic [3:0] y_repeat_bitslice, y_repeat_partselect;

  ConcatRepeatSliceHoist dut(
    .a(a), .c(c),
    .y_concat_bitslice(y_concat_bitslice),
    .y_concat_partselect(y_concat_partselect),
    .y_repeat_bitslice(y_repeat_bitslice),
    .y_repeat_partselect(y_repeat_partselect)
  );

  initial begin
    a = 4'h3; c = 4'hA; #1;
    if (y_concat_bitslice !== 4'hE) begin
      $display("FAIL concat_bitslice case1: got=%h expect=E", y_concat_bitslice);
      $finish(1);
    end
    if (y_concat_partselect !== 4'hE) begin
      $display("FAIL concat_partselect case1: got=%h expect=E", y_concat_partselect);
      $finish(1);
    end

    a = 4'hF; c = 4'h0; #1;
    if (y_concat_bitslice !== 4'hC) begin
      $display("FAIL concat_bitslice case2: got=%h expect=C", y_concat_bitslice);
      $finish(1);
    end
    if (y_concat_partselect !== 4'hC) begin
      $display("FAIL concat_partselect case2: got=%h expect=C", y_concat_partselect);
      $finish(1);
    end

    a = 4'h3; #1;
    if (y_repeat_bitslice !== 4'hC) begin
      $display("FAIL repeat_bitslice case1: got=%h expect=C", y_repeat_bitslice);
      $finish(1);
    end
    if (y_repeat_partselect !== 4'hC) begin
      $display("FAIL repeat_partselect case1: got=%h expect=C", y_repeat_partselect);
      $finish(1);
    end

    a = 4'h9; #1;
    if (y_repeat_bitslice !== 4'h6) begin
      $display("FAIL repeat_bitslice case2: got=%h expect=6", y_repeat_bitslice);
      $finish(1);
    end
    if (y_repeat_partselect !== 4'h6) begin
      $display("FAIL repeat_partselect case2: got=%h expect=6", y_repeat_partselect);
      $finish(1);
    end

    $display("PASS");
    $finish(0);
  end
endmodule
