`timescale 1ns/1ns
module tb_bst_debug;
  parameter DATA_WIDTH = 6;
  parameter ARRAY_SIZE = 4;

  reg clk, reset, start;
  reg [ARRAY_SIZE*DATA_WIDTH-1:0] data_in;
  wire [ARRAY_SIZE*DATA_WIDTH-1:0] sorted_out;
  wire done;

  binary_search_tree_sort #(.DATA_WIDTH(DATA_WIDTH), .ARRAY_SIZE(ARRAY_SIZE)) dut (
    .clk(clk), .reset(reset), .data_in(data_in), .start(start),
    .sorted_out(sorted_out), .done(done)
  );

  always #5 clk = ~clk;

  integer cycle;
  initial begin
    $dumpfile("dump.vcd");
    $dumpvars(0, tb_bst_debug);

    clk = 0; reset = 1; start = 0; data_in = 0;

    // Reset for 5 cycles
    repeat(5) @(posedge clk);
    reset = 0;
    @(posedge clk);

    // Test: [49, 53, 5, 33] -> sorted: [5, 33, 49, 53]
    // Pack: index 0 = 49, index 1 = 53, index 2 = 5, index 3 = 33
    data_in = {6'd33, 6'd5, 6'd53, 6'd49};  // MSB first
    @(posedge clk);
    start = 1;
    @(posedge clk);
    start = 0;

    cycle = 0;
    while (!done) begin
      @(posedge clk);
      cycle = cycle + 1;
      if (cycle > 200) begin
        $display("TIMEOUT");
        $finish;
      end
    end

    $display("Done after %0d cycles", cycle);
    $display("sorted_out = %h", sorted_out);
    // Extract individual values
    $display("out[0] = %0d", sorted_out[DATA_WIDTH-1:0]);
    $display("out[1] = %0d", sorted_out[2*DATA_WIDTH-1:DATA_WIDTH]);
    $display("out[2] = %0d", sorted_out[3*DATA_WIDTH-1:2*DATA_WIDTH]);
    $display("out[3] = %0d", sorted_out[4*DATA_WIDTH-1:3*DATA_WIDTH]);

    @(posedge clk);
    $display("After 1 cycle: done=%b sorted_out=%h", done, sorted_out);

    $finish;
  end
endmodule
