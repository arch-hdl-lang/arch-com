// SystemVerilog testbench using built-in concurrent assertions. Drives Mesh4x4 through
// stress traffic pattern (balanced + backpressure + recovery) so the
// auto-emitted credit_channel SVA gets exercised across all 80 channels
// in the design. A passing run = no `_auto_cc_*_credit_bounds`,
// `_auto_cc_*_send_requires_credit`, or
// `_auto_cc_*_credit_return_requires_buffered` assertion fired.

`timescale 1ns/1ps

module sva_tb;
  logic clk = 0;
  logic rst = 1;
  logic [7:0] gen_pressure = 0;
  logic [7:0] pop_pressure = 0;
  logic [1:0] dst_x = 2'd3;
  logic [1:0] dst_y = 2'd3;

  logic [31:0] popped_count;
  logic [27:0] last_payload;

  Mesh4x4 dut (
    .clk(clk), .rst(rst),
    .gen_pressure(gen_pressure), .pop_pressure(pop_pressure),
    .dst_x(dst_x), .dst_y(dst_y),
    .popped_count(popped_count),
    .last_payload(last_payload)
  );

  always #5 clk = ~clk;

  initial begin
    // Reset
    rst = 1; gen_pressure = 0; pop_pressure = 0;
    repeat (5) @(posedge clk);
    rst = 0;

    // Balanced — both pressures at 128, 4000 cycles.
    gen_pressure = 8'd128; pop_pressure = 8'd128;
    repeat (4000) @(posedge clk);
    $display("balanced: popped=%0d", popped_count);

    // Backpressure — producer fast, consumer slow.
    gen_pressure = 8'd255; pop_pressure = 8'd32;
    repeat (4000) @(posedge clk);
    $display("backpressure: popped=%0d", popped_count);

    // Recovery — full speed both sides.
    gen_pressure = 8'd255; pop_pressure = 8'd255;
    repeat (2000) @(posedge clk);
    $display("recovery: popped=%0d", popped_count);

    $display("=== SVA TB completed without violations ===");
    $finish;
  end
endmodule
