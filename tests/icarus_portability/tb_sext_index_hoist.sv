// arch#827 B1 regression: `.sext<N>()` on a runtime-indexed `Vec` element
// behavioral check under Icarus. Pre-fix this design failed to even
// *compile* on iverilog ("reference to a wire or reg... not allowed in a
// constant expression"); this confirms the hoisted form also computes the
// same values ARCH's own semantics dictate.
// din = {5, -1, -128, 127} (elements 0..3).
module tb;
  logic [1:0] sel;
  logic signed [3:0][7:0] din;
  logic signed [11:0] y_runtime_idx, y_const_idx;

  SextIndexHoist dut(.sel(sel), .din(din),
                      .y_runtime_idx(y_runtime_idx), .y_const_idx(y_const_idx));

  task check(input [1:0] s, input signed [11:0] exp_runtime);
    begin
      sel = s; #1;
      if (y_runtime_idx !== exp_runtime) begin
        $display("FAIL sel=%0d: y_runtime_idx=%0d expect=%0d", s, y_runtime_idx, exp_runtime);
        $finish(1);
      end
      // y_const_idx always reads din[2] regardless of sel.
      if (y_const_idx !== -12'sd128) begin
        $display("FAIL sel=%0d: y_const_idx=%0d expect=-128", s, y_const_idx);
        $finish(1);
      end
    end
  endtask

  initial begin
    din[0] = 8'sd5;
    din[1] = -8'sd1;
    din[2] = 8'sh80;   // -128
    din[3] = 8'sd127;

    check(2'd0, 12'sd5);
    check(2'd1, -12'sd1);
    check(2'd2, -12'sd128);
    check(2'd3, 12'sd127);

    $display("PASS");
    $finish(0);
  end
endmodule
