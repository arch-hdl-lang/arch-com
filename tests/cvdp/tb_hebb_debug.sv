module tb_hebb_debug;
  reg clk, rst, start;
  reg signed [3:0] a, b;
  reg [1:0] gate_select;
  wire signed [3:0] w1, w2, bias;
  wire [3:0] present_state, next_state;

  hebb_gates dut(.*);

  initial clk = 0;
  always #5 clk = ~clk;

  initial begin
    rst = 0; start = 0; a = 0; b = 0; gate_select = 0;
    #10;
    rst = 1;
    #10;

    start = 1;
    gate_select = 2'b00;

    a = 1; b = 1;
    #60;
    $display("T=%0t AND(1,1): w1=%0d w2=%0d bias=%0d ps=%0d iter=%0d", $time, w1, w2, bias, present_state, dut.iter);

    a = 1; b = -1;
    #60;
    $display("T=%0t AND(1,-1): w1=%0d w2=%0d bias=%0d ps=%0d iter=%0d", $time, w1, w2, bias, present_state, dut.iter);

    a = -1; b = 1;
    #60;
    $display("T=%0t AND(-1,1): w1=%0d w2=%0d bias=%0d ps=%0d iter=%0d", $time, w1, w2, bias, present_state, dut.iter);

    a = -1; b = -1;
    #70;
    $display("T=%0t AND(-1,-1): w1=%0d w2=%0d bias=%0d ps=%0d iter=%0d", $time, w1, w2, bias, present_state, dut.iter);

    $finish;
  end
endmodule
