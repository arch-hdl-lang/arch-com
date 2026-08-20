// arch#650 regression: (a - b)[i] behavioral check under Icarus.
module tb;
  logic [7:0] a, b;
  logic [2:0] i;
  logic y;

  IdxArithHoist dut(.a(a), .b(b), .i(i), .y(y));

  initial begin
    a = 8'd5; b = 8'd3; i = 3'd1; #1; // a-b=2 (0b010), bit1=1
    if (y !== 1'b1) begin
      $display("FAIL case1: expected y=1, got y=%b", y);
      $finish(1);
    end
    a = 8'd200; b = 8'd7; i = 3'd0; #1; // a-b=193 (0b11000001), bit0=1
    if (y !== 1'b1) begin
      $display("FAIL case2: expected y=1, got y=%b", y);
      $finish(1);
    end
    a = 8'd10; b = 8'd10; i = 3'd0; #1; // a-b=0, bit0=0
    if (y !== 1'b0) begin
      $display("FAIL case3: expected y=0, got y=%b", y);
      $finish(1);
    end
    $display("PASS");
    $finish(0);
  end
endmodule
