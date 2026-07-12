// Pinned characterization testbench for int.to_bf16() on the built-SV
// backend (issue #629). Calls the emitted synthesizable helpers directly
// (arch_f32_to_bf16(arch_i64_to_f32(i))) -- the exact lowering `arch build`
// uses for `<SInt<N>|UInt<N>>.to_bf16()` -- and locks the f32-routed result
// for the same witness pinned on the arch-sim backend in
// tests/fp_v1/tb_int_to_bf16.cpp. int.to_bf16() is DECLARED as f32-routed
// (see doc/ARCH_HDL_Specification.md §3.8 "Rounding convention"); a change to
// correctly-rounded semantics must trip this test and update that decision.
module tb;
  logic [15:0] h;

  initial begin
    // Witness (issue #629): i = 2^24 + 2^16 + 1 = 16842753. f32-routed
    // int->bf16 gives 0x4b80; correctly-rounded bf16 would be 0x4b81 (1 ULP
    // away -- the f32 step ties-to-even onto the exact bf16 midpoint).
    h = arch_f32_to_bf16(arch_i64_to_f32(64'sd16842753));
    if (h !== 16'h4b80) begin
      $display("ARCH_INT_TO_BF16_WITNESS: FAIL i=16842753 got=%h want=4b80", h);
      $fatal(1);
    end

    // Exact case below 2^24: no double-rounding hazard -- f32-routed and
    // correctly-rounded bf16 agree bit-for-bit.
    h = arch_f32_to_bf16(arch_i64_to_f32(64'sd1000));
    if (h !== 16'h447a) begin
      $display("ARCH_INT_TO_BF16_WITNESS: FAIL i=1000 got=%h want=447a", h);
      $fatal(1);
    end

    $display("ARCH_INT_TO_BF16_WITNESS: ALL PASS");
    $finish;
  end
endmodule
