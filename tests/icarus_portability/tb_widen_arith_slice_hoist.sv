// Regression: slices into the widened high bits of `a + b` / `a * b`
// behavioral check under Icarus. Pre-fix the hoist temp was 8 bits and
// every high-bit read returned X.
module tb;
  logic [7:0] a, b;
  logic add_carry, mul_top;
  logic [3:0] add_ps;
  logic [7:0] mul_hi;

  WidenArithSliceHoist dut(.a(a), .b(b), .add_carry(add_carry),
                           .add_ps(add_ps), .mul_hi(mul_hi), .mul_top(mul_top));

  task check(input [7:0] va, vb, input exp_carry, input [3:0] exp_ps,
             input [7:0] exp_hi, input exp_top);
    begin
      a = va; b = vb; #1;
      if (add_carry !== exp_carry || add_ps !== exp_ps ||
          mul_hi !== exp_hi || mul_top !== exp_top) begin
        $display("FAIL a=%0d b=%0d: carry=%b(exp %b) ps=%0d(exp %0d) hi=%0d(exp %0d) top=%b(exp %b)",
                 va, vb, add_carry, exp_carry, add_ps, exp_ps, mul_hi, exp_hi, mul_top, exp_top);
        $finish(1);
      end
    end
  endtask

  initial begin
    // a+b=300=9'b1_0010_1100 (carry 1), a*b=20000=16'h4E20 (hi 0x4E, top 0)
    check(8'd200, 8'd100, 1'b1, 4'd9,  8'd78,  1'b0);
    // a+b=510=9'b1_1111_1110 (carry 1), a*b=65025=16'hFE01 (hi 0xFE, top 1)
    check(8'd255, 8'd255, 1'b1, 4'd15, 8'd254, 1'b1);
    // a+b=200=9'b0_1100_1000 (carry 0, [8:5]=0110=6), a*b=10000=16'h2710 (hi 0x27)
    check(8'd100, 8'd100, 1'b0, 4'd6,  8'd39,  1'b0);
    // all zero
    check(8'd0,   8'd0,   1'b0, 4'd0,  8'd0,   1'b0);
    $display("PASS");
    $finish(0);
  end
endmodule
