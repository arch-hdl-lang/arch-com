// Comprehensive operator x data-type correctness test.
// Verifies the compiler correctly handles width widening, signedness,
// type checking, and explicit casts for all operator/type combinations.
//
// IEEE 1800-2012 section 11.6 governs arithmetic result widths.
// ARCH enforces explicit casts for all conversions.
module OperatorTypeTest (
  input logic [7:0] a8,
  input logic [7:0] b8,
  input logic [3:0] c4,
  input logic [2:0] d3,
  input logic signed [7:0] sa,
  input logic signed [7:0] sb,
  input logic bool_a,
  input logic bool_b
);

  // ── 1. Arithmetic widening (IEEE 1800-2012 section 11.6) ──
  // Addition: max(W(a),W(b)) + 1
  logic [8:0] arith_add_same;
  assign arith_add_same = a8 + b8;
  // 8+8 -> 9
  logic [8:0] arith_add_mixed;
  assign arith_add_mixed = a8 + c4;
  // max(8,4)+1 = 9
  logic [8:0] arith_sub_same;
  assign arith_sub_same = a8 - b8;
  // 8-8 -> 9
  // Multiplication: W(a) + W(b)
  logic [15:0] arith_mul_same;
  assign arith_mul_same = a8 * b8;
  // 8*8 -> 16
  logic [11:0] arith_mul_mixed;
  assign arith_mul_mixed = a8 * c4;
  // 8+4 = 12
  // ── 2. Wrapping operators ──
  // Same width: result = max(W(a),W(b))
  logic [7:0] wrap_add_same;
  assign wrap_add_same = 8'(a8 + b8);
  // max(8,8) = 8
  logic [7:0] wrap_sub_same;
  assign wrap_sub_same = 8'(a8 - b8);
  // max(8,8) = 8
  logic [7:0] wrap_mul_same;
  assign wrap_mul_same = 8'(a8 * b8);
  // max(8,8) = 8
  // Mixed width: result = max(W(a),W(b))
  logic [7:0] wrap_add_mixed;
  assign wrap_add_mixed = (8 > 4 ? 8 : 4)'(a8 + c4);
  // max(8,4) = 8
  logic [7:0] wrap_add_comm;
  assign wrap_add_comm = (4 > 8 ? 4 : 8)'(c4 + a8);
  // max(4,8) = 8 (commutative)
  logic [7:0] wrap_sub_mixed;
  assign wrap_sub_mixed = (8 > 4 ? 8 : 4)'(a8 - c4);
  // max(8,4) = 8
  logic [7:0] wrap_mul_mixed;
  assign wrap_mul_mixed = (8 > 4 ? 8 : 4)'(a8 * c4);
  // max(8,4) = 8
  // ── 3. Signed arithmetic ──
  logic signed [8:0] sint_add;
  assign sint_add = sa + sb;
  // SInt<8>+SInt<8> -> SInt<9>
  logic signed [15:0] sint_mul;
  assign sint_mul = sa * sb;
  // SInt<8>*SInt<8> -> SInt<16>
  logic signed [8:0] sint_cast_add;
  assign sint_cast_add = $signed(a8) + $signed(b8);
  // cast then add
  // ── 4. Comparison results (always Bool) ──
  logic cmp_eq;
  assign cmp_eq = a8 == b8;
  logic cmp_lt;
  assign cmp_lt = a8 < b8;
  logic cmp_ne;
  assign cmp_ne = a8 != b8;
  logic cmp_ge;
  assign cmp_ge = a8 >= b8;
  logic cmp_gt;
  assign cmp_gt = a8 > b8;
  // ── 5. Bitwise operators (same-width, no widening) ──
  logic [7:0] bit_and;
  assign bit_and = a8 & b8;
  logic [7:0] bit_or;
  assign bit_or = a8 | b8;
  logic [7:0] bit_xor;
  assign bit_xor = a8 ^ b8;
  logic [7:0] bit_not;
  assign bit_not = ~a8;
  // ── 6. Shift operators (non-widening) ──
  logic [7:0] shift_left_var;
  assign shift_left_var = a8 << d3;
  // result width = LHS width
  logic [7:0] shift_right_var;
  assign shift_right_var = a8 >> d3;
  logic [7:0] shift_left_lit;
  assign shift_left_lit = a8 << 4;
  // literal shift amount
  // ── 7. Bool operations ──
  logic bool_and;
  assign bool_and = bool_a & bool_b;
  logic bool_or;
  assign bool_or = bool_a | bool_b;
  logic bool_not;
  assign bool_not = ~bool_a;
  logic bool_xor;
  assign bool_xor = bool_a ^ bool_b;
  // ── 8. Reduction operators ──
  logic red_and;
  assign red_and = &a8;
  // reduction AND
  logic red_or;
  assign red_or = |a8;
  // reduction OR
  logic red_xor;
  assign red_xor = ^a8;
  // reduction XOR
  // ── 9. Width casts in expressions ──
  logic [7:0] trunc_add;
  assign trunc_add = 8'(a8 + b8);
  // 9->8
  logic [7:0] zext_small;
  assign zext_small = 8'($unsigned(c4));
  // 4->8
  logic [16:0] zext_add;
  assign zext_add = 16'($unsigned(a8)) + 16'($unsigned(b8));
  // 16+16->17
  logic signed [15:0] sext_test;
  assign sext_test = {{(16-$bits(sa)){sa[$bits(sa)-1]}}, sa};
  // SInt<8>->SInt<16>
  // ── 10. Concatenation ──
  logic [15:0] concat_same;
  assign concat_same = {a8, b8};
  // 8+8 = 16
  logic [7:0] concat_diff;
  assign concat_diff = {c4, c4};
  // 4+4 = 8
  logic [7:0] concat_bool;
  assign concat_bool = {bool_a, a8[6:0]};
  // 1+7 = 8
  // ── 11. onehot ──
  logic [7:0] onehot_test;
  assign onehot_test = (1 << d3);

endmodule

// 1 << d3
