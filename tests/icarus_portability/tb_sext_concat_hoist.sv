// arch#827 B2 regression: `.sext<N>()` on a `Concat` receiver behavioral
// check under Icarus. Pre-fix this design was a straight syntax error on
// iverilog (`{a, b}[12-1]` — a select applied to an unparenthesized brace
// literal); this confirms the hoisted form also computes the values
// ARCH's own semantics dictate.
module tb;
  logic [5:0] a, b;
  logic signed [31:0] y;

  SextConcatHoist dut(.a(a), .b(b), .y(y));

  task check(input [5:0] va, vb, input signed [31:0] exp);
    begin
      a = va; b = vb; #1;
      if (y !== exp) begin
        $display("FAIL a=%0d b=%0d: y=%0d expect=%0d", va, vb, y, exp);
        $finish(1);
      end
    end
  endtask

  initial begin
    check(6'd0,  6'd1,  32'sd1);
    check(6'h3F, 6'h3F, -32'sd1);
    check(6'h20, 6'd0,  -32'sd2048);
    check(6'h1F, 6'h3F, 32'sd2047);

    $display("PASS");
    $finish(0);
  end
endmodule
