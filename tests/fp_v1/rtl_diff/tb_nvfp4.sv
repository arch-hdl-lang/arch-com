// SystemVerilog twin of `tests/fp_v1/tb_nvfp4.cpp`.
//
// Drives the SAME vectors into the SAME design and prints the SAME
// transcript, so `fp_nvfp4_sv_matches_sim` can byte-compare the two. That
// comparison is the §8 gate for phase 5b (arch#905): the UE4M3 quantizer's
// two constant-threshold ladders are rendered once as SystemVerilog and once
// as C++ from one descriptor in `src/fp_block.rs`, and this is what proves
// the two renderings agree on values rather than merely on shape.
//
// Keep the vector table below in lock-step with the C++ one — same order,
// same hex.
`timescale 1ns/1ps

module tb;
  localparam int NC = 10;

  logic [15:0][31:0] v;
  logic  [7:0][31:0] v8;
  logic [71:0]       q_def, q_exact, q_floor, q_ceil, q_mx, q8;
  logic [15:0][31:0] back, back_mx;
  logic  [7:0][31:0] back8;
  logic [31:0]       dot, dot_x, dot8;

  Nvfp4Quant dut (
    .v(v), .v8(v8),
    .q_def(q_def), .q_exact(q_exact), .q_floor(q_floor), .q_ceil(q_ceil),
    .q_mx(q_mx), .q8(q8),
    .back(back), .back_mx(back_mx), .back8(back8),
    .dot(dot), .dot_x(dot_x), .dot8(dot8)
  );

  string       names [NC];
  logic [31:0] vecs  [NC][16];
  logic [31:0] vec8s [NC][8];

  task automatic show(input string name, input logic [71:0] val, input int words);
    $write("%s", name);
    for (int w = 0; w < words; w++) $write(" %h", val[w*32 +: 32]);
    $write("\n");
  endtask

  task automatic show_vec(input string name, input logic [15:0][31:0] val, input int n);
    $write("%s", name);
    for (int i = 0; i < n; i++) $write(" %h", val[i]);
    $write("\n");
  endtask

  initial begin
    // Block maximum 8.0 is a power of two.
    names[0] = "pow2";
    vecs[0]  = '{32'h3F800000, 32'hC0000000, 32'h40800000, 32'h3F000000,
                 32'h41000000, 32'hBE800000, 32'h00000000, 32'h40000000,
                 32'h3F800000, 32'h40800000, 32'hC1000000, 32'h3F000000,
                 32'h40000000, 32'h00000000, 32'h3E800000, 32'h40800000};
    vec8s[0] = '{32'h3F800000, 32'h40000000, 32'h40800000, 32'h3F000000,
                 32'h41000000, 32'h3F800000, 32'h40400000, 32'h00000000};
    // Maximum 5.0, so amax/elem_max is also not a power of two: `exact`
    // vs `floor_pow2`. (6.0 would be vacuous — 6/6 is exactly 1.0.)
    names[1] = "nonpow2";
    vecs[1]  = '{32'h3F800000, 32'hC0000000, 32'h40400000, 32'h3F000000,
                 32'h40A00000, 32'hBE800000, 32'h00000000, 32'h40800000,
                 32'h3FC00000, 32'h3F400000, 32'hC0400000, 32'h40000000,
                 32'h3F800000, 32'h40A00000, 32'h3E800000, 32'h3F000000};
    vec8s[1] = '{32'h40A00000, 32'h40400000, 32'h3FC00000, 32'h3F400000,
                 32'h40000000, 32'h3F800000, 32'hC0A00000, 32'h3F000000};
    // All zero: scale code 0x00, a genuine zero for UE4M3.
    names[2] = "zeros";
    vecs[2]  = '{32'h0, 32'h0, 32'h0, 32'h0, 32'h0, 32'h0, 32'h0, 32'h0,
                 32'h0, 32'h0, 32'h0, 32'h0, 32'h0, 32'h0, 32'h0, 32'h0};
    vec8s[2] = '{32'h0, 32'h0, 32'h0, 32'h0, 32'h0, 32'h0, 32'h0, 32'h0};
    // Signed zeros only.
    names[3] = "negzero";
    vecs[3]  = '{32'h80000000, 32'h0, 32'h80000000, 32'h0,
                 32'h0, 32'h80000000, 32'h0, 32'h0,
                 32'h80000000, 32'h0, 32'h0, 32'h0,
                 32'h80000000, 32'h0, 32'h0, 32'h0};
    vec8s[3] = '{32'h80000000, 32'h0, 32'h0, 32'h0,
                 32'h80000000, 32'h0, 32'h0, 32'h0};
    // NaN forces the NaN scale 0x7F, NOT 0xFF.
    names[4] = "nan";
    vecs[4]  = '{32'h3F800000, 32'h7FC00000, 32'h40400000, 32'h3F000000,
                 32'h40C00000, 32'hBE800000, 32'h00000000, 32'h40800000,
                 32'h3F800000, 32'h40000000, 32'h3F000000, 32'h40400000,
                 32'h3F800000, 32'h40800000, 32'h3E800000, 32'h3F000000};
    vec8s[4] = '{32'h3F800000, 32'h7FC00000, 32'h3F800000, 32'h3F800000,
                 32'h40000000, 32'h3F800000, 32'h3F800000, 32'h3F800000};
    // Inf likewise.
    names[5] = "inf";
    vecs[5]  = '{32'h3F800000, 32'h7F800000, 32'h40400000, 32'h3F000000,
                 32'h40C00000, 32'hBE800000, 32'h00000000, 32'h40800000,
                 32'h3F800000, 32'h40000000, 32'h3F000000, 32'h40400000,
                 32'h3F800000, 32'h40800000, 32'h3E800000, 32'h3F000000};
    vec8s[5] = '{32'hFF800000, 32'h3F800000, 32'h3F800000, 32'h3F800000,
                 32'h40000000, 32'h3F800000, 32'h3F800000, 32'h3F800000};
    // Subnormals: the scale underflows and clamps at 0x01, not 0x00.
    names[6] = "subnormal";
    vecs[6]  = '{32'h00000001, 32'h00000002, 32'h007FFFFF, 32'h00400000,
                 32'h00000000, 32'h00000003, 32'h00800000, 32'h00000001,
                 32'h00000002, 32'h00000001, 32'h00000000, 32'h00000004,
                 32'h00000001, 32'h00200000, 32'h00000000, 32'h00000001};
    vec8s[6] = '{32'h00000001, 32'h00000002, 32'h00000003, 32'h00000004,
                 32'h00000001, 32'h00800000, 32'h00000002, 32'h00000001};
    // Huge magnitudes: the scale saturates at the top UE4M3 code.
    names[7] = "huge";
    vecs[7]  = '{32'h7F7FFFFF, 32'h7F000000, 32'h3F800000, 32'h00000000,
                 32'hFF7FFFFF, 32'h40000000, 32'h40800000, 32'h41000000,
                 32'h7F000000, 32'h3F800000, 32'h00000000, 32'h40000000,
                 32'h7F7FFFFF, 32'h40800000, 32'h3F000000, 32'h41000000};
    vec8s[7] = '{32'h7F7FFFFF, 32'h3F800000, 32'h00000000, 32'h40000000,
                 32'h7F000000, 32'h40800000, 32'h3F800000, 32'h41000000};
    // Wide dynamic range inside one block.
    names[8] = "range";
    vecs[8]  = '{32'h44800000, 32'h3A83126F, 32'hC4000000, 32'h00000000,
                 32'h3F800000, 32'hB8D1B717, 32'h43800000, 32'h40000000,
                 32'h44800000, 32'h3A83126F, 32'h3F800000, 32'h00000000,
                 32'hC4000000, 32'h43800000, 32'h40000000, 32'h3F800000};
    vec8s[8] = '{32'h44800000, 32'h3A83126F, 32'h3F800000, 32'h00000000,
                 32'hC4000000, 32'h43800000, 32'h40000000, 32'h3F800000};
    // E4M3 elements straddling their own top code (448 / 464 / 480) — the
    // only vector that fires the overflow rung.
    names[9] = "e4m3_top";
    vecs[9]  = '{32'h43E00000, 32'h43F00000, 32'h43600000, 32'h3F800000,
                 32'h43E00000, 32'hC3F00000, 32'h40000000, 32'h43600000,
                 32'h43E00000, 32'h43F00000, 32'h3F800000, 32'h40800000,
                 32'h43600000, 32'h43E00000, 32'h40000000, 32'h3F000000};
    vec8s[9] = '{32'h43E00000, 32'h43F00000, 32'h43680000, 32'h43600000,
                 32'hC3E00000, 32'hC3F00000, 32'h3F800000, 32'h40000000};

    for (int c = 0; c < NC; c++) begin
      for (int i = 0; i < 16; i++) v[i]  = vecs[c][i];
      for (int i = 0; i < 8;  i++) v8[i] = vec8s[c][i];
      #1;
      $display("== %s", names[c]);
      show("qd ", q_def,   3);
      show("qe ", q_exact, 3);
      show("qf ", q_floor, 3);
      show("qc ", q_ceil,  3);
      show("qmx", q_mx,    3);
      show("q8 ", q8,      3);
      show_vec("bk ", back,    16);
      show_vec("bmx", back_mx, 16);
      $write("bk8");
      for (int i = 0; i < 8; i++) $write(" %h", back8[i]);
      $write("\n");
      $display("dot %h %h %h", dot, dot_x, dot8);
    end
    $finish;
  end
endmodule
