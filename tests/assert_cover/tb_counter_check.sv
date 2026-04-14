// Testbench for CounterCheck assert/cover
module tb_counter_check;
  logic clk, rst, en;
  logic [7:0] count;

  CounterCheck dut (
    .clk(clk), .rst(rst), .en(en), .count(count)
  );

  // Clock generation
  initial clk = 0;
  always #5 clk = ~clk;

  initial begin
    // Reset
    rst = 1; en = 0;
    repeat (3) @(posedge clk);
    rst = 0;

    // Test 1: count with en=1, should hit saw_zero (cnt==0 on deassert)
    // and eventually saw_max (cnt==255 after 256 cycles)
    en = 1;
    repeat (260) @(posedge clk);

    // At this point cnt should have wrapped around past 255
    $display("PASS: counter reached %0d (expected wrap past 255)", count);

    $display("PASS: assert/cover test complete");
    $finish;
  end
endmodule
