// SystemVerilog twin of `tests/fp_v1/tb_scaled_quant.cpp`.
//
// Drives the SAME vectors into the SAME design and prints the SAME
// transcript, so `fp_scaled_quant_sv_matches_sim` can byte-compare the two.
// That comparison is the phase-2 gate (§8 of arch#884): `arch build` and
// `arch sim` emit the block conversion from one descriptor in
// `src/fp_block.rs`, and this is what proves the two renderings agree on
// values rather than merely on shape.
//
// Blocks print as 32-bit words LSB-first because a 72-bit `%h` has no
// portable C++ counterpart. Keep the vector table below in lock-step with
// the C++ one — same order, same hex.
`timescale 1ns/1ps

module tb;
  localparam int NC = 9;

  logic [7:0][31:0] v;
  logic [3:0][31:0] v4;
  logic [39:0]      q4_floor, q4_ceil;
  logic [55:0]      q6;
  logic [71:0]      q8;
  logic [23:0]      q4s;
  logic [7:0][31:0] back4, back6, back8;

  ScaledQuant dut (
    .v(v), .v4(v4),
    .q4_floor(q4_floor), .q4_ceil(q4_ceil), .q6(q6), .q8(q8), .q4s(q4s),
    .back4(back4), .back6(back6), .back8(back8)
  );

  string       names [NC];
  logic [31:0] vecs  [NC][8];
  logic [31:0] vec4s [NC][4];

  // Print one signal as `name` followed by `words` 32-bit words, LSB first.
  // `val` is passed at the widest block size (72 bits) and sliced here, so
  // one task serves every block width.
  task automatic show(input string name, input logic [71:0] val, input int words);
    $write("%s", name);
    for (int w = 0; w < words; w++) $write(" %h", val[w*32 +: 32]);
    $write("\n");
  endtask

  task automatic show_vec(input string name, input logic [7:0][31:0] val, input int n);
    $write("%s", name);
    for (int i = 0; i < n; i++) $write(" %h", val[i]);
    $write("\n");
  endtask

  initial begin
    // Powers of two throughout: floor_pow2 and ceil_pow2 must agree.
    names[0] = "pow2";
    vecs[0]  = '{32'h3F800000, 32'hC0000000, 32'h40800000, 32'h3F000000,
                 32'h41000000, 32'hBE800000, 32'h00000000, 32'h40000000};
    vec4s[0] = '{32'h3F800000, 32'h40000000, 32'h40800000, 32'h3F000000};
    // Max 6.0 is not a power of two: the two scale policies must differ.
    names[1] = "nonpow2";
    vecs[1]  = '{32'h3F800000, 32'hC0000000, 32'h40400000, 32'h3F000000,
                 32'h40C00000, 32'hBE800000, 32'h00000000, 32'h40800000};
    vec4s[1] = '{32'h40C00000, 32'h40400000, 32'h3FC00000, 32'h3F400000};
    // All zero: minimum scale 0x00 and zero elements, NOT a NaN scale.
    names[2] = "zeros";
    vecs[2]  = '{32'h0, 32'h0, 32'h0, 32'h0, 32'h0, 32'h0, 32'h0, 32'h0};
    vec4s[2] = '{32'h0, 32'h0, 32'h0, 32'h0};
    // Signed zeros only — still an all-zero block by magnitude.
    names[3] = "negzero";
    vecs[3]  = '{32'h80000000, 32'h0, 32'h80000000, 32'h0,
                 32'h0, 32'h80000000, 32'h0, 32'h0};
    vec4s[3] = '{32'h80000000, 32'h0, 32'h0, 32'h0};
    // One NaN anywhere forces the NaN scale 0xFF; element bits are
    // don't-care and must still agree bit for bit across backends.
    names[4] = "nan";
    vecs[4]  = '{32'h3F800000, 32'h7FC00000, 32'h40400000, 32'h3F000000,
                 32'h40C00000, 32'hBE800000, 32'h00000000, 32'h40800000};
    vec4s[4] = '{32'h3F800000, 32'h7FC00000, 32'h3F800000, 32'h3F800000};
    // Inf likewise: `arch_f32_to_e8m0` maps every non-finite to 0xFF.
    names[5] = "inf";
    vecs[5]  = '{32'h3F800000, 32'h7F800000, 32'h40400000, 32'h3F000000,
                 32'h40C00000, 32'hBE800000, 32'h00000000, 32'h40800000};
    vec4s[5] = '{32'hFF800000, 32'h3F800000, 32'h3F800000, 32'h3F800000};
    // Subnormals: the shared scale underflows and clamps at 0x00.
    names[6] = "subnormal";
    vecs[6]  = '{32'h00000001, 32'h00000002, 32'h007FFFFF, 32'h00400000,
                 32'h00000000, 32'h00000003, 32'h00800000, 32'h00000001};
    vec4s[6] = '{32'h00000001, 32'h00000002, 32'h00000003, 32'h00000004};
    // Huge magnitudes: the scale saturates near the top of E8M0 and the
    // dequantized product is the saturation case (decision #5).
    names[7] = "huge";
    vecs[7]  = '{32'h7F7FFFFF, 32'h7F000000, 32'h3F800000, 32'h00000000,
                 32'hFF7FFFFF, 32'h40000000, 32'h40800000, 32'h41000000};
    vec4s[7] = '{32'h7F7FFFFF, 32'h3F800000, 32'h00000000, 32'h40000000};
    // Wide dynamic range inside one block: the small elements must
    // quantize to zero rather than to a wrapped code.
    names[8] = "range";
    vecs[8]  = '{32'h44800000, 32'h3A83126F, 32'hC4000000, 32'h00000000,
                 32'h3F800000, 32'hB8D1B717, 32'h43800000, 32'h40000000};
    vec4s[8] = '{32'h44800000, 32'h3A83126F, 32'h3F800000, 32'h00000000};

    for (int c = 0; c < NC; c++) begin
      for (int i = 0; i < 8; i++) v[i]  = vecs[c][i];
      for (int i = 0; i < 4; i++) v4[i] = vec4s[c][i];
      #1;
      $display("== %s", names[c]);
      show("q4f", {32'b0, q4_floor}, 2);
      show("q4c", {32'b0, q4_ceil},  2);
      show("q6 ", {16'b0, q6},       2);
      show("q8 ", q8,                3);
      show("q4s", {48'b0, q4s},      1);
      show_vec("b4 ", back4, 8);
      show_vec("b6 ", back6, 8);
      show_vec("b8 ", back8, 8);
    end
    $finish;
  end
endmodule
