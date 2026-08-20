// arch#827 P4.1 loop-var regression: `.sext<N>()` on a receiver that
// references a live runtime `for`-loop iterator, behavioral check under
// Icarus. Confirms the in-loop hoist split (declare at loop top, assign
// in place) computes the same values as the non-loop hoist.
// din = {5, -1, -128, 127} (elements 0..3).
module tb;
  logic signed [3:0][7:0] din;
  logic signed [3:0][11:0] dout;

  SextLoopVarHoist dut(.din(din), .dout(dout));

  initial begin
    din[0] = 8'sd5;
    din[1] = -8'sd1;
    din[2] = 8'sh80;   // -128
    din[3] = 8'sd127;
    #1;
    if (dout[0] !== 12'sd5) begin
      $display("FAIL dout[0]: got=%0d expect=5", dout[0]);
      $finish(1);
    end
    if (dout[1] !== -12'sd1) begin
      $display("FAIL dout[1]: got=%0d expect=-1", dout[1]);
      $finish(1);
    end
    if (dout[2] !== -12'sd128) begin
      $display("FAIL dout[2]: got=%0d expect=-128", dout[2]);
      $finish(1);
    end
    if (dout[3] !== 12'sd127) begin
      $display("FAIL dout[3]: got=%0d expect=127", dout[3]);
      $finish(1);
    end
    $display("PASS");
    $finish(0);
  end
endmodule
