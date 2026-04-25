// Regression test for the sim Bool `~` masking bug.
//
// Pre-fix: `wire b: Bool;` had no width registered in the sim
// codegen's `widths` map (build_widths only handled RegDecl /
// LetBinding). So `~b` in cpp_expr's BitNot arm took the "wider
// type" branch and emitted `(~(uint8_t)1)` = 0xFE, never == 0.
// `if ~b == false` therefore never entered its body in arch sim
// (iverilog masked correctly so existing CVDP tests passed).
//
// Post-fix: build_widths registers wires too. `~b` correctly emits
// `(uint8_t)(!(...))` so `~b == false` evaluates the way the design
// intended.
module sim_bool_not_regression (
  input logic clk,
  input logic rst,
  input logic a,
  input logic b,
  output logic not_a_eq_false,
  output logic [7:0] not_b_neg_a
);

  // (~a == false)
  // first idx i in 0..3 where ~b == false then ~a[i]
  logic flag_w;
  assign flag_w = ~a;
  assign not_a_eq_false = ~a == 1'b0;
  // Loop pattern from cache_mshr's alloc_idx priority encoder
  always_comb begin
    not_b_neg_a = 0;
    for (int i = 0; i <= 3; i++) begin
      if (~b == 1'b0) begin
        not_b_neg_a = 8'(not_b_neg_a + 1);
      end
    end
  end

endmodule

