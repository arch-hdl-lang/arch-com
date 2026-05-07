// Comprehensive operator precedence test.
// Verifies the compiler emits correct SV parenthesization for all operator combinations.
//
// Key concern: ARCH treats bitwise ops as tighter than comparisons,
// but SV treats comparisons as tighter than bitwise.  The compiler
// collapses them to the same precedence tier and adds parens when mixed.
//
// Note: ARCH reserves `<=` for seq assignment; use `>=` with swapped
// operands or `!(a > b)` for less-than-or-equal comparisons.
module OperatorPrecedenceTest #(
  parameter int W = 8
) (
  input logic [W-1:0] a,
  input logic [W-1:0] b,
  input logic [W-1:0] c,
  input logic [W-1:0] d,
  input logic [W-1:0] e,
  input logic [W-1:0] f,
  input logic sel
);

  // ── 1. Same-operator chains (no parens needed) ──
  logic [W-1:0] chain_and;
  assign chain_and = a & b & c;
  logic [W-1:0] chain_or;
  assign chain_or = a | b | c;
  logic [W-1:0] chain_xor;
  assign chain_xor = a ^ b ^ c;
  logic [W-1:0] chain_add;
  assign chain_add = W'((W'(a + b)) + c);
  // ── 2. Mixed comparison + bitwise (parens required) ──
  //    ARCH: bitwise tighter than comparison
  //    SV:   comparison tighter than bitwise
  //    Compiler must emit parens around comparisons when mixed with bitwise.
  logic cmp_and_eq;
  assign cmp_and_eq = (a == b) & (c == d);
  logic cmp_or_neq;
  assign cmp_or_neq = (a != b) | (c != d);
  logic cmp_and_lt_gt;
  assign cmp_and_lt_gt = (a < b) & (c > d);
  logic cmp_or_gte;
  assign cmp_or_gte = (a >= b) | (d >= c);
  // Multiple comparisons with same bitwise op
  logic cmp_and_three;
  assign cmp_and_three = (a == b) & (c == d) & (e == f);
  // ── 3. Mixed bitwise operators (parens for different ops) ──
  logic [W-1:0] mix_and_or;
  assign mix_and_or = (a & b) | (c & d);
  logic [W-1:0] mix_or_xor;
  assign mix_or_xor = (a | b) ^ (c | d);
  logic [W-1:0] mix_xor_and;
  assign mix_xor_and = (a ^ b) & (c ^ d);
  logic [W-1:0] mix_or_and;
  assign mix_or_and = (a | b) & (c | d);
  logic [W-1:0] mix_xor_or;
  assign mix_xor_or = (a ^ b) | (c ^ d);
  logic [W-1:0] mix_and_xor;
  assign mix_and_xor = (a & b) ^ (c & d);
  // ── 4. Arithmetic vs comparison (standard precedence) ──
  logic arith_cmp_add;
  assign arith_cmp_add = (W + 1)'($unsigned(a)) + (W + 1)'($unsigned(b)) == (W + 1)'($unsigned(c));
  logic arith_cmp_mul;
  assign arith_cmp_mul = (2 * W)'($unsigned(a)) * (2 * W)'($unsigned(b)) < (2 * W)'($unsigned(c)) + (2 * W)'($unsigned(d));
  // ── 5. Shift operators ──
  logic [W-1:0] shift_left;
  assign shift_left = a << b;
  logic [W-1:0] shift_right;
  assign shift_right = a >> b;
  logic [W-1:0] shift_plus;
  assign shift_plus = ($bits(a << b) > W ? $bits(a << b) : W)'((a << b) + c);
  // ── 6. Logical operators (and, or) ──
  logic logic_and_or;
  assign logic_and_or = a == b && c == d || e == f;
  logic logic_or_and;
  assign logic_or_and = a == b || c == d && e == f;
  // ── 7. Unary operators ──
  logic [W-1:0] unary_not_and;
  assign unary_not_and = ~a & b;
  logic [W-1:0] unary_not_group;
  assign unary_not_group = ~(a & b);
  logic unary_reduct_and;
  assign unary_reduct_and = &a;
  logic unary_reduct_or;
  assign unary_reduct_or = |a;
  logic unary_reduct_xor;
  assign unary_reduct_xor = ^a;
  // ── 8. Ternary in expressions ──
  logic [W-1:0] tern_simple;
  assign tern_simple = sel ? a : b;
  logic [W-1:0] tern_in_add;
  assign tern_in_add = ($bits(sel ? a : b) > W ? $bits(sel ? a : b) : W)'((sel ? a : b) + c);
  logic [W-1:0] tern_cmp_cond;
  assign tern_cmp_cond = a == b ? c : d;
  // ── 9. Method calls with operators ──
  logic meth_trunc_eq;
  assign meth_trunc_eq = W'((W + 1)'($unsigned(a)) + (W + 1)'($unsigned(b))) == c;
  logic [W-1:0] meth_zext_add;
  assign meth_zext_add = W'(a + b);
  // ── 10. Wrapping operators mixed with regular ──
  logic wrap_add_eq;
  assign wrap_add_eq = W'(a + b) == c;
  logic [W-1:0] wrap_chain;
  assign wrap_chain = W'((W'(a + b)) + c);
  logic wrap_sub_eq;
  assign wrap_sub_eq = W'(a - b) == W'(c - d);
  // ── 11. Complex nested expressions ──
  logic nested_cmp_bitwise;
  assign nested_cmp_bitwise = ((a == b) & (c == d)) | ((e == f) & (a != c));
  logic [W-1:0] nested_arith_shift;
  assign nested_arith_shift = W'((W + 1)'($unsigned(a)) + (W + 1)'($unsigned(b))) << 2;
  logic [W-1:0] nested_multi_level;
  assign nested_multi_level = ((a & b) | (c & d)) ^ ((e & f) | (a & c));
  // ── 12. Comparison chains with bitwise ──
  logic cmp_neq_and;
  assign cmp_neq_and = (a != b) & (c != d);
  logic cmp_lt_or;
  assign cmp_lt_or = (a < b) | (c < d);
  logic cmp_gt_xor;
  assign cmp_gt_xor = (a > b) ^ (c > d);
  // ── 13. Arithmetic in shift amount ──
  logic [W-1:0] shift_arith_amt;
  assign shift_arith_amt = a << W'(b + c);
  // ── 14. Mixed everything ──
  logic kitchen_sink;
  assign kitchen_sink = (W'(a + b) == c) & ((d > e) | (f != a));

endmodule

