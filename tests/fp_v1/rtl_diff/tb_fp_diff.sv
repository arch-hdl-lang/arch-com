// Differential testbench for the synthesizable FP helpers (doc/plan_fp_types.md
// §8.2). The `arch_*` helper functions come from the `arch build` output that is
// verilated alongside this file; the reference is host IEEE-754 via DPI-C
// (dpi_ref.cpp), i.e. the same semantics the arch-sim backend implements.
// Checks bit-equality of every op / compare / conversion / BF16 wrapper over the
// §8.2 corner vectors plus randomized and cancellation-prone pairs.
module tb;
  import "DPI-C" function int unsigned dpi_mul(input int unsigned a, input int unsigned b);
  import "DPI-C" function int unsigned dpi_add(input int unsigned a, input int unsigned b);
  import "DPI-C" function int unsigned dpi_sub(input int unsigned a, input int unsigned b);
  import "DPI-C" function int unsigned dpi_fma(input int unsigned a, input int unsigned b, input int unsigned c);
  import "DPI-C" function int dpi_eq(input int unsigned a, input int unsigned b);
  import "DPI-C" function int dpi_ne(input int unsigned a, input int unsigned b);
  import "DPI-C" function int dpi_lt(input int unsigned a, input int unsigned b);
  import "DPI-C" function int dpi_le(input int unsigned a, input int unsigned b);
  import "DPI-C" function int dpi_gt(input int unsigned a, input int unsigned b);
  import "DPI-C" function int dpi_ge(input int unsigned a, input int unsigned b);
  import "DPI-C" function int unsigned dpi_s2f(input longint v);
  import "DPI-C" function int unsigned dpi_u2f(input longint unsigned v);
  import "DPI-C" function longint dpi_f2s(input int unsigned x, input int n);
  import "DPI-C" function longint unsigned dpi_f2u(input int unsigned x, input int n);
  import "DPI-C" function int unsigned dpi_bf16_add(input shortint unsigned a, input shortint unsigned b);
  import "DPI-C" function int unsigned dpi_bf16_sub(input shortint unsigned a, input shortint unsigned b);
  import "DPI-C" function int unsigned dpi_bf16_mul(input shortint unsigned a, input shortint unsigned b);
  import "DPI-C" function int unsigned dpi_bf16_fma(input shortint unsigned a, input shortint unsigned b, input shortint unsigned c);

  integer errors = 0;
  integer i, j;
  logic [31:0] a, b, c, g, r;

  localparam int NC = 24;
  logic [31:0] cv [0:NC-1];
  initial begin
    cv[0]=32'h00000000; cv[1]=32'h80000000;
    cv[2]=32'h3F800000; cv[3]=32'hBF800000;
    cv[4]=32'h7F800000; cv[5]=32'hFF800000;
    cv[6]=32'h7FC00000; cv[7]=32'h7F800001;
    cv[8]=32'h00000001; cv[9]=32'h007FFFFF;
    cv[10]=32'h00800000; cv[11]=32'h7F7FFFFF;
    cv[12]=32'h40000000; cv[13]=32'hC0000000;
    cv[14]=32'h3F000000; cv[15]=32'h33800000;
    cv[16]=32'h4B7FFFFF; cv[17]=32'h4B800000;
    cv[18]=32'h00400000; cv[19]=32'h80400000;
    cv[20]=32'h3FFFFFFF; cv[21]=32'h3F800001;
    cv[22]=32'h7E800000; cv[23]=32'h01000000;
  end

  task fail(input string nm, input logic [31:0] x, input logic [31:0] y, input logic [31:0] gv, input logic [31:0] rv);
    errors = errors + 1;
    if (errors <= 25) $display("%s FAIL a=%h b=%h got=%h ref=%h", nm, x, y, gv, rv);
  endtask

  task check_mul(input logic [31:0] x, input logic [31:0] y);
    g = arch_f32_mul(x, y); r = dpi_mul(x, y);
    if (g !== r) fail("MUL", x, y, g, r);
  endtask
  task check_add(input logic [31:0] x, input logic [31:0] y);
    g = arch_f32_add(x, y); r = dpi_add(x, y);
    if (g !== r) fail("ADD", x, y, g, r);
    g = arch_f32_sub(x, y); r = dpi_sub(x, y);
    if (g !== r) fail("SUB", x, y, g, r);
  endtask
  task check_fma(input logic [31:0] x, input logic [31:0] y, input logic [31:0] z);
    g = arch_fma_f32(x, y, z); r = dpi_fma(x, y, z);
    if (g !== r) begin errors=errors+1; if(errors<=25) $display("FMA FAIL a=%h b=%h c=%h got=%h ref=%h", x, y, z, g, r); end
  endtask
  task check_cmp(input logic [31:0] x, input logic [31:0] y);
    if (arch_f32_eq(x,y) !== (dpi_eq(x,y) != 0)) fail("EQ", x, y, {31'b0,arch_f32_eq(x,y)}, {31'b0,(dpi_eq(x,y)!=0)});
    if (arch_f32_ne(x,y) !== (dpi_ne(x,y) != 0)) fail("NE", x, y, 0, 0);
    if (arch_f32_lt(x,y) !== (dpi_lt(x,y) != 0)) fail("LT", x, y, 0, 0);
    if (arch_f32_le(x,y) !== (dpi_le(x,y) != 0)) fail("LE", x, y, 0, 0);
    if (arch_f32_gt(x,y) !== (dpi_gt(x,y) != 0)) fail("GT", x, y, 0, 0);
    if (arch_f32_ge(x,y) !== (dpi_ge(x,y) != 0)) fail("GE", x, y, 0, 0);
  endtask
  integer NW [0:5] = '{8, 16, 24, 32, 53, 64};
  task check_conv(input logic [31:0] x, input logic [63:0] iv);
    integer kk, nn;
    logic [63:0] gs, rs;
    if (arch_i64_to_f32(iv) !== dpi_s2f(iv)) fail("S2F", iv[63:32], iv[31:0], arch_i64_to_f32(iv), dpi_s2f(iv));
    if (arch_u64_to_f32(iv) !== dpi_u2f(iv)) fail("U2F", iv[63:32], iv[31:0], arch_u64_to_f32(iv), dpi_u2f(iv));
    for (kk = 0; kk < 6; kk = kk + 1) begin
      nn = NW[kk];
      gs = arch_f32_to_sint(x, nn); rs = dpi_f2s(x, nn);
      if (gs !== rs) begin errors=errors+1; if(errors<=25) $display("F2S n=%0d x=%h got=%h ref=%h", nn, x, gs, rs); end
      gs = arch_f32_to_uint(x, nn); rs = dpi_f2u(x, nn);
      if (gs !== rs) begin errors=errors+1; if(errors<=25) $display("F2U n=%0d x=%h got=%h ref=%h", nn, x, gs, rs); end
    end
  endtask
  task check_bf16(input logic [15:0] x, input logic [15:0] y, input logic [15:0] z);
    logic [31:0] rb;
    rb = dpi_bf16_add(x,y); if (arch_bf16_add(x,y) !== rb[15:0]) begin errors=errors+1; if(errors<=25) $display("BF16ADD %h %h g=%h r=%h",x,y,arch_bf16_add(x,y),rb[15:0]); end
    rb = dpi_bf16_sub(x,y); if (arch_bf16_sub(x,y) !== rb[15:0]) begin errors=errors+1; if(errors<=25) $display("BF16SUB %h %h",x,y); end
    rb = dpi_bf16_mul(x,y); if (arch_bf16_mul(x,y) !== rb[15:0]) begin errors=errors+1; if(errors<=25) $display("BF16MUL %h %h",x,y); end
    rb = dpi_bf16_fma(x,y,z); if (arch_fma_bf16(x,y,z) !== rb[15:0]) begin errors=errors+1; if(errors<=25) $display("BF16FMA %h %h %h",x,y,z); end
  endtask
  initial begin
    #1;
    for (i = 0; i < NC; i = i + 1)
      for (j = 0; j < NC; j = j + 1) begin
        check_mul(cv[i], cv[j]);
        check_add(cv[i], cv[j]);
        check_cmp(cv[i], cv[j]);
        check_conv(cv[i], {cv[i], cv[j]});
        check_bf16(cv[i][31:16], cv[j][31:16], cv[i][15:0]);
        for (int kk = 0; kk < NC; kk = kk + 1) check_fma(cv[i], cv[j], cv[kk]);
      end

    for (i = 0; i < 300000; i = i + 1) begin
      a = {$urandom}; b = {$urandom}; c = {$urandom};
      check_mul(a, b);
      check_add(a, b);
      check_cmp(a, b);
      check_fma(a, b, c);
      check_conv(a, {b, c});
      check_bf16(a[31:16], a[15:0], b[31:16]);
      if (i[0]) begin  // exponent-near-equal (cancellation-prone) pairs
        b = (a & 32'h7F800000) | ({1'b0,$urandom} & 32'h807FFFFF);
        check_add(a, b);
        check_fma(a, b, c);
      end
    end

    if (errors == 0) $display("ARCH_FP_RTL_DIFF: ALL PASS");
    else $display("ARCH_FP_RTL_DIFF: FAILS=%0d", errors);
    $finish;
  end
endmodule
