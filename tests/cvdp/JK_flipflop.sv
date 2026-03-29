module JK_flipflop (
  input logic i_clk,
  input logic i_rst_b,
  input logic i_J,
  input logic i_K,
  output logic o_Q,
  output logic o_Q_b
);

  always_ff @(posedge i_clk or negedge i_rst_b) begin
    if ((!i_rst_b)) begin
      o_Q <= 1'b0;
      o_Q_b <= 1'b1;
    end else begin
      if (i_J & i_K) begin
        // Toggle
        o_Q <= ~o_Q;
        o_Q_b <= ~o_Q_b;
      end else if (i_J) begin
        o_Q <= 1'b1;
        o_Q_b <= 1'b0;
      end else if (i_K) begin
        o_Q <= 1'b0;
        o_Q_b <= 1'b1;
      end
      // else: hold
    end
  end

endmodule

