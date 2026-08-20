// arch#892 regression: adjacent prefix operators, behavioral check under
// Icarus. Values are worked out by hand from the ARCH semantics (see the
// Verilator harness for the identical vectors):
//
//   a=4'hA (signed 4-bit = -6), p=1:
//     ~(~a)                 = 4'hA
//     -(-sext<6>(signed a)) = -6  -> 6'b111010
//     not(not p)            = 1
//     &(&a) = &a            = 0   (a != 4'hF)
//     |(|a) = |a            = 1
//     ^(^a) = ^a            = 0   (two set bits)
//
//   a=4'h7 (signed 4-bit = 7), p=0:
//     ~(~a)                 = 4'h7
//     -(-sext<6>(signed a)) = 7   -> 6'b000111
//     not(not p)            = 0
//     &a                    = 0
//     |a                    = 1
//     ^a                    = 1   (three set bits)
module tb;
  logic [3:0] a;
  logic p, q;
  logic [3:0] y_notnot;
  logic signed [5:0] y_negneg;
  logic y_lognot, y_redand, y_redor, y_redxor;

  AdjacentUnary dut(
    .a(a), .p(p), .q(q),
    .y_notnot(y_notnot), .y_negneg(y_negneg), .y_lognot(y_lognot),
    .y_redand(y_redand), .y_redor(y_redor), .y_redxor(y_redxor)
  );

  task check(input signed [7:0] got, input signed [7:0] want, input string label);
    if (got !== want) begin
      $display("FAIL %0s: got=%0d expect=%0d", label, got, want);
      $finish(1);
    end
  endtask

  initial begin
    a = 4'hA; p = 1'b1; q = 1'b0; #1;
    check(y_notnot, 8'sd10, "notnot case1");
    check(y_negneg, -8'sd6, "negneg case1");
    check(y_lognot, 8'sd1,  "lognot case1");
    check(y_redand, 8'sd0,  "redand case1");
    check(y_redor,  8'sd1,  "redor case1");
    check(y_redxor, 8'sd0,  "redxor case1");

    a = 4'h7; p = 1'b0; q = 1'b1; #1;
    check(y_notnot, 8'sd7,  "notnot case2");
    check(y_negneg, 8'sd7,  "negneg case2");
    check(y_lognot, 8'sd0,  "lognot case2");
    check(y_redand, 8'sd0,  "redand case2");
    check(y_redor,  8'sd1,  "redor case2");
    check(y_redxor, 8'sd1,  "redxor case2");

    $display("PASS");
    $finish(0);
  end
endmodule
